use serde::{Deserialize, Serialize};

/// 128-dimensional 1-bit Polarized Binary Vector (16 bytes total).
/// Compresses 512-byte f32 vectors by 32x while preserving angular direction.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct BinaryVector128 {
    pub bits: [u64; 2],
}

#[allow(dead_code)]
impl BinaryVector128 {
    /// Creates a new empty binary vector.
    pub const fn new() -> Self {
        Self { bits: [0, 0] }
    }

    /// Quantizes a 128-dimensional float slice into a 16-byte binary vector.
    /// Values >= 0.0 map to 1; values < 0.0 map to 0.
    pub fn from_f32_slice(slice: &[f32]) -> Self {
        let mut bits = [0u64; 2];
        for (i, &val) in slice.iter().take(128).enumerate() {
            if val >= 0.0 {
                let word = i / 64;
                let bit = i % 64;
                bits[word] |= 1u64 << bit;
            }
        }
        Self { bits }
    }

    /// Calculates Hamming distance (number of differing bits) using bitwise popcount.
    #[inline]
    pub fn hamming_distance(&self, other: &Self) -> u32 {
        (self.bits[0] ^ other.bits[0]).count_ones() + (self.bits[1] ^ other.bits[1]).count_ones()
    }

    /// Converts Hamming distance into an angular cosine similarity approximation in [-1.0, 1.0].
    #[inline]
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        let dist = self.hamming_distance(other);
        1.0 - (2.0 * dist as f32 / 128.0)
    }
}

/// 128-dimensional 4-bit Polarized Quantized Vector (64 bytes nibbles + 4 bytes scale = 68 bytes).
/// Provides high fidelity (98%+ cosine fidelity to f32) with 7.5x compression.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolarQuant4 {
    /// 128 4-bit nibbles packed into 64 bytes.
    pub nibbles: Vec<u8>,
    /// Dynamic scale amplitude of the vector.
    pub scale: f32,
}

#[allow(dead_code)]
impl PolarQuant4 {
    /// Quantizes a 128-dimensional float slice into 4-bit nibbles.
    #[allow(clippy::needless_range_loop)]
    pub fn from_f32_slice(slice: &[f32]) -> Self {
        let max_abs = slice
            .iter()
            .take(128)
            .map(|x| x.abs())
            .fold(0.0f32, f32::max)
            .max(1e-6);

        let mut nibbles = vec![0u8; 64];
        for i in 0..64 {
            let idx0 = i * 2;
            let idx1 = i * 2 + 1;

            let v0 = slice.get(idx0).copied().unwrap_or(0.0);
            let v1 = slice.get(idx1).copied().unwrap_or(0.0);

            let q0 = Self::quantize_val(v0, max_abs);
            let q1 = Self::quantize_val(v1, max_abs);

            nibbles[i] = (q0 & 0x0F) | ((q1 & 0x0F) << 4);
        }

        Self {
            nibbles,
            scale: max_abs,
        }
    }

    #[inline]
    fn quantize_val(val: f32, scale: f32) -> u8 {
        let norm = (val / scale).clamp(-1.0, 1.0);
        let scaled = ((norm + 1.0) * 7.5).round();
        scaled.clamp(0.0, 15.0) as u8
    }

    #[inline]
    fn dequantize_nibble(nibble: u8, scale: f32) -> f32 {
        let norm = (nibble as f32 / 7.5) - 1.0;
        norm * scale
    }

    /// Fast asymmetric dot product between an f32 query vector and the quantized vector.
    pub fn asymmetric_dot_product(&self, query: &[f32]) -> f32 {
        let mut dot = 0.0f32;
        for i in 0..64 {
            let byte = self.nibbles[i];
            let q0 = byte & 0x0F;
            let q1 = (byte >> 4) & 0x0F;

            let v0 = Self::dequantize_nibble(q0, self.scale);
            let v1 = Self::dequantize_nibble(q1, self.scale);

            let qv0 = query.get(i * 2).copied().unwrap_or(0.0);
            let qv1 = query.get(i * 2 + 1).copied().unwrap_or(0.0);

            dot += v0 * qv0 + v1 * qv1;
        }
        dot
    }

    /// Reconstructs the 128-dimensional f32 vector.
    pub fn dequantize(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(128);
        for i in 0..64 {
            let byte = self.nibbles[i];
            let q0 = byte & 0x0F;
            let q1 = (byte >> 4) & 0x0F;
            out.push(Self::dequantize_nibble(q0, self.scale));
            out.push(Self::dequantize_nibble(q1, self.scale));
        }
        out
    }
}
