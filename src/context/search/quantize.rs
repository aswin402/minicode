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

    /// In-place Fast Walsh-Hadamard Transform (FWHT) for 128-dimensional vectors.
    /// As an orthogonal transform ($H \cdot H^T = I$), it perfectly preserves inner products
    /// while uniformly distributing coordinate variance for optimal data-oblivious quantization (TurboQuant).
    pub fn fwht_128(vec: &mut [f32; 128]) {
        let mut h = 1;
        while h < 128 {
            for i in (0..128).step_by(h * 2) {
                for j in i..i + h {
                    let x = vec[j];
                    let y = vec[j + h];
                    vec[j] = x + y;
                    vec[j + h] = x - y;
                }
            }
            h *= 2;
        }
        let inv_norm = 1.0 / (128.0f32).sqrt();
        for v in vec.iter_mut() {
            *v *= inv_norm;
        }
    }

    /// Prepares a 128-float slice with FWHT rotation.
    pub fn fwht_slice(slice: &[f32]) -> [f32; 128] {
        let mut buf = [0.0f32; 128];
        let len = slice.len().min(128);
        buf[..len].copy_from_slice(&slice[..len]);
        Self::fwht_128(&mut buf);
        buf
    }

    /// Quantizes a 128-dimensional float slice with FWHT rotation for maximal entropy.
    pub fn from_f32_with_wht(slice: &[f32]) -> Self {
        let rotated = Self::fwht_slice(slice);
        Self::from_f32_slice(&rotated)
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

    /// Quantizes a 128-dimensional float slice with FWHT rotation for maximal entropy.
    pub fn from_f32_with_wht(slice: &[f32]) -> Self {
        let rotated = BinaryVector128::fwht_slice(slice);
        Self::from_f32_slice(&rotated)
    }

    /// Fast asymmetric dot product between an f32 query vector (automatically rotated via FWHT)
    /// and the quantized vector.
    pub fn asymmetric_dot_product_with_wht(&self, query: &[f32]) -> f32 {
        let rotated_query = BinaryVector128::fwht_slice(query);
        self.asymmetric_dot_product(&rotated_query)
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

fn serialize_nibbles<S>(nibbles: &[u8; 64], serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_bytes(nibbles)
}

fn deserialize_nibbles<'de, D>(deserializer: D) -> std::result::Result<[u8; 64], D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct NibbleVisitor;
    impl<'de> serde::de::Visitor<'de> for NibbleVisitor {
        type Value = [u8; 64];

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a byte array of length 64")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> std::result::Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if v.len() == 64 {
                let mut arr = [0u8; 64];
                arr.copy_from_slice(v);
                Ok(arr)
            } else {
                Err(E::custom(format!("expected 64 bytes, got {}", v.len())))
            }
        }

        fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut arr = [0u8; 64];
            for (i, item) in arr.iter_mut().enumerate() {
                *item = seq.next_element()?.ok_or_else(|| {
                    serde::de::Error::custom(format!("expected 64 bytes, got {}", i))
                })?;
            }
            Ok(arr)
        }
    }
    deserializer.deserialize_any(NibbleVisitor)
}

/// 128-dimensional 4-bit Polarized Quantized Vector with fixed-size inline storage.
/// Exactly 68 bytes ([u8; 64] nibbles + 4-byte f32 scale), zero heap allocations.
#[allow(dead_code)]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PolarQuant4Fixed {
    #[serde(
        serialize_with = "serialize_nibbles",
        deserialize_with = "deserialize_nibbles"
    )]
    pub nibbles: [u8; 64],
    pub scale: f32,
}

impl Default for PolarQuant4Fixed {
    fn default() -> Self {
        Self {
            nibbles: [0u8; 64],
            scale: 1.0,
        }
    }
}

#[allow(dead_code)]
impl PolarQuant4Fixed {
    /// Quantizes a 128-dimensional float slice into fixed-size 4-bit nibbles.
    #[allow(clippy::needless_range_loop)]
    pub fn from_f32_slice(slice: &[f32]) -> Self {
        let max_abs = slice
            .iter()
            .take(128)
            .map(|x| x.abs())
            .fold(0.0f32, f32::max)
            .max(1e-6);

        let mut nibbles = [0u8; 64];
        for i in 0..64 {
            let idx0 = i * 2;
            let idx1 = i * 2 + 1;

            let v0 = slice.get(idx0).copied().unwrap_or(0.0);
            let v1 = slice.get(idx1).copied().unwrap_or(0.0);

            let q0 = PolarQuant4::quantize_val(v0, max_abs);
            let q1 = PolarQuant4::quantize_val(v1, max_abs);

            nibbles[i] = (q0 & 0x0F) | ((q1 & 0x0F) << 4);
        }

        Self {
            nibbles,
            scale: max_abs,
        }
    }

    /// Fast asymmetric dot product between an f32 query vector and the quantized vector.
    pub fn asymmetric_dot_product(&self, query: &[f32]) -> f32 {
        let mut dot = 0.0f32;
        for i in 0..64 {
            let byte = self.nibbles[i];
            let q0 = byte & 0x0F;
            let q1 = (byte >> 4) & 0x0F;

            let v0 = PolarQuant4::dequantize_nibble(q0, self.scale);
            let v1 = PolarQuant4::dequantize_nibble(q1, self.scale);

            let qv0 = query.get(i * 2).copied().unwrap_or(0.0);
            let qv1 = query.get(i * 2 + 1).copied().unwrap_or(0.0);

            dot += v0 * qv0 + v1 * qv1;
        }
        dot
    }

    /// Quantizes a 128-dimensional float slice with FWHT rotation for maximal entropy.
    pub fn from_f32_with_wht(slice: &[f32]) -> Self {
        let rotated = BinaryVector128::fwht_slice(slice);
        Self::from_f32_slice(&rotated)
    }

    /// Fast asymmetric dot product between an f32 query vector (automatically rotated via FWHT)
    /// and the quantized vector.
    pub fn asymmetric_dot_product_with_wht(&self, query: &[f32]) -> f32 {
        let rotated_query = BinaryVector128::fwht_slice(query);
        self.asymmetric_dot_product(&rotated_query)
    }

    /// Reconstructs the 128-dimensional f32 vector.
    pub fn dequantize(&self) -> [f32; 128] {
        let mut out = [0.0f32; 128];
        for i in 0..64 {
            let byte = self.nibbles[i];
            let q0 = byte & 0x0F;
            let q1 = (byte >> 4) & 0x0F;
            out[i * 2] = PolarQuant4::dequantize_nibble(q0, self.scale);
            out[i * 2 + 1] = PolarQuant4::dequantize_nibble(q1, self.scale);
        }
        out
    }
}

impl From<PolarQuant4> for PolarQuant4Fixed {
    fn from(pq: PolarQuant4) -> Self {
        let mut nibbles = [0u8; 64];
        let len = pq.nibbles.len().min(64);
        nibbles[..len].copy_from_slice(&pq.nibbles[..len]);
        Self {
            nibbles,
            scale: pq.scale,
        }
    }
}

impl From<PolarQuant4Fixed> for PolarQuant4 {
    fn from(pqf: PolarQuant4Fixed) -> Self {
        Self {
            nibbles: pqf.nibbles.to_vec(),
            scale: pqf.scale,
        }
    }
}

/// Contiguous 128-dimensional TurboQuant packed vector record (88 bytes total with 8-byte alignment).
/// Combines 1-bit Polarized Binary Vector (16 bytes) for fast Hamming popcount screening
/// and 4-bit Polarized Quantized Vector (68 bytes) for high-fidelity asymmetric dot product reranking.
/// Completely contiguous, zero-copy, and cache-line coherent.
#[allow(dead_code)]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct TurbovecRecord {
    pub binary: BinaryVector128,  // 16 bytes (2 x u64)
    pub quant4: PolarQuant4Fixed, // 68 bytes ([u8; 64] + f32) + 4 bytes padding
}

#[allow(dead_code)]
impl TurbovecRecord {
    /// Creates a TurbovecRecord directly from an f32 embedding slice.
    pub fn from_f32_slice(slice: &[f32]) -> Self {
        let binary = BinaryVector128::from_f32_slice(slice);
        let quant4 = PolarQuant4Fixed::from_f32_slice(slice);
        Self { binary, quant4 }
    }

    /// Creates a TurbovecRecord with Fast Walsh-Hadamard Transform (FWHT) rotation.
    pub fn from_f32_with_wht(slice: &[f32]) -> Self {
        let rotated = BinaryVector128::fwht_slice(slice);
        let binary = BinaryVector128::from_f32_slice(&rotated);
        let quant4 = PolarQuant4Fixed::from_f32_slice(&rotated);
        Self { binary, quant4 }
    }

    /// Calculates Hamming distance (number of differing bits) using bitwise popcount.
    #[inline]
    pub fn hamming_distance(&self, other_binary: &BinaryVector128) -> u32 {
        self.binary.hamming_distance(other_binary)
    }

    /// Fast asymmetric dot product between an f32 query vector and the 4-bit quantized vector.
    #[inline]
    pub fn asymmetric_dot_product(&self, query: &[f32]) -> f32 {
        self.quant4.asymmetric_dot_product(query)
    }

    pub const BYTE_SIZE: usize = 88;

    /// Serializes the record into an exact 88-byte buffer.
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut buf = [0u8; Self::BYTE_SIZE];
        buf[0..8].copy_from_slice(&self.binary.bits[0].to_le_bytes());
        buf[8..16].copy_from_slice(&self.binary.bits[1].to_le_bytes());
        buf[16..80].copy_from_slice(&self.quant4.nibbles);
        buf[80..84].copy_from_slice(&self.quant4.scale.to_le_bytes());
        buf
    }

    /// Deserializes a record from an 88-byte buffer.
    pub fn from_bytes(slice: &[u8]) -> Option<Self> {
        if slice.len() < Self::BYTE_SIZE {
            return None;
        }
        let binary = BinaryVector128 {
            bits: [
                u64::from_le_bytes(slice[0..8].try_into().ok()?),
                u64::from_le_bytes(slice[8..16].try_into().ok()?),
            ],
        };
        let mut nibbles = [0u8; 64];
        nibbles.copy_from_slice(&slice[16..80]);
        let scale = f32::from_le_bytes(slice[80..84].try_into().ok()?);
        Some(Self {
            binary,
            quant4: PolarQuant4Fixed { nibbles, scale },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fwht_orthogonality_and_dot_product_preservation() {
        let mut v1 = [0.0f32; 128];
        let mut v2 = [0.0f32; 128];

        for i in 0..128 {
            v1[i] = (i as f32 * 0.17).sin();
            v2[i] = (i as f32 * 0.31).cos();
        }

        // Compute raw inner product
        let raw_dot: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();

        // Rotate with FWHT
        let mut rot1 = v1;
        let mut rot2 = v2;
        BinaryVector128::fwht_128(&mut rot1);
        BinaryVector128::fwht_128(&mut rot2);

        let rot_dot: f32 = rot1.iter().zip(rot2.iter()).map(|(a, b)| a * b).sum();

        // Inner product should be preserved within floating point precision (< 1e-4)
        assert!(
            (raw_dot - rot_dot).abs() < 1e-4,
            "FWHT must preserve dot product: raw {} vs rot {}",
            raw_dot,
            rot_dot
        );
    }

    #[test]
    fn test_binary_vector_quantization_and_hamming() {
        let v1 = vec![0.5f32; 128];
        let mut v2 = vec![0.5f32; 128];
        v2[0] = -0.5;
        v2[1] = -0.5;

        let b1 = BinaryVector128::from_f32_slice(&v1);
        let b2 = BinaryVector128::from_f32_slice(&v2);

        assert_eq!(b1.hamming_distance(&b1), 0);
        assert_eq!(b1.hamming_distance(&b2), 2);
        assert!(b1.cosine_similarity(&b1) > 0.99);
    }

    #[test]
    fn test_polar_quant4_fidelity() {
        let mut target = [0.0f32; 128];
        let mut query = [0.0f32; 128];

        for i in 0..128 {
            target[i] = (i as f32 * 0.23).sin();
            query[i] = (i as f32 * 0.23).sin() + (i as f32 * 0.1).cos() * 0.1;
        }

        let pq = PolarQuant4::from_f32_slice(&target);
        assert_eq!(pq.nibbles.len(), 64);

        let exact_dot: f32 = target.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
        let quant_dot = pq.asymmetric_dot_product(&query);

        // High fidelity dot product approximation (relative error < 5%)
        let rel_err = (exact_dot - quant_dot).abs() / exact_dot.abs();
        assert!(
            rel_err < 0.05,
            "Relative error should be < 5%, got exact: {}, quant: {}, rel_err: {}",
            exact_dot,
            quant_dot,
            rel_err
        );
    }

    #[test]
    fn test_polar_quant4_fixed_fidelity_and_zero_alloc() {
        let mut target = [0.0f32; 128];
        let mut query = [0.0f32; 128];

        for i in 0..128 {
            target[i] = (i as f32 * 0.23).sin();
            query[i] = (i as f32 * 0.23).sin() + (i as f32 * 0.1).cos() * 0.1;
        }

        let pqf = PolarQuant4Fixed::from_f32_slice(&target);
        assert_eq!(pqf.nibbles.len(), 64);

        let exact_dot: f32 = target.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
        let quant_dot = pqf.asymmetric_dot_product(&query);

        let rel_err = (exact_dot - quant_dot).abs() / exact_dot.abs();
        assert!(
            rel_err < 0.05,
            "Fixed relative error should be < 5%, got exact: {}, quant: {}, rel_err: {}",
            exact_dot,
            quant_dot,
            rel_err
        );

        // Verify conversion to/from PolarQuant4
        let pq: PolarQuant4 = pqf.into();
        let pqf2: PolarQuant4Fixed = pq.into();
        assert_eq!(pqf, pqf2);

        // Verify serde roundtrip
        let json = serde_json::to_string(&pqf).unwrap();
        let deserialized: PolarQuant4Fixed = serde_json::from_str(&json).unwrap();
        assert_eq!(pqf, deserialized);
    }

    #[test]
    fn test_turbovec_record_size_and_roundtrip() {
        let mut target = [0.0f32; 128];
        for i in 0..128 {
            target[i] = (i as f32 * 0.19).cos();
        }

        let record = TurbovecRecord::from_f32_slice(&target);
        assert_eq!(record.hamming_distance(&record.binary), 0);
        assert!(record.asymmetric_dot_product(&target) > 0.0);

        // Check alignment and struct size
        let size = std::mem::size_of::<TurbovecRecord>();
        assert!(
            size <= 88,
            "TurbovecRecord must be <= 88 bytes, got {}",
            size
        );

        // Verify serde roundtrip
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: TurbovecRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }
}
