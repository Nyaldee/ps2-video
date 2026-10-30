/// Lecteur de bits, poids fort d'abord. Au-delà de la fin, il lit des zéros
/// (ce qui ressemble à un code de début : la tranche s'arrête).
pub struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Les `n` bits suivants (1 à 32), sans avancer.
    #[inline]
    pub fn peek(&self, n: u32) -> u32 {
        let byte = self.pos >> 3;
        let word = match self.data.get(byte..byte + 8) {
            Some(b) => u64::from_be_bytes(b.try_into().unwrap()),
            None => (0..8).fold(0, |w, i| w << 8 | u64::from(self.data.get(byte + i).copied().unwrap_or(0))),
        };
        ((word << (self.pos & 7)) >> (64 - n)) as u32
    }

    #[inline]
    pub fn skip(&mut self, n: u32) {
        self.pos += n as usize;
    }

    #[inline]
    pub fn read(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = self.peek(n);
        self.skip(n);
        v
    }

    #[inline]
    pub fn bit(&mut self) -> bool {
        self.read(1) != 0
    }

    /// Bits restants (négatif si la lecture a dépassé la fin).
    pub fn left(&self) -> isize {
        (self.data.len() * 8) as isize - self.pos as isize
    }
}

#[cfg(test)]
mod tests {
    use super::Bits;

    #[test]
    fn read_across_bytes() {
        let mut b = Bits::new(&[0b1010_1100, 0b0101_0011, 0xFF]);
        assert_eq!(b.read(3), 0b101);
        assert_eq!(b.read(7), 0b0110001);
        assert_eq!(b.peek(6), 0b010011);
        b.skip(14);
        assert_eq!(b.left(), 0);
        assert_eq!(b.read(8), 0);
    }
}
