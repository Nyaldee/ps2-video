//! Décodeur vidéo MPEG-1 et MPEG-2 (profil principal, 4:2:0), celui des
//! vidéos de la PS2 (l'IPU de la console ne décode que celui-là). Images et
//! trames, prédiction d'image, de trame, 16×8 et « dual prime », matrices
//! chargées, parcours alterné, échelle non linéaire.
//!
//! Le flux élémentaire arrive par morceaux (`push`), les images sortent dans
//! l'ordre d'affichage (`frame`).

use std::collections::VecDeque;

use crate::bits::Bits;
use crate::idct::idct;
use crate::tables::{
    ALTERNATE, BACKWARD, DCT_EOB, DCT_ESCAPE, DEFAULT_INTRA, FORWARD, INTRA, MBA_ESCAPE, MBA_STUFFING, NON_LINEAR_SCALE, PATTERN, QUANT,
    ZIGZAG, tables,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureType {
    I,
    P,
    B,
}

/// En-tête de séquence (et son extension en MPEG-2).
#[derive(Clone, Debug, PartialEq)]
pub struct Sequence {
    pub width: usize,
    pub height: usize,
    /// Images par seconde : numérateur, dénominateur.
    pub frame_rate: (u32, u32),
    /// `aspect_ratio_information` : 1 pixels carrés, 2 4:3, 3 16:9, 4 2.21:1.
    pub aspect: u8,
    pub mpeg2: bool,
    pub progressive: bool,
    mb_width: usize,
    mb_height: usize,
}

impl Sequence {
    /// Proportions de l'image affichée (largeur / hauteur) : les pixels des
    /// vidéos ne sont pas carrés (640×448 s'affiche en 4:3).
    pub fn display_aspect(&self) -> f64 {
        let (w, h) = (self.width as f64, self.height as f64);
        if self.mpeg2 {
            return match self.aspect {
                2 => 4.0 / 3.0,
                3 => 16.0 / 9.0,
                4 => 2.21,
                _ => w / h,
            };
        }
        // MPEG-1 : rapport hauteur / largeur d'un pixel.
        const PEL: [f64; 15] =
            [0.0, 1.0, 0.6735, 0.7031, 0.7615, 0.8055, 0.8437, 0.8935, 0.9157, 0.9815, 1.0255, 1.0695, 1.0950, 1.1575, 1.2015];
        match PEL.get(usize::from(self.aspect)) {
            Some(&p) if p > 0.0 => w / (h * p),
            _ => w / h,
        }
    }

    fn sizes(&mut self) {
        self.mb_width = self.width.div_ceil(16);
        self.mb_height = if self.progressive { self.height.div_ceil(16) } else { 2 * self.height.div_ceil(32) };
    }
}

/// Une image décodée, en YCbCr 4:2:0.
#[derive(Clone, Debug)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub y: Vec<u8>,
    pub cb: Vec<u8>,
    pub cr: Vec<u8>,
    /// Horodatage du paquet qui portait l'image (90 kHz).
    pub pts: Option<u64>,
    pub kind: PictureType,
    pub progressive: bool,
    pub top_field_first: bool,
    pub repeat_first_field: bool,
}

impl Frame {
    /// Taille des plans de chrominance.
    pub fn chroma_size(&self) -> (usize, usize) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }

    /// En RGBA 8 bits (BT.601, Y de 16 à 235), la chrominance de chaque pixel
    /// prise sur son échantillon, comme la conversion de l'IPU.
    pub fn rgba(&self) -> Vec<u8> {
        let cw = self.chroma_size().0;
        let mut out = Vec::with_capacity(self.width * self.height * 4);
        for row in 0..self.height {
            for col in 0..self.width {
                let c = (row / 2) * cw + col / 2;
                out.extend(rgb(self.y[row * self.width + col], i32::from(self.cb[c]) << 3, i32::from(self.cr[c]) << 3));
            }
        }
        out
    }

    /// En RGBA comme `rgba`, mais la chrominance interpolée entre ses
    /// échantillons, placés comme le veut le MPEG-2 (sur les colonnes
    /// paires, entre deux lignes ; trame par trame si l'image est
    /// entrelacée) : des bords de couleur plus nets, sans rien inventer.
    pub fn rgba_smooth(&self) -> Vec<u8> {
        let (cw, ch) = self.chroma_size();
        let mut out = Vec::with_capacity(self.width * self.height * 4);
        // Une ligne de chrominance filtrée verticalement (×4).
        let mut line = [vec![0; cw], vec![0; cw]];
        for row in 0..self.height {
            // Ligne d'échantillons la plus proche (poids 3) et sa voisine
            // (poids 1) : dans l'image, ou dans la trame de la ligne.
            let (near, far) = if self.progressive {
                let j = row / 2;
                (j, if row % 2 == 0 { j.saturating_sub(1) } else { (j + 1).min(ch - 1) })
            } else {
                let (field, r) = (row % 2, row / 2);
                let j = r / 2;
                let at = |j: usize| (2 * j + field).min(ch - 1);
                (at(j), if r % 2 == 0 { at(j.saturating_sub(1)) } else { at(j + 1) })
            };
            for (dst, plane) in line.iter_mut().zip([&self.cb, &self.cr]) {
                for (x, d) in dst.iter_mut().enumerate() {
                    *d = 3 * i32::from(plane[near * cw + x]) + i32::from(plane[far * cw + x]);
                }
            }
            for col in 0..self.width {
                let (a, b) = (col / 2, col.div_ceil(2).min(cw - 1));
                out.extend(rgb(self.y[row * self.width + col], line[0][a] + line[0][b], line[1][a] + line[1][b]));
            }
        }
        out
    }
}

/// Un pixel RGBA : la luminance, la chrominance multipliée par 8.
fn rgb(y: u8, cb8: i32, cr8: i32) -> [u8; 4] {
    let y = (i32::from(y) - 16) * 76309;
    let (cb, cr) = (cb8 - 1024, cr8 - 1024);
    // Les coefficients (×2¹⁶) s'appliquent à la chrominance ×8 : >> 19.
    let px = |v: i64| ((v + (1 << 18)) >> 19).clamp(0, 255) as u8;
    let y = i64::from(y) << 3;
    let (cb, cr) = (i64::from(cb), i64::from(cr));
    [px(y + 104597 * cr), px(y - 25675 * cb - 53279 * cr), px(y + 132201 * cb), 255]
}

/// Structure d'image : trame du haut, du bas, image entière.
const TOP: u32 = 1;
const FRAME: u32 = 3;

#[derive(Clone, Debug)]
struct Picture {
    kind: PictureType,
    f_code: [[u32; 2]; 2],
    full_pel: [bool; 2],
    dc_precision: u32,
    structure: u32,
    top_field_first: bool,
    frame_pred_frame_dct: bool,
    concealment: bool,
    q_scale_type: bool,
    intra_vlc: bool,
    alternate_scan: bool,
    repeat_first_field: bool,
    progressive_frame: bool,
    pts: Option<u64>,
}

impl Picture {
    fn frame(&self) -> bool {
        self.structure == FRAME
    }

    /// Parité de la trame décodée (0 haut, 1 bas).
    fn parity(&self) -> usize {
        usize::from(self.structure != TOP)
    }
}

/// Image en mémoire, à la taille codée (multiple de 16).
struct Buffer {
    width: usize,
    height: usize,
    y: Vec<u8>,
    c: [Vec<u8>; 2],
    pic: Option<Picture>,
}

impl Buffer {
    fn new(width: usize, height: usize) -> Self {
        let chroma = vec![128; width * height / 4];
        Self { width, height, y: vec![0; width * height], c: [chroma.clone(), chroma], pic: None }
    }
}

/// Décodage de l'image en cours.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Active {
    /// Pas encore commencée (elle commence à la première tranche).
    No,
    /// Ignorée : il manque une image de référence.
    Skip,
    Yes,
}

pub struct Decoder {
    /// Flux en attente, à partir d'un code de début.
    data: Vec<u8>,
    /// Position de `data[0]` dans tout le flux.
    base: u64,
    /// Où reprendre la recherche du code de début suivant.
    searched: usize,
    /// Horodatages : position dans le flux du paquet qui les porte.
    pts: VecDeque<(u64, u64)>,
    seq: Option<Sequence>,
    intra_q: [u8; 64],
    inter_q: [u8; 64],
    pic: Option<Picture>,
    active: Active,
    bufs: Vec<Buffer>,
    /// Références : la plus ancienne, la plus récente.
    refs: [Option<usize>; 2],
    cur: usize,
    /// Structure de la première trame de l'image en cours, en attendant la
    /// seconde.
    first_field: Option<u32>,
    second: bool,
    out: VecDeque<Frame>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            base: 0,
            searched: 0,
            pts: VecDeque::new(),
            seq: None,
            intra_q: DEFAULT_INTRA,
            inter_q: [16; 64],
            pic: None,
            active: Active::No,
            bufs: Vec::new(),
            refs: [None; 2],
            cur: 0,
            first_field: None,
            second: false,
            out: VecDeque::new(),
        }
    }

    /// La séquence, une fois son en-tête lu.
    pub fn sequence(&self) -> Option<&Sequence> {
        self.seq.as_ref()
    }

    /// Ajoute un morceau du flux élémentaire ; `pts` vaut pour la première
    /// image qui commence dans ce morceau.
    pub fn push(&mut self, data: &[u8], pts: Option<u64>) {
        if let Some(pts) = pts {
            self.pts.push_back((self.base + self.data.len() as u64, pts));
        }
        self.data.extend_from_slice(data);
        self.run(false);
    }

    /// Fin du flux : décode ce qui reste et sort la dernière image.
    pub fn flush(&mut self) {
        self.run(true);
        self.end_stream();
    }

    /// Image suivante dans l'ordre d'affichage.
    pub fn frame(&mut self) -> Option<Frame> {
        self.out.pop_front()
    }

    /// Oublie tout (pour reprendre ailleurs dans le flux).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn run(&mut self, eos: bool) {
        let data = std::mem::take(&mut self.data);
        let mut start = match find(&data, 0) {
            Some(s) => s,
            None => {
                // Pas encore de code de début : garde de quoi le compléter.
                let keep = data.len().saturating_sub(3);
                self.base += keep as u64;
                self.data = data[keep..].to_vec();
                return;
            }
        };
        loop {
            match find(&data, (start + 4).max(start + self.searched)) {
                Some(next) => {
                    self.unit(&data[start..next], self.base + start as u64);
                    self.searched = 0;
                    start = next;
                }
                None if eos => {
                    if data.len() >= start + 4 {
                        self.unit(&data[start..], self.base + start as u64);
                    }
                    self.searched = 0;
                    start = data.len();
                    break;
                }
                None => {
                    self.searched = (data.len() - start).saturating_sub(3).max(4);
                    break;
                }
            }
        }
        self.base += start as u64;
        self.data = data[start..].to_vec();
    }

    fn unit(&mut self, unit: &[u8], pos: u64) {
        let body = &unit[4..];
        match unit[3] {
            0x00 => {
                self.end_picture();
                self.picture_header(body, pos);
            }
            code @ 0x01..=0xAF => self.slice(code, body),
            0xB3 => {
                self.end_picture();
                self.sequence_header(body);
            }
            0xB5 => self.extension(body),
            0xB7 => self.end_stream(),
            0xB8 => self.end_picture(),
            _ => {}
        }
    }

    fn sequence_header(&mut self, body: &[u8]) {
        let b = &mut Bits::new(body);
        let width = b.read(12) as usize;
        let height = b.read(12) as usize;
        let aspect = b.read(4) as u8;
        let rate = b.read(4) as usize;
        b.skip(18 + 1 + 10 + 1);
        self.intra_q = if b.bit() { matrix(b) } else { DEFAULT_INTRA };
        self.inter_q = if b.bit() { matrix(b) } else { [16; 64] };
        const RATES: [(u32, u32); 9] = [(0, 1), (24000, 1001), (24, 1), (25, 1), (30000, 1001), (30, 1), (50, 1), (60000, 1001), (60, 1)];
        let mut seq = Sequence {
            width,
            height,
            frame_rate: RATES.get(rate).copied().unwrap_or((0, 1)),
            aspect,
            mpeg2: false,
            progressive: true,
            mb_width: 0,
            mb_height: 0,
        };
        // En MPEG-2, l'extension qui suit complète la séquence.
        seq.sizes();
        self.seq = Some(seq);
    }

    fn extension(&mut self, body: &[u8]) {
        let b = &mut Bits::new(body);
        match b.read(4) {
            1 => {
                let Some(seq) = self.seq.as_mut() else { return };
                b.skip(8);
                seq.mpeg2 = true;
                seq.progressive = b.bit();
                if b.read(2) != 1 {
                    // Seul le 4:2:0 existe sur PS2.
                    self.seq = None;
                    return;
                }
                seq.width = seq.width & 0xFFF | (b.read(2) as usize) << 12;
                seq.height = seq.height & 0xFFF | (b.read(2) as usize) << 12;
                b.skip(12 + 1 + 8 + 1);
                let n = b.read(2) + 1;
                let d = b.read(2) + 1;
                seq.frame_rate = (seq.frame_rate.0 * n, seq.frame_rate.1 * d);
                seq.sizes();
            }
            3 => {
                if b.bit() {
                    self.intra_q = matrix(b);
                }
                if b.bit() {
                    self.inter_q = matrix(b);
                }
            }
            8 => {
                let Some(pic) = self.pic.as_mut() else { return };
                for s in 0..2 {
                    for t in 0..2 {
                        pic.f_code[s][t] = b.read(4);
                    }
                }
                pic.dc_precision = b.read(2);
                pic.structure = b.read(2);
                pic.top_field_first = b.bit();
                pic.frame_pred_frame_dct = b.bit();
                pic.concealment = b.bit();
                pic.q_scale_type = b.bit();
                pic.intra_vlc = b.bit();
                pic.alternate_scan = b.bit();
                pic.repeat_first_field = b.bit();
                b.skip(1);
                pic.progressive_frame = b.bit();
            }
            _ => {}
        }
    }

    fn picture_header(&mut self, body: &[u8], pos: u64) {
        let b = &mut Bits::new(body);
        b.skip(10);
        let kind = match b.read(3) {
            1 => Some(PictureType::I),
            2 => Some(PictureType::P),
            3 => Some(PictureType::B),
            // Images D du MPEG-1 : non prises en charge.
            _ => None,
        };
        b.skip(16);
        let mut f_code = [[0; 2]; 2];
        let mut full_pel = [false; 2];
        for s in 0..usize::from(kind.map_or(0, |k| k as u8)) {
            full_pel[s] = b.bit();
            f_code[s] = [b.read(3); 2];
        }
        let mut pts = None;
        while let Some(&(at, p)) = self.pts.front()
            && at <= pos
        {
            pts = Some(p);
            self.pts.pop_front();
        }
        self.active = Active::No;
        self.pic = kind.map(|kind| Picture {
            kind,
            f_code,
            full_pel,
            dc_precision: 0,
            structure: FRAME,
            top_field_first: false,
            frame_pred_frame_dct: true,
            concealment: false,
            q_scale_type: false,
            intra_vlc: false,
            alternate_scan: false,
            repeat_first_field: false,
            progressive_frame: true,
            pts,
        });
        if self.pic.is_none() {
            self.active = Active::Skip;
        }
    }

    /// Prépare l'image en cours à sa première tranche ; faux s'il faut
    /// l'ignorer.
    fn begin_picture(&mut self) -> bool {
        let (Some(seq), Some(pic)) = (&self.seq, &self.pic) else { return false };
        let (width, height) = (seq.mb_width * 16, seq.mb_height * 16);
        if self.bufs.first().is_none_or(|b| b.width != width || b.height != height) {
            self.bufs = (0..3).map(|_| Buffer::new(width, height)).collect();
            self.refs = [None; 2];
            self.first_field = None;
        }
        if let Some(first) = self.first_field {
            if !pic.frame() && first != pic.structure {
                self.second = true;
                return true;
            }
            // Première trame restée seule : l'image s'arrête là.
            self.complete_frame();
        }
        let pic = self.pic.clone().unwrap();
        let missing = match pic.kind {
            PictureType::I => false,
            PictureType::P => self.refs[1].is_none(),
            PictureType::B => self.refs.contains(&None),
        };
        if missing {
            return false;
        }
        if pic.kind != PictureType::B
            && let Some(r) = self.refs[1]
        {
            self.emit(r);
        }
        let busy = if pic.kind == PictureType::B { self.refs } else { [self.refs[1], None] };
        self.cur = (0..3).find(|&i| !busy.contains(&Some(i))).unwrap();
        if !pic.frame() {
            self.first_field = Some(pic.structure);
        }
        self.second = false;
        self.bufs[self.cur].pic = Some(pic);
        true
    }

    /// Fin des tranches d'une image ou d'une trame.
    fn end_picture(&mut self) {
        if self.active == Active::Yes {
            let first = self.pic.as_ref().is_some_and(|p| !p.frame() && self.first_field == Some(p.structure));
            if !first {
                self.complete_frame();
            }
        }
        self.active = Active::No;
    }

    fn complete_frame(&mut self) {
        self.first_field = None;
        let kind = self.bufs[self.cur].pic.as_ref().map(|p| p.kind);
        if kind == Some(PictureType::B) {
            self.emit(self.cur);
        } else {
            self.refs = [self.refs[1], Some(self.cur)];
        }
    }

    fn end_stream(&mut self) {
        self.end_picture();
        if self.first_field.is_some() {
            self.complete_frame();
        }
        if let Some(r) = self.refs[1] {
            self.emit(r);
        }
        self.refs = [None; 2];
    }

    fn emit(&mut self, i: usize) {
        let (Some(seq), Some(pic)) = (&self.seq, &self.bufs[i].pic) else { return };
        let buf = &self.bufs[i];
        let crop = |plane: &[u8], stride: usize, w: usize, h: usize| {
            plane.chunks(stride).take(h).flat_map(|row| &row[..w]).copied().collect::<Vec<_>>()
        };
        let (w, h) = (seq.width, seq.height);
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        self.out.push_back(Frame {
            width: w,
            height: h,
            y: crop(&buf.y, buf.width, w, h),
            cb: crop(&buf.c[0], buf.width / 2, cw, ch),
            cr: crop(&buf.c[1], buf.width / 2, cw, ch),
            pts: pic.pts,
            kind: pic.kind,
            progressive: pic.progressive_frame,
            top_field_first: pic.top_field_first,
            repeat_first_field: pic.repeat_first_field,
        });
    }

    fn slice(&mut self, code: u8, body: &[u8]) {
        if self.active == Active::No {
            self.active = if self.begin_picture() { Active::Yes } else { Active::Skip };
        }
        if self.active != Active::Yes {
            return;
        }
        let (Some(seq), Some(pic)) = (&self.seq, &self.pic) else { return };
        let mut slice = Slice {
            seq,
            pic,
            intra_q: &self.intra_q,
            inter_q: &self.inter_q,
            bufs: &mut self.bufs,
            cur: self.cur,
            refs: self.refs,
            second: self.second,
            qscale: 0,
            dc: [0; 3],
            pmv: [[[0; 2]; 2]; 2],
            last_dirs: [false; 2],
        };
        // Une erreur perd la fin de la tranche ; la suivante reprend.
        let _ = slice.decode(code, body);
    }
}

/// Premier code de début (00 00 01) à partir de `from`.
fn find(data: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 3 <= data.len() {
        if data[i + 2] > 1 {
            i += 3;
        } else if data[i + 2] == 1 && data[i] == 0 && data[i + 1] == 0 {
            return Some(i);
        } else {
            i += 1;
        }
    }
    None
}

/// Matrice transmise (en zigzag), rendue en ordre naturel.
fn matrix(b: &mut Bits) -> [u8; 64] {
    let mut m = [0; 64];
    for &pos in &ZIGZAG {
        m[pos] = b.read(8) as u8;
    }
    m
}

/// Type de prédiction d'un macrobloc. Dans une image : `Field` deux vecteurs
/// de trame ; dans une trame : `Field` un vecteur, `Half` deux (16×8).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Kind {
    #[default]
    Frame,
    Field,
    Half,
    Dual,
}

#[derive(Clone, Copy, Default)]
struct Motion {
    dirs: [bool; 2],
    kind: Kind,
    /// Vecteurs [r][s][t] en demi-pixels (vertical en lignes de trame pour la
    /// prédiction de trame).
    mv: [[[i32; 2]; 2]; 2],
    /// Trame de référence [r][s] : 0 haut, 1 bas.
    sel: [[usize; 2]; 2],
    dmv: [i32; 2],
}

/// Macrobloc en construction : prédiction puis résidu.
struct Mb {
    y: [u8; 256],
    c: [[u8; 64]; 2],
}

struct Slice<'a> {
    seq: &'a Sequence,
    pic: &'a Picture,
    intra_q: &'a [u8; 64],
    inter_q: &'a [u8; 64],
    bufs: &'a mut Vec<Buffer>,
    cur: usize,
    refs: [Option<usize>; 2],
    second: bool,
    qscale: i32,
    dc: [i32; 3],
    pmv: [[[i32; 2]; 2]; 2],
    /// Directions du macrobloc précédent (reprises par les sautés des images B).
    last_dirs: [bool; 2],
}

impl Slice<'_> {
    fn decode(&mut self, code: u8, body: &[u8]) -> Option<()> {
        let b = &mut Bits::new(body);
        let mbw = self.seq.mb_width;
        let rows = if self.pic.frame() { self.seq.mb_height } else { self.seq.mb_height / 2 };
        let row = usize::from(code) - 1;
        if row >= rows {
            return None;
        }
        self.quantiser(b.read(5));
        while b.bit() {
            b.skip(8);
        }
        self.reset_dc();
        self.pmv = Default::default();
        let mut addr = row * mbw;
        let mut first = true;
        loop {
            let mut inc = 0;
            loop {
                match tables().mba.read(b)? {
                    MBA_STUFFING => {}
                    MBA_ESCAPE => inc += 33,
                    v => {
                        inc += v as usize;
                        break;
                    }
                }
            }
            if first {
                addr += inc - 1;
                first = false;
            } else {
                for _ in 1..inc {
                    addr += 1;
                    self.skipped(addr)?;
                }
                addr += 1;
            }
            if addr >= mbw * rows {
                return None;
            }
            self.macroblock(b, addr)?;
            if b.left() <= 0 || b.peek(23) == 0 {
                return Some(());
            }
        }
    }

    fn quantiser(&mut self, code: u32) {
        self.qscale = if self.pic.q_scale_type { i32::from(NON_LINEAR_SCALE[code as usize]) } else { 2 * code as i32 };
    }

    fn reset_dc(&mut self) {
        self.dc = [1 << (7 + self.pic.dc_precision); 3];
    }

    /// Prédiction sans vecteur des images P : même parité en trame.
    fn zero_motion(&self) -> Motion {
        let p = self.pic.parity();
        Motion {
            dirs: [true, false],
            kind: if self.pic.frame() { Kind::Frame } else { Kind::Field },
            sel: [[p; 2]; 2],
            ..Motion::default()
        }
    }

    fn skipped(&mut self, addr: usize) -> Option<()> {
        self.reset_dc();
        let m = match self.pic.kind {
            PictureType::I => return None,
            PictureType::P => {
                self.pmv = Default::default();
                self.zero_motion()
            }
            // Mêmes directions que le macrobloc précédent, prédiction d'image
            // (de trame dans une trame) aux vecteurs prédits, comme FFmpeg et
            // libmpeg2.
            PictureType::B => {
                let mut m = Motion { dirs: self.last_dirs, ..self.zero_motion() };
                for s in 0..2 {
                    let scale = if self.pic.full_pel[s] { 2 } else { 1 };
                    m.mv[0][s] = self.pmv[0][s].map(|v| v * scale);
                }
                m
            }
        };
        let mut mb = Mb { y: [0; 256], c: [[0; 64]; 2] };
        self.predict(&m, addr, &mut mb);
        self.store(&mb, addr);
        Some(())
    }

    fn macroblock(&mut self, b: &mut Bits, addr: usize) -> Option<()> {
        let t = tables();
        let ty = t.mb_type[self.pic.kind as usize].read(b)?;
        let intra = ty & INTRA != 0;
        let dirs = [ty & FORWARD != 0, ty & BACKWARD != 0];
        let pattern = ty & PATTERN != 0;
        let frame = self.pic.frame();
        let mut m = Motion { dirs, kind: if frame { Kind::Frame } else { Kind::Field }, ..Motion::default() };
        if dirs[0] || dirs[1] {
            if !frame {
                m.kind = [None, Some(Kind::Field), Some(Kind::Half), Some(Kind::Dual)][b.read(2) as usize]?;
            } else if !self.pic.frame_pred_frame_dct {
                m.kind = [None, Some(Kind::Field), Some(Kind::Frame), Some(Kind::Dual)][b.read(2) as usize]?;
            }
        }
        let dct_field = frame && !self.pic.frame_pred_frame_dct && (intra || pattern) && b.bit();
        if ty & QUANT != 0 {
            self.quantiser(b.read(5));
        }
        if intra {
            if self.pic.concealment {
                self.vectors(b, &mut m, 0)?;
                b.skip(1);
            } else {
                self.pmv = Default::default();
            }
        } else {
            self.reset_dc();
            for s in [0, 1].into_iter().filter(|&s| dirs[s]) {
                self.vectors(b, &mut m, s)?;
            }
            if self.pic.kind == PictureType::P && !dirs[0] {
                self.pmv = Default::default();
                m = self.zero_motion();
            }
        }
        let cbp = if pattern {
            t.cbp.read(b)?
        } else if intra {
            63
        } else {
            0
        };
        let mut mb = Mb { y: [0; 256], c: [[0; 64]; 2] };
        if !intra {
            self.predict(&m, addr, &mut mb);
            self.last_dirs = m.dirs;
        }
        for i in 0..6 {
            if cbp & (32 >> i) == 0 {
                continue;
            }
            let mut f = self.block(b, i, intra)?;
            idct(&mut f);
            let (dst, stride, x0, row0, step) = match i {
                0..4 if dct_field => (&mut mb.y[..], 16, (i & 1) * 8, i >> 1, 2),
                0..4 => (&mut mb.y[..], 16, (i & 1) * 8, (i >> 1) * 8, 1),
                _ => (&mut mb.c[i - 4][..], 8, 0, 0, 1),
            };
            for (y, row) in f.chunks_exact(8).enumerate() {
                let line = &mut dst[(row0 + y * step) * stride + x0..][..8];
                for (d, &v) in line.iter_mut().zip(row) {
                    let base = if intra { 0 } else { i32::from(*d) };
                    *d = (base + v).clamp(0, 255) as u8;
                }
            }
        }
        self.store(&mb, addr);
        Some(())
    }

    /// Vecteurs d'une direction (`s` : 0 avant, 1 arrière).
    fn vectors(&mut self, b: &mut Bits, m: &mut Motion, s: usize) -> Option<()> {
        let frame = self.pic.frame();
        let (two, field) = match (frame, m.kind) {
            (true, Kind::Frame) => (false, false),
            (true, Kind::Field) | (false, Kind::Half) => (true, true),
            _ => (false, true),
        };
        let dual = m.kind == Kind::Dual;
        if two {
            for r in 0..2 {
                m.sel[r][s] = usize::from(b.bit());
                self.vector(b, m, r, s, field, false)?;
            }
        } else {
            if field && !dual {
                m.sel[0][s] = usize::from(b.bit());
            }
            self.vector(b, m, 0, s, field, dual)?;
            self.pmv[1][s] = self.pmv[0][s];
        }
        Some(())
    }

    fn vector(&mut self, b: &mut Bits, m: &mut Motion, r: usize, s: usize, field: bool, dual: bool) -> Option<()> {
        let t = tables();
        for c in 0..2 {
            let mut code = i32::from(t.motion.read(b)?);
            if code != 0 && b.bit() {
                code = -code;
            }
            let r_size = self.pic.f_code[s][c].saturating_sub(1);
            let residual = if r_size > 0 && code != 0 { b.read(r_size) as i32 } else { 0 };
            if dual {
                m.dmv[c] = i32::from(t.dmv.read(b)?);
            }
            let f = 1 << r_size;
            let delta = if f == 1 || code == 0 {
                code
            } else {
                let d = (code.abs() - 1) * f + residual + 1;
                if code < 0 { -d } else { d }
            };
            // Vecteur de trame dans une image : prédit et gardé en lignes d'image.
            let half = field && c == 1 && self.pic.frame();
            let pred = if half { self.pmv[r][s][1] >> 1 } else { self.pmv[r][s][c] };
            let mut v = pred + delta;
            if v < -16 * f {
                v += 32 * f;
            } else if v > 16 * f - 1 {
                v -= 32 * f;
            }
            self.pmv[r][s][c] = if half { v * 2 } else { v };
            m.mv[r][s][c] = if self.pic.full_pel[s] { v * 2 } else { v };
        }
        Some(())
    }

    fn block(&mut self, b: &mut Bits, i: usize, intra: bool) -> Option<[i32; 64]> {
        let t = tables();
        let mut f = [0i32; 64];
        let scan = if self.pic.alternate_scan { &ALTERNATE } else { &ZIGZAG };
        let mpeg2 = self.seq.mpeg2;
        let (q, table) = if intra { (self.intra_q, &t.dct[usize::from(self.pic.intra_vlc)]) } else { (self.inter_q, &t.dct[0]) };
        let qscale = self.qscale;
        let mut idx = 0;
        let mut sum = 0;
        let put = |idx: usize, level: i32, f: &mut [i32; 64], sum: &mut i32| {
            let pos = scan[idx];
            let w = i32::from(q[pos]);
            let mut v = (if intra { 2 * level } else { 2 * level + level.signum() }) * w * qscale / 32;
            // MPEG-1 : coefficients rendus impairs.
            if !mpeg2 && v & 1 == 0 {
                v -= v.signum();
            }
            let v = v.clamp(-2048, 2047);
            f[pos] = v;
            *sum += v;
        };
        if intra {
            let c = i.saturating_sub(3);
            let size = if c == 0 { t.dc_luma.read(b)? } else { t.dc_chroma.read(b)? } as u32;
            let diff = match size {
                0 => 0,
                _ => {
                    let v = b.read(size) as i32;
                    if v < 1 << (size - 1) { v - (1 << size) + 1 } else { v }
                }
            };
            self.dc[c] += diff;
            f[0] = self.dc[c] << (3 - self.pic.dc_precision);
            sum = f[0];
            idx = 1;
        } else if b.peek(1) == 1 {
            // Premier coefficient d'un bloc non intra : « 1s » vaut ±1.
            b.skip(1);
            put(0, if b.bit() { -1 } else { 1 }, &mut f, &mut sum);
            idx = 1;
        }
        loop {
            let v = table.read(b)?;
            let (run, level) = match v {
                DCT_EOB => break,
                DCT_ESCAPE => {
                    let run = b.read(6) as usize;
                    let level = if mpeg2 {
                        (b.read(12) as i32) << 20 >> 20
                    } else {
                        match b.read(8) {
                            0 => b.read(8) as i32,
                            128 => b.read(8) as i32 - 256,
                            l => i32::from(l as u8 as i8),
                        }
                    };
                    if level == 0 {
                        return None;
                    }
                    (run, level)
                }
                _ => {
                    let level = i32::from(v & 0xFF);
                    (v as usize >> 8, if b.bit() { -level } else { level })
                }
            };
            idx += run;
            if idx > 63 {
                return None;
            }
            put(idx, level, &mut f, &mut sum);
            idx += 1;
        }
        if mpeg2 && sum & 1 == 0 {
            f[63] ^= 1;
        }
        Some(f)
    }

    /// Image de référence d'une direction, pour la trame `field` s'il y en a.
    fn reference(&self, s: usize, field: Option<usize>) -> &Buffer {
        let r = match (s, self.pic.kind) {
            (0, PictureType::B) => self.refs[0],
            // Seconde trame d'une image P : la trame de parité opposée est la
            // première de l'image en cours.
            (0, _) if self.second && field.is_some_and(|f| f != self.pic.parity()) => None,
            _ => self.refs[1],
        };
        &self.bufs[r.unwrap_or(self.cur)]
    }

    fn predict(&self, m: &Motion, addr: usize, mb: &mut Mb) {
        let mbw = self.seq.mb_width;
        let (x, y) = ((addr % mbw * 16) as i32, (addr / mbw * 16) as i32);
        let mut avg = false;
        for s in (0..2).filter(|&s| m.dirs[s]) {
            let mv = |r: usize| m.mv[r][s];
            match (self.pic.frame(), m.kind) {
                (true, Kind::Frame) => mc(self.reference(s, None), None, x, y, 16, mv(0), mb, 0, 1, avg),
                (true, Kind::Field) => {
                    for r in 0..2 {
                        let f = m.sel[r][s];
                        mc(self.reference(s, Some(f)), Some(f), x, y / 2, 8, mv(r), mb, r, 2, avg);
                    }
                }
                (true, _) => {
                    // Dual prime : chaque trame, moyenne de la même parité et
                    // de l'autre, au vecteur dérivé.
                    let [vx, vy] = mv(0);
                    for r in 0..2 {
                        let near = self.pic.top_field_first == (r == 0);
                        let k = if near { 1 } else { 3 };
                        let e = if r == 0 { -1 } else { 1 };
                        let dv = [dual(vx, k) + m.dmv[0], dual(vy, k) + m.dmv[1] + e];
                        mc(self.reference(0, Some(r)), Some(r), x, y / 2, 8, [vx, vy], mb, r, 2, false);
                        mc(self.reference(0, Some(1 - r)), Some(1 - r), x, y / 2, 8, dv, mb, r, 2, true);
                    }
                }
                (false, Kind::Half) => {
                    for r in 0..2 {
                        let f = m.sel[r][s];
                        mc(self.reference(s, Some(f)), Some(f), x, y + 8 * r as i32, 8, mv(r), mb, 8 * r, 1, avg);
                    }
                }
                (false, Kind::Dual) => {
                    let p = self.pic.parity();
                    let [vx, vy] = mv(0);
                    let e = if p == 0 { -1 } else { 1 };
                    let dv = [dual(vx, 1) + m.dmv[0], dual(vy, 1) + m.dmv[1] + e];
                    mc(self.reference(0, Some(p)), Some(p), x, y, 16, [vx, vy], mb, 0, 1, false);
                    mc(self.reference(0, Some(1 - p)), Some(1 - p), x, y, 16, dv, mb, 0, 1, true);
                }
                (false, _) => {
                    let f = m.sel[0][s];
                    mc(self.reference(s, Some(f)), Some(f), x, y, 16, mv(0), mb, 0, 1, avg);
                }
            }
            avg = true;
        }
    }

    fn store(&mut self, mb: &Mb, addr: usize) {
        let mbw = self.seq.mb_width;
        let (mx, my) = (addr % mbw, addr / mbw);
        let field = (!self.pic.frame()).then(|| self.pic.parity());
        let buf = &mut self.bufs[self.cur];
        let line = |row: usize| field.map_or(row, |p| 2 * row + p);
        for i in 0..16 {
            let at = line(my * 16 + i) * buf.width + mx * 16;
            buf.y[at..at + 16].copy_from_slice(&mb.y[i * 16..][..16]);
        }
        for (plane, c) in buf.c.iter_mut().zip(&mb.c) {
            for i in 0..8 {
                let at = line(my * 8 + i) * buf.width / 2 + mx * 8;
                plane[at..at + 8].copy_from_slice(&c[i * 8..][..8]);
            }
        }
    }
}

/// Vecteur dérivé du dual prime pour une distance de `k` trames.
fn dual(v: i32, k: i32) -> i32 {
    (v * k + i32::from(v > 0)) >> 1
}

/// Prédit `h` lignes de luminance (16 de large) et la chrominance qui va
/// avec, en (x, y) de la référence (image, ou trame `field`), vers les lignes
/// `row`, `row + step`… du macrobloc.
#[allow(clippy::too_many_arguments)]
fn mc(buf: &Buffer, field: Option<usize>, x: i32, y: i32, h: usize, mv: [i32; 2], mb: &mut Mb, row: usize, step: usize, avg: bool) {
    plane(&buf.y, buf.width, buf.height, field, (x, y), (16, h), mv, &mut mb.y, 16, (row, step), avg);
    let cmv = [mv[0] / 2, mv[1] / 2];
    let crow = if step == 2 { row } else { row / 2 };
    for (src, dst) in buf.c.iter().zip(&mut mb.c) {
        plane(src, buf.width / 2, buf.height / 2, field, (x / 2, y / 2), (8, h / 2), cmv, dst, 8, (crow, step), avg);
    }
}

#[allow(clippy::too_many_arguments)]
fn plane(
    src: &[u8],
    width: usize,
    height: usize,
    field: Option<usize>,
    (x, y): (i32, i32),
    (w, h): (usize, usize),
    [dx, dy]: [i32; 2],
    dst: &mut [u8],
    stride: usize,
    (row, step): (usize, usize),
    avg: bool,
) {
    let (base, pitch, rows) = match field {
        Some(p) => (p * width, 2 * width, height / 2),
        None => (0, width, height),
    };
    let (sx, sy) = (x + (dx >> 1), y + (dy >> 1));
    let (maxx, maxy) = (width as i32 - 1, rows as i32 - 1);
    // Hors de l'image (flux abîmé), le bord se répète.
    let at = |px: i32, py: i32| u32::from(src[base + py.clamp(0, maxy) as usize * pitch + px.clamp(0, maxx) as usize]);
    for i in 0..h {
        let py = sy + i as i32;
        let line = &mut dst[(row + i * step) * stride..][..w];
        for (j, d) in line.iter_mut().enumerate() {
            let px = sx + j as i32;
            let p = match (dx & 1, dy & 1) {
                (0, 0) => at(px, py),
                (1, 0) => (at(px, py) + at(px + 1, py) + 1) >> 1,
                (0, _) => (at(px, py) + at(px, py + 1) + 1) >> 1,
                _ => (at(px, py) + at(px + 1, py) + at(px, py + 1) + at(px + 1, py + 1) + 2) >> 2,
            };
            *d = if avg { ((u32::from(*d) + p + 1) >> 1) as u8 } else { p as u8 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, PictureType};

    #[test]
    fn smooth_keeps_flat_colour() {
        let frame = |cb: Vec<u8>| Frame {
            width: 4,
            height: 4,
            y: (0..16).map(|i| 16 + i * 13).collect(),
            cr: vec![90; 4],
            cb,
            pts: None,
            kind: PictureType::I,
            progressive: true,
            top_field_first: false,
            repeat_first_field: false,
        };
        let flat = frame(vec![200; 4]);
        assert_eq!(flat.rgba(), flat.rgba_smooth());
        // Deux colonnes de chrominance : le pixel 0 garde la sienne, le
        // pixel 1 (colonne impaire) prend entre les deux (son bleu entre
        // ceux des pixels 0 et 2).
        let edge = frame(vec![100, 200, 100, 200]);
        let (a, b) = (edge.rgba(), edge.rgba_smooth());
        assert_eq!(a[..4], b[..4]);
        assert!(b[4 + 2] > a[2] && b[4 + 2] < a[8 + 2]);
    }
}
