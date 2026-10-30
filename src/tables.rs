//! Tables de codes à longueur variable de la norme (ISO 13818-2, annexe B),
//! lues d'un coup par une table indexée sur les bits suivants.

use std::sync::OnceLock;

use crate::bits::Bits;

pub struct Vlc {
    bits: u32,
    /// Valeur et longueur du code ; longueur 0 : code invalide.
    table: Vec<(i16, u8)>,
}

impl Vlc {
    /// `codes` : (code, longueur, valeur).
    fn new(codes: impl IntoIterator<Item = (u32, u8, i16)>) -> Self {
        let codes: Vec<_> = codes.into_iter().collect();
        let bits = codes.iter().map(|c| u32::from(c.1)).max().unwrap();
        let mut table = vec![(0, 0); 1 << bits];
        for (code, len, value) in codes {
            let free = bits - u32::from(len);
            let first = (code << free) as usize;
            table[first..first + (1 << free)].fill((value, len));
        }
        Self { bits, table }
    }

    #[inline]
    pub fn read(&self, b: &mut Bits) -> Option<i16> {
        let (value, len) = self.table[b.peek(self.bits) as usize];
        if len == 0 {
            return None;
        }
        b.skip(u32::from(len));
        Some(value)
    }
}

/// Types de macrobloc : drapeaux.
pub const QUANT: i16 = 1;
pub const FORWARD: i16 = 2;
pub const BACKWARD: i16 = 4;
pub const PATTERN: i16 = 8;
pub const INTRA: i16 = 16;

/// Incrément d'adresse de macrobloc : valeurs spéciales.
pub const MBA_ESCAPE: i16 = 0;
pub const MBA_STUFFING: i16 = -1;

/// Coefficients : valeurs spéciales, sinon `course << 8 | niveau`.
pub const DCT_EOB: i16 = -1;
pub const DCT_ESCAPE: i16 = -2;

pub struct Tables {
    pub mba: Vlc,
    /// Types de macrobloc des images I, P, B.
    pub mb_type: [Vlc; 3],
    pub cbp: Vlc,
    /// Code de mouvement, en valeur absolue (le signe suit).
    pub motion: Vlc,
    pub dmv: Vlc,
    pub dc_luma: Vlc,
    pub dc_chroma: Vlc,
    /// Tables B.14 et B.15 des coefficients (sans le bit de signe).
    pub dct: [Vlc; 2],
}

pub fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(build)
}

/// Code écrit en binaire (« 0001 1 ») : (code, longueur).
fn bin(s: &str) -> (u32, u8) {
    s.chars().filter(|c| *c != ' ').fold((0, 0), |(c, l), d| (c << 1 | u32::from(d == '1'), l + 1))
}

fn from_bin(codes: &[(&str, i16)]) -> Vlc {
    Vlc::new(codes.iter().map(|&(s, v)| {
        let (c, l) = bin(s);
        (c, l, v)
    }))
}

/// B.1 : incréments 1 à 33, puis l'échappement (+33) et le bourrage (MPEG-1).
const MBA: [(u32, u8); 35] = [
    (0x1, 1),
    (0x3, 3),
    (0x2, 3),
    (0x3, 4),
    (0x2, 4),
    (0x3, 5),
    (0x2, 5),
    (0x7, 7),
    (0x6, 7),
    (0xb, 8),
    (0xa, 8),
    (0x9, 8),
    (0x8, 8),
    (0x7, 8),
    (0x6, 8),
    (0x17, 10),
    (0x16, 10),
    (0x15, 10),
    (0x14, 10),
    (0x13, 10),
    (0x12, 10),
    (0x23, 11),
    (0x22, 11),
    (0x21, 11),
    (0x20, 11),
    (0x1f, 11),
    (0x1e, 11),
    (0x1d, 11),
    (0x1c, 11),
    (0x1b, 11),
    (0x1a, 11),
    (0x19, 11),
    (0x18, 11),
    (0x8, 11),
    (0xf, 11),
];

/// B.9 : code de chaque motif de blocs codés (indice = motif).
const CBP: [(u32, u8); 64] = [
    (0x1, 9),
    (0xb, 5),
    (0x9, 5),
    (0xd, 6),
    (0xd, 4),
    (0x17, 7),
    (0x13, 7),
    (0x1f, 8),
    (0xc, 4),
    (0x16, 7),
    (0x12, 7),
    (0x1e, 8),
    (0x13, 5),
    (0x1b, 8),
    (0x17, 8),
    (0x13, 8),
    (0xb, 4),
    (0x15, 7),
    (0x11, 7),
    (0x1d, 8),
    (0x11, 5),
    (0x19, 8),
    (0x15, 8),
    (0x11, 8),
    (0xf, 6),
    (0xf, 8),
    (0xd, 8),
    (0x3, 9),
    (0xf, 5),
    (0xb, 8),
    (0x7, 8),
    (0x7, 9),
    (0xa, 4),
    (0x14, 7),
    (0x10, 7),
    (0x1c, 8),
    (0xe, 6),
    (0xe, 8),
    (0xc, 8),
    (0x2, 9),
    (0x10, 5),
    (0x18, 8),
    (0x14, 8),
    (0x10, 8),
    (0xe, 5),
    (0xa, 8),
    (0x6, 8),
    (0x6, 9),
    (0x12, 5),
    (0x1a, 8),
    (0x16, 8),
    (0x12, 8),
    (0xd, 5),
    (0x9, 8),
    (0x5, 8),
    (0x5, 9),
    (0xc, 5),
    (0x8, 8),
    (0x4, 8),
    (0x4, 9),
    (0x7, 3),
    (0xa, 5),
    (0x8, 5),
    (0xc, 6),
];

/// B.10 : codes de mouvement 0 à 16.
const MOTION: [(u32, u8); 17] = [
    (0x1, 1),
    (0x1, 2),
    (0x1, 3),
    (0x1, 4),
    (0x3, 6),
    (0x5, 7),
    (0x4, 7),
    (0x3, 7),
    (0xb, 9),
    (0xa, 9),
    (0x9, 9),
    (0x11, 10),
    (0x10, 10),
    (0xf, 10),
    (0xe, 10),
    (0xd, 10),
    (0xc, 10),
];

/// B.12 et B.13 : taille du différentiel continu 0 à 11.
const DC_LUMA: [(u32, u8); 12] =
    [(0x4, 3), (0x0, 2), (0x1, 2), (0x5, 3), (0x6, 3), (0xe, 4), (0x1e, 5), (0x3e, 6), (0x7e, 7), (0xfe, 8), (0x1fe, 9), (0x1ff, 9)];
const DC_CHROMA: [(u32, u8); 12] =
    [(0x0, 2), (0x1, 2), (0x2, 2), (0x6, 3), (0xe, 4), (0x1e, 5), (0x3e, 6), (0x7e, 7), (0xfe, 8), (0x1fe, 9), (0x3fe, 10), (0x3ff, 10)];

/// Course et niveau des 111 coefficients des tables B.14 et B.15 (même ordre).
const RUN: [u8; 111] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
    13, 13, 14, 14, 15, 15, 16, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
];
const LEVEL: [u8; 111] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36,
    37, 38, 39, 40, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 1, 2, 3, 4, 5, 1, 2, 3, 4, 1, 2, 3, 1, 2, 3, 1, 2, 3, 1,
    2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];

/// B.14 : codes des 111 coefficients, puis échappement et fin de bloc.
const DCT_ZERO: [(u32, u8); 113] = [
    (0x3, 2),
    (0x4, 4),
    (0x5, 5),
    (0x6, 7),
    (0x26, 8),
    (0x21, 8),
    (0xa, 10),
    (0x1d, 12),
    (0x18, 12),
    (0x13, 12),
    (0x10, 12),
    (0x1a, 13),
    (0x19, 13),
    (0x18, 13),
    (0x17, 13),
    (0x1f, 14),
    (0x1e, 14),
    (0x1d, 14),
    (0x1c, 14),
    (0x1b, 14),
    (0x1a, 14),
    (0x19, 14),
    (0x18, 14),
    (0x17, 14),
    (0x16, 14),
    (0x15, 14),
    (0x14, 14),
    (0x13, 14),
    (0x12, 14),
    (0x11, 14),
    (0x10, 14),
    (0x18, 15),
    (0x17, 15),
    (0x16, 15),
    (0x15, 15),
    (0x14, 15),
    (0x13, 15),
    (0x12, 15),
    (0x11, 15),
    (0x10, 15),
    (0x3, 3),
    (0x6, 6),
    (0x25, 8),
    (0xc, 10),
    (0x1b, 12),
    (0x16, 13),
    (0x15, 13),
    (0x1f, 15),
    (0x1e, 15),
    (0x1d, 15),
    (0x1c, 15),
    (0x1b, 15),
    (0x1a, 15),
    (0x19, 15),
    (0x13, 16),
    (0x12, 16),
    (0x11, 16),
    (0x10, 16),
    (0x5, 4),
    (0x4, 7),
    (0xb, 10),
    (0x14, 12),
    (0x14, 13),
    (0x7, 5),
    (0x24, 8),
    (0x1c, 12),
    (0x13, 13),
    (0x6, 5),
    (0xf, 10),
    (0x12, 12),
    (0x7, 6),
    (0x9, 10),
    (0x12, 13),
    (0x5, 6),
    (0x1e, 12),
    (0x14, 16),
    (0x4, 6),
    (0x15, 12),
    (0x7, 7),
    (0x11, 12),
    (0x5, 7),
    (0x11, 13),
    (0x27, 8),
    (0x10, 13),
    (0x23, 8),
    (0x1a, 16),
    (0x22, 8),
    (0x19, 16),
    (0x20, 8),
    (0x18, 16),
    (0xe, 10),
    (0x17, 16),
    (0xd, 10),
    (0x16, 16),
    (0x8, 10),
    (0x15, 16),
    (0x1f, 12),
    (0x1a, 12),
    (0x19, 12),
    (0x17, 12),
    (0x16, 12),
    (0x1f, 13),
    (0x1e, 13),
    (0x1d, 13),
    (0x1c, 13),
    (0x1b, 13),
    (0x1f, 16),
    (0x1e, 16),
    (0x1d, 16),
    (0x1c, 16),
    (0x1b, 16),
    (0x1, 6),
    (0x2, 2),
];

/// B.15 : la table des blocs intra quand `intra_vlc_format` vaut 1.
const DCT_ONE: [(u32, u8); 113] = [
    (0x02, 2),
    (0x06, 3),
    (0x07, 4),
    (0x1c, 5),
    (0x1d, 5),
    (0x05, 6),
    (0x04, 6),
    (0x7b, 7),
    (0x7c, 7),
    (0x23, 8),
    (0x22, 8),
    (0xfa, 8),
    (0xfb, 8),
    (0xfe, 8),
    (0xff, 8),
    (0x1f, 14),
    (0x1e, 14),
    (0x1d, 14),
    (0x1c, 14),
    (0x1b, 14),
    (0x1a, 14),
    (0x19, 14),
    (0x18, 14),
    (0x17, 14),
    (0x16, 14),
    (0x15, 14),
    (0x14, 14),
    (0x13, 14),
    (0x12, 14),
    (0x11, 14),
    (0x10, 14),
    (0x18, 15),
    (0x17, 15),
    (0x16, 15),
    (0x15, 15),
    (0x14, 15),
    (0x13, 15),
    (0x12, 15),
    (0x11, 15),
    (0x10, 15),
    (0x02, 3),
    (0x06, 5),
    (0x79, 7),
    (0x27, 8),
    (0x20, 8),
    (0x16, 13),
    (0x15, 13),
    (0x1f, 15),
    (0x1e, 15),
    (0x1d, 15),
    (0x1c, 15),
    (0x1b, 15),
    (0x1a, 15),
    (0x19, 15),
    (0x13, 16),
    (0x12, 16),
    (0x11, 16),
    (0x10, 16),
    (0x05, 5),
    (0x07, 7),
    (0xfc, 8),
    (0x0c, 10),
    (0x14, 13),
    (0x07, 5),
    (0x26, 8),
    (0x1c, 12),
    (0x13, 13),
    (0x06, 6),
    (0xfd, 8),
    (0x12, 12),
    (0x07, 6),
    (0x04, 9),
    (0x12, 13),
    (0x06, 7),
    (0x1e, 12),
    (0x14, 16),
    (0x04, 7),
    (0x15, 12),
    (0x05, 7),
    (0x11, 12),
    (0x78, 7),
    (0x11, 13),
    (0x7a, 7),
    (0x10, 13),
    (0x21, 8),
    (0x1a, 16),
    (0x25, 8),
    (0x19, 16),
    (0x24, 8),
    (0x18, 16),
    (0x05, 9),
    (0x17, 16),
    (0x07, 9),
    (0x16, 16),
    (0x0d, 10),
    (0x15, 16),
    (0x1f, 12),
    (0x1a, 12),
    (0x19, 12),
    (0x17, 12),
    (0x16, 12),
    (0x1f, 13),
    (0x1e, 13),
    (0x1d, 13),
    (0x1c, 13),
    (0x1b, 13),
    (0x1f, 16),
    (0x1e, 16),
    (0x1d, 16),
    (0x1c, 16),
    (0x1b, 16),
    (0x01, 6),
    (0x06, 4),
];

fn indexed(codes: &[(u32, u8)], value: impl Fn(usize) -> i16) -> Vlc {
    Vlc::new(codes.iter().enumerate().map(|(i, &(c, l))| (c, l, value(i))))
}

fn dct(codes: &[(u32, u8); 113]) -> Vlc {
    indexed(codes, |i| match i {
        111 => DCT_ESCAPE,
        112 => DCT_EOB,
        _ => i16::from(RUN[i]) << 8 | i16::from(LEVEL[i]),
    })
}

fn build() -> Tables {
    use self::{BACKWARD as B, FORWARD as F, INTRA as I, PATTERN as C, QUANT as Q};
    Tables {
        mba: indexed(&MBA, |i| match i {
            33 => MBA_ESCAPE,
            34 => MBA_STUFFING,
            _ => i as i16 + 1,
        }),
        mb_type: [
            from_bin(&[("1", I), ("01", Q | I)]),
            from_bin(&[("1", F | C), ("01", C), ("001", F), ("0001 1", I), ("0001 0", Q | F | C), ("0000 1", Q | C), ("0000 01", Q | I)]),
            from_bin(&[
                ("10", F | B),
                ("11", F | B | C),
                ("010", B),
                ("011", B | C),
                ("0010", F),
                ("0011", F | C),
                ("0001 1", I),
                ("0001 0", Q | F | B | C),
                ("0000 11", Q | F | C),
                ("0000 10", Q | B | C),
                ("0000 01", Q | I),
            ]),
        ],
        cbp: indexed(&CBP, |i| i as i16),
        motion: indexed(&MOTION, |i| i as i16),
        dmv: from_bin(&[("0", 0), ("10", 1), ("11", -1)]),
        dc_luma: indexed(&DC_LUMA, |i| i as i16),
        dc_chroma: indexed(&DC_CHROMA, |i| i as i16),
        dct: [dct(&DCT_ZERO), dct(&DCT_ONE)],
    }
}

/// Parcours en zigzag et parcours alterné : position naturelle de chaque rang.
pub const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56,
    57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
pub const ALTERNATE: [usize; 64] = [
    0, 8, 16, 24, 1, 9, 2, 10, 17, 25, 32, 40, 48, 56, 57, 49, 41, 33, 26, 18, 3, 11, 4, 12, 19, 27, 34, 42, 50, 58, 35, 43, 51, 59, 20,
    28, 5, 13, 6, 14, 21, 29, 36, 44, 52, 60, 37, 45, 53, 61, 22, 30, 7, 15, 23, 31, 38, 46, 54, 62, 39, 47, 55, 63,
];

/// Matrice intra par défaut, en ordre naturel.
pub const DEFAULT_INTRA: [u8; 64] = [
    8, 16, 19, 22, 26, 27, 29, 34, 16, 16, 22, 24, 27, 29, 34, 37, 19, 22, 26, 27, 29, 34, 34, 38, 22, 22, 26, 27, 29, 34, 37, 40, 22, 26,
    27, 29, 32, 35, 40, 48, 26, 27, 29, 32, 35, 40, 48, 58, 26, 27, 29, 34, 38, 46, 56, 69, 27, 29, 35, 38, 46, 56, 69, 83,
];

/// `quantiser_scale` non linéaire (q_scale_type 1).
pub const NON_LINEAR_SCALE: [u8; 32] =
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 18, 20, 22, 24, 28, 32, 36, 40, 44, 48, 52, 56, 64, 72, 80, 88, 96, 104, 112];

#[cfg(test)]
mod tests {
    use super::*;

    /// Somme de Kraft : chaque table est un code préfixe sans recouvrement.
    fn kraft(codes: &[(u32, u8)]) -> f64 {
        codes.iter().map(|c| 0.5f64.powi(i32::from(c.1))).sum()
    }

    #[test]
    fn prefix_codes() {
        for codes in [&DCT_ZERO[..], &DCT_ONE[..], &MBA[..], &CBP[..], &MOTION[..], &DC_LUMA[..], &DC_CHROMA[..]] {
            assert!(kraft(codes) <= 1.0);
            for (i, a) in codes.iter().enumerate() {
                for b in &codes[i + 1..] {
                    let l = a.1.min(b.1);
                    assert_ne!(a.0 >> (a.1 - l), b.0 >> (b.1 - l), "{a:?} {b:?}");
                }
            }
        }
        let t = tables();
        let mut b = crate::bits::Bits::new(&[0b0000_0001, 0b1101_0000]);
        assert_eq!(t.dct[0].read(&mut b), Some(8));
    }
}
