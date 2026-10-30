//! Transformée en cosinus inverse 8×8, en entiers : lignes puis colonnes,
//! coefficients cos(kπ/16)·√2·2¹⁴. C'est le même calcul que l'IDCT
//! « simple » de FFmpeg (`-idct simple`), au bit près : les tests comparent
//! les images décodées octet par octet.

const W1: i32 = 22725;
const W2: i32 = 21407;
const W3: i32 = 19266;
const W4: i32 = 16383;
const W5: i32 = 12873;
const W6: i32 = 8867;
const W7: i32 = 4520;
const ROW_SHIFT: i32 = 11;
const COL_SHIFT: i32 = 20;

/// Le bloc (ordre naturel, ligne par ligne) est transformé sur place.
pub fn idct(block: &mut [i32; 64]) {
    for row in block.chunks_exact_mut(8) {
        if row[1..].iter().all(|&c| c == 0) {
            // Ligne réduite au continu : un simple décalage, comme FFmpeg
            // (le calcul complet arrondirait un peu différemment).
            let dc = (row[0] << 3) as i16 as i32;
            row.fill(dc);
            continue;
        }
        let mut a0 = W4 * row[0] + (1 << (ROW_SHIFT - 1));
        let (mut a1, mut a2, mut a3) = (a0, a0, a0);
        a0 += W2 * row[2] + W4 * row[4] + W6 * row[6];
        a1 += W6 * row[2] - W4 * row[4] - W2 * row[6];
        a2 += -W6 * row[2] - W4 * row[4] + W2 * row[6];
        a3 += -W2 * row[2] + W4 * row[4] - W6 * row[6];
        let b0 = W1 * row[1] + W3 * row[3] + W5 * row[5] + W7 * row[7];
        let b1 = W3 * row[1] - W7 * row[3] - W1 * row[5] - W5 * row[7];
        let b2 = W5 * row[1] - W1 * row[3] + W7 * row[5] + W3 * row[7];
        let b3 = W7 * row[1] - W5 * row[3] + W3 * row[5] - W1 * row[7];
        row[0] = (a0 + b0) >> ROW_SHIFT;
        row[7] = (a0 - b0) >> ROW_SHIFT;
        row[1] = (a1 + b1) >> ROW_SHIFT;
        row[6] = (a1 - b1) >> ROW_SHIFT;
        row[2] = (a2 + b2) >> ROW_SHIFT;
        row[5] = (a2 - b2) >> ROW_SHIFT;
        row[3] = (a3 + b3) >> ROW_SHIFT;
        row[4] = (a3 - b3) >> ROW_SHIFT;
    }
    for x in 0..8 {
        let c = |y: usize| block[y * 8 + x];
        let mut a0 = W4 * (c(0) + ((1 << (COL_SHIFT - 1)) / W4));
        let (mut a1, mut a2, mut a3) = (a0, a0, a0);
        a0 += W2 * c(2) + W4 * c(4) + W6 * c(6);
        a1 += W6 * c(2) - W4 * c(4) - W2 * c(6);
        a2 += -W6 * c(2) - W4 * c(4) + W2 * c(6);
        a3 += -W2 * c(2) + W4 * c(4) - W6 * c(6);
        let b0 = W1 * c(1) + W3 * c(3) + W5 * c(5) + W7 * c(7);
        let b1 = W3 * c(1) - W7 * c(3) - W1 * c(5) - W5 * c(7);
        let b2 = W5 * c(1) - W1 * c(3) + W7 * c(5) + W3 * c(7);
        let b3 = W7 * c(1) - W5 * c(3) + W3 * c(5) - W1 * c(7);
        let out = [a0 + b0, a1 + b1, a2 + b2, a3 + b3, a3 - b3, a2 - b2, a1 - b1, a0 - b0];
        for (y, v) in out.into_iter().enumerate() {
            block[y * 8 + x] = v >> COL_SHIFT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::idct;

    #[test]
    fn dc_only_is_flat() {
        let mut b = [0; 64];
        b[0] = 8 * 100;
        idct(&mut b);
        assert!(b.iter().all(|&v| v == 100));
    }
}
