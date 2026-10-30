//! Son des PSS : en-tête « SShd » (format, fréquence, voies, entrelacement),
//! puis « SSbd » et les données, par blocs de `interleave` octets voie après
//! voie. PCM 16 bits petit-boutiste ou ADPCM de la PS2.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Pcm16,
    Adpcm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioInfo {
    pub codec: Codec,
    pub rate: u32,
    pub channels: usize,
    pub interleave: usize,
}

#[derive(Default)]
pub struct AudioDecoder {
    pending: Vec<u8>,
    info: Option<AudioInfo>,
    /// En-tête illisible : le son est ignoré.
    failed: bool,
    /// ADPCM : les deux derniers échantillons de chaque voie.
    history: Vec<(i32, i32)>,
}

impl AudioDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn info(&self) -> Option<AudioInfo> {
        self.info
    }

    /// Décode un morceau ; les échantillons, entrelacés par voie, s'ajoutent
    /// à `out`.
    pub fn push(&mut self, data: &[u8], out: &mut Vec<i16>) {
        if self.failed {
            return;
        }
        self.pending.extend_from_slice(data);
        if self.info.is_none() && !self.header() {
            return;
        }
        let Some(info) = self.info else { return };
        let pending = std::mem::take(&mut self.pending);
        let mut blocks = pending.chunks_exact(info.channels * info.interleave);
        for b in &mut blocks {
            self.block(b, info, out);
        }
        self.pending = blocks.remainder().to_vec();
    }

    fn header(&mut self) -> bool {
        let p = &self.pending;
        let le = |i: usize| u32::from_le_bytes(p[i..i + 4].try_into().unwrap());
        if p.len() < 8 {
            return false;
        }
        if &p[..4] != b"SShd" {
            self.failed = true;
            return false;
        }
        let size = le(4) as usize;
        if p.len() < 16 + size || size < 16 {
            return false;
        }
        let codec = match le(8) {
            0x01 => Codec::Pcm16,
            0x10 => Codec::Adpcm,
            _ => {
                self.failed = true;
                return false;
            }
        };
        let (rate, channels, interleave) = (le(12), le(16) as usize, le(20) as usize);
        if &p[8 + size..12 + size] != b"SSbd" || !(1..=8).contains(&channels) || interleave == 0 || interleave % 16 != 0 {
            self.failed = true;
            return false;
        }
        self.info = Some(AudioInfo { codec, rate, channels, interleave });
        self.history = vec![(0, 0); channels];
        self.pending.drain(..16 + size);
        true
    }

    fn block(&mut self, data: &[u8], info: AudioInfo, out: &mut Vec<i16>) {
        let voices: Vec<Vec<i16>> = data
            .chunks_exact(info.interleave)
            .zip(&mut self.history)
            .map(|(d, h)| match info.codec {
                Codec::Pcm16 => d.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]])).collect(),
                Codec::Adpcm => adpcm(d, h),
            })
            .collect();
        for i in 0..voices[0].len() {
            out.extend(voices.iter().map(|v| v[i]));
        }
    }
}

/// ADPCM de la PS2 : blocs de 16 octets (décalage et filtre, drapeaux,
/// 28 échantillons de 4 bits).
fn adpcm(data: &[u8], (h1, h2): &mut (i32, i32)) -> Vec<i16> {
    const COEFS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];
    let mut out = Vec::with_capacity(data.len() / 16 * 28);
    for block in data.chunks_exact(16) {
        let shift = i32::from(block[0] & 0x0F);
        let (c1, c2) = COEFS[usize::from(block[0] >> 4).min(4)];
        for &byte in &block[2..] {
            for nibble in [byte & 0x0F, byte >> 4] {
                let n = i32::from(nibble as i8) << 28 >> 28;
                let s = (((n << 12) >> shift) + ((*h1 * c1 + *h2 * c2 + 32) >> 6)).clamp(-32768, 32767);
                *h2 = *h1;
                *h1 = s;
                out.push(s as i16);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_header_and_interleave() {
        let mut d = b"SShd".to_vec();
        for v in [0x18u32, 1, 48000, 2, 16, 0, 0] {
            d.extend(v.to_le_bytes());
        }
        d.extend(b"SSbd");
        d.extend(64u32.to_le_bytes());
        // 8 échantillons à gauche, puis 8 à droite.
        d.extend((0..8i16).flat_map(|i| i.to_le_bytes()));
        d.extend((0..8i16).flat_map(|i| (-i).to_le_bytes()));
        let mut a = AudioDecoder::new();
        let mut out = Vec::new();
        a.push(&d[..20], &mut out);
        a.push(&d[20..], &mut out);
        assert_eq!(a.info().map(|i| (i.codec, i.rate, i.channels)), Some((Codec::Pcm16, 48000, 2)));
        assert_eq!(&out[..6], &[0, 0, 1, -1, 2, -2]);
    }
}
