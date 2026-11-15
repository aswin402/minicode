use serde::{Deserialize, Serialize};

/// Fixed 128-bit SplitMix64 orthogonal sign-flip mask ($D_{1,ii} \in \{-1, +1\}$) applied prior to
/// Stage-1 Fast Walsh-Hadamard Transform (`D_1 · H`). Eliminates DC/positive-orthant clustering on
/// sparse or non-negative features while preserving exact Euclidean norms and inner products.
const TURBOQUANT_SIGN_MASK: [u64; 2] = [0x9E37_79B9_7F4A_7C15, 0xBF58_476D_1CE4_E5B9];

/// Independent 128-bit SplitMix64 Rademacher sign-flip mask ($D_{2,ii} \in \{-1, +1\}$) applied prior to
/// Stage-2 Quantized Johnson-Lindenstrauss (`QJL`) residual projection (`D_2 · H`).
const TURBOQUANT_QJL_SIGN_MASK: [u64; 2] = [0x94D0_49BB_1331_11EB, 0xD6E8_FEB8_6659_FD93];

/// Optimal 4-level (2-bit) Lloyd-Max decision threshold on unit-variance Gaussian $\mathcal{N}(0, 1)$
/// coordinates: $t = \frac{c_{\text{inner}} + c_{\text{outer}}}{2} = 0.98159883$.
pub const LLOYD_MAX_2BIT_THRESHOLD: f32 = 0.981_598_83;

/// Optimal 4-level (2-bit) inner centroid $\mathbb{E}[Z \mid 0 \le Z < t]$ for $Z \sim \mathcal{N}(0, 1)$.
pub const LLOYD_MAX_2BIT_INNER: f32 = 0.452_78;

/// Optimal 4-level (2-bit) outer centroid $\mathbb{E}[Z \mid Z \ge t]$ for $Z \sim \mathcal{N}(0, 1)$.
pub const LLOYD_MAX_2BIT_OUTER: f32 = 1.510_417_6;

/// Declarative lookup table indexed by `(mag_bit << 1) | sign_bit`:
/// - `0b00` (`sign=0, mag=0`): `-0.4527800`
/// - `0b01` (`sign=1, mag=0`): `+0.4527800`
/// - `0b10` (`sign=0, mag=1`): `-1.5104176`
/// - `0b11` (`sign=1, mag=1`): `+1.5104176`
pub const LLOYD_MAX_2BIT_CODEBOOK: [f32; 4] = [
    -LLOYD_MAX_2BIT_INNER,
    LLOYD_MAX_2BIT_INNER,
    -LLOYD_MAX_2BIT_OUTER,
    LLOYD_MAX_2BIT_OUTER,
];

/// 128-dimensional 1-bit Polarized Binary Vector (16 bytes total).
/// Compresses 512-byte f32 vectors by 32x while preserving angular direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct BinaryVector128 {
    pub bits: [u64; 2],
}

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
                bits[i / 64] |= 1u64 << (i % 64);
            }
        }
        Self { bits }
    }

    /// Applies a 128-bit diagonal sign-flip mask in-place ($D_{ii} \in \{-1, +1\}$).
    #[inline]
    fn apply_mask_128(vec: &mut [f32; 128], sign_mask: &[u64; 2]) {
        for (word_idx, &mask) in sign_mask.iter().enumerate() {
            let base = word_idx * 64;
            for bit in 0..64 {
                if (mask >> bit) & 1 == 1 {
                    vec[base + bit] = -vec[base + bit];
                }
            }
        }
    }

    /// Applies the deterministic Stage-1 128-bit diagonal sign-flip $D_1$ in-place.
    #[inline]
    pub fn apply_sign_flip_128(vec: &mut [f32; 128]) {
        Self::apply_mask_128(vec, &TURBOQUANT_SIGN_MASK);
    }

    /// Applies the independent Stage-2 `QJL` Randomized Hadamard projection ($\frac{1}{\sqrt{128}} H_{128} D_2$) in-place.
    #[inline]
    pub fn qjl_fwht_128(vec: &mut [f32; 128]) {
        Self::apply_mask_128(vec, &TURBOQUANT_QJL_SIGN_MASK);
        Self::fwht_128(vec);
    }

    /// In-place orthonormal Fast Walsh-Hadamard Transform (FWHT) for 128-dimensional vectors.
    /// As an orthogonal transform ($H \cdot H^T = I$), it preserves inner products while
    /// distributing coordinate variance into an approximate Gaussian distribution.
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

    /// Prepares a 128-float slice with Randomized Fast Walsh-Hadamard rotation ($\frac{1}{\sqrt{128}} H_{128} D x$).
    /// Strictly orthogonal: $\langle R(x), R(y) \rangle = \langle x, y \rangle$.
    pub fn fwht_slice(slice: &[f32]) -> [f32; 128] {
        let mut buf = [0.0f32; 128];
        let len = slice.len().min(128);
        buf[..len].copy_from_slice(&slice[..len]);
        Self::apply_sign_flip_128(&mut buf);
        Self::fwht_128(&mut buf);
        buf
    }

    /// Quantizes a 128-dimensional float slice with Randomized FWHT rotation for maximal entropy.
    pub fn from_f32_with_wht(slice: &[f32]) -> Self {
        let rotated = Self::fwht_slice(slice);
        Self::from_f32_slice(&rotated)
    }

    /// Calculates Hamming distance (number of differing bits) using hardware bitwise popcount.
    #[inline]
    pub fn hamming_distance(&self, other: &Self) -> u32 {
        (self.bits[0] ^ other.bits[0]).count_ones() + (self.bits[1] ^ other.bits[1]).count_ones()
    }

    /// Converts Hamming distance into an angular cosine similarity approximation in `[-1.0, 1.0]`.
    #[inline]
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        let dist = self.hamming_distance(other);
        1.0 - (2.0 * dist as f32 / 128.0)
    }
}

/// 128-dimensional Two-Stage **TurboQuant (`PolarQuant` + `QJL`)** vector (`56 bytes` total, zero heap allocs).
///
/// Implements the full ICLR 2026 two-stage unbiased inner-product quantizer (`TurboQuant_prod`):
/// - `sign_bits` (`16 bytes`): Stage-1 1-bit sign plane (`1` iff $y_i \ge 0$), doubling directly as a `BinaryVector128`
///   so Stage-1 Hamming `POPCNT` screening needs zero duplicate storage.
/// - `mag_bits` (`16 bytes`): Stage-1 Lloyd-Max magnitude shell (`1` iff $|y_i| \ge 0.98159883 \cdot \sigma$).
/// - `qjl_bits` (`16 bytes`): Stage-2 Quantized Johnson-Lindenstrauss (`QJL`) 1-bit sign sketch on the orthogonal
///   residual $u = D_2 H (y - \alpha_{\text{MMSE}} \hat{z})$.
/// - `scale` (`4 bytes`): Stage-1 MMSE projection scale $\alpha_{\text{MMSE}} = \frac{\langle y, \hat{z} \rangle}{\|\hat{z}\|_2^2}$.
/// - `qjl_scale` (`4 bytes`): Stage-2 unbiased QJL residual scale $\beta^* = \frac{\|r\|_2^2}{\|u\|_1}$, occupying
///   what was previously 4 bytes of struct alignment padding so zero bytes are wasted.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PolarQuant2Fixed {
    pub sign_bits: [u64; 2],
    pub mag_bits: [u64; 2],
    #[serde(default)]
    pub qjl_bits: [u64; 2],
    pub scale: f32,
    #[serde(default)]
    pub qjl_scale: f32,
}

impl Default for PolarQuant2Fixed {
    fn default() -> Self {
        Self {
            sign_bits: [0u64; 2],
            mag_bits: [0u64; 2],
            qjl_bits: [0u64; 2],
            scale: 1.0,
            qjl_scale: 0.0,
        }
    }
}

impl PolarQuant2Fixed {
    pub const BYTE_SIZE: usize = 56;

    /// Quantizes an already-rotated 128-dimensional float slice using Two-Stage `PolarQuant + QJL`:
    /// Stage 1 fits the 4-level Gaussian Lloyd-Max codebook with MMSE scale $\alpha_{\text{MMSE}}$;
    /// Stage 2 projects the orthogonal residual $r = y - \hat{y}$ through an independent Randomized
    /// Hadamard transform (`D_2 · H`) into a 1-bit `QJL` sign sketch with unbiased scale $\beta^*$.
    pub fn from_rotated_slice(rotated: &[f32; 128]) -> Self {
        let mut sum_sq = 0.0f32;
        for &v in rotated.iter() {
            if v.is_finite() {
                sum_sq += v * v;
            }
        }
        if sum_sq <= 1e-20 {
            return Self {
                sign_bits: [0u64; 2],
                mag_bits: [0u64; 2],
                qjl_bits: [0u64; 2],
                scale: 0.0,
                qjl_scale: 0.0,
            };
        }
        let rms = (sum_sq / 128.0).sqrt();
        let threshold = LLOYD_MAX_2BIT_THRESHOLD * rms;

        let mut sign_bits = [0u64; 2];
        let mut mag_bits = [0u64; 2];
        let mut centroids = [0.0f32; 128];
        let mut clean_y = [0.0f32; 128];
        let mut dot_yz = 0.0f32;
        let mut norm_z_sq = 0.0f32;

        for (i, &val) in rotated.iter().enumerate() {
            let clean = if val.is_finite() { val } else { 0.0 };
            clean_y[i] = clean;
            let word = i / 64;
            let bit = i % 64;
            let s = usize::from(clean >= 0.0);
            let m = usize::from(clean.abs() >= threshold);
            if s == 1 {
                sign_bits[word] |= 1u64 << bit;
            }
            if m == 1 {
                mag_bits[word] |= 1u64 << bit;
            }
            let c = LLOYD_MAX_2BIT_CODEBOOK[(m << 1) | s];
            centroids[i] = c;
            dot_yz += clean * c;
            norm_z_sq += c * c;
        }

        let alpha_mmse = if norm_z_sq > 1e-12 {
            dot_yz / norm_z_sq
        } else {
            rms
        };

        // Stage 2: Orthogonal residual r = y - alpha_mmse * z_hat, projected via independent D_2 * H
        let mut residual_u = [0.0f32; 128];
        let mut res_sq = 0.0f32;
        for i in 0..128 {
            let r = clean_y[i] - alpha_mmse * centroids[i];
            residual_u[i] = r;
            res_sq += r * r;
        }

        let mut qjl_bits = [0u64; 2];
        let qjl_scale = if res_sq > 1e-20 {
            BinaryVector128::qjl_fwht_128(&mut residual_u);
            let mut u_l1 = 0.0f32;
            for (i, &u_val) in residual_u.iter().enumerate() {
                if u_val >= 0.0 {
                    qjl_bits[i / 64] |= 1u64 << (i % 64);
                }
                u_l1 += u_val.abs();
            }
            if u_l1 > 1e-12 {
                res_sq / u_l1
            } else {
                0.0
            }
        } else {
            0.0
        };

        Self {
            sign_bits,
            mag_bits,
            qjl_bits,
            scale: alpha_mmse,
            qjl_scale,
        }
    }

    /// Rotates `slice` via Randomized FWHT (`D_1 · H`) and quantizes into a 56-byte Two-Stage TurboQuant record.
    pub fn from_f32_with_wht(slice: &[f32]) -> Self {
        let rotated = BinaryVector128::fwht_slice(slice);
        Self::from_rotated_slice(&rotated)
    }

    /// Quantizes a raw 128-dimensional slice without Stage-1 FWHT rotation.
    pub fn from_f32_slice(slice: &[f32]) -> Self {
        let mut buf = [0.0f32; 128];
        let len = slice.len().min(128);
        buf[..len].copy_from_slice(&slice[..len]);
        Self::from_rotated_slice(&buf)
    }

    /// Returns the embedded 1-bit `BinaryVector128` view of `sign_bits` in $O(1)$ with zero copying.
    #[inline]
    pub const fn binary_vector(&self) -> BinaryVector128 {
        BinaryVector128 {
            bits: self.sign_bits,
        }
    }

    /// Stage-1 Hamming distance directly on the 1-bit sign plane (2 hardware `POPCNT` ops).
    #[inline]
    pub fn hamming_distance(&self, other_binary: &BinaryVector128) -> u32 {
        (self.sign_bits[0] ^ other_binary.bits[0]).count_ones()
            + (self.sign_bits[1] ^ other_binary.bits[1]).count_ones()
    }

    /// Two-Stage FastScan asymmetric dot product using a precomputed L1-resident `QueryLut2Bit`.
    /// Combines 32 Stage-1 2-bit PolarQuant nibble lookups + 32 Stage-2 1-bit `QJL` residual lookups.
    #[inline]
    pub fn score_with_lut(&self, lut: &QueryLut2Bit) -> f32 {
        let mut polar_acc = 0.0f32;
        let mut qjl_acc = 0.0f32;

        let s0 = self.sign_bits[0];
        let m0 = self.mag_bits[0];
        let q0 = self.qjl_bits[0];
        for k in 0..16 {
            let shift = k * 4;
            let idx = (((s0 >> shift) & 0x0F) | (((m0 >> shift) & 0x0F) << 4)) as usize;
            let q_nib = ((q0 >> shift) & 0x0F) as usize;
            polar_acc += lut.tables[k][idx & 0xFF];
            qjl_acc += lut.qjl_tables[k][q_nib & 0x0F];
        }

        let s1 = self.sign_bits[1];
        let m1 = self.mag_bits[1];
        let q1 = self.qjl_bits[1];
        for k in 0..16 {
            let shift = k * 4;
            let idx = (((s1 >> shift) & 0x0F) | (((m1 >> shift) & 0x0F) << 4)) as usize;
            let q_nib = ((q1 >> shift) & 0x0F) as usize;
            polar_acc += lut.tables[16 + k][idx & 0xFF];
            qjl_acc += lut.qjl_tables[16 + k][q_nib & 0x0F];
        }

        polar_acc * self.scale + qjl_acc * self.qjl_scale
    }

    /// Direct scalar asymmetric dot product against an already Stage-1 rotated query slice.
    pub fn asymmetric_dot_rotated(&self, rotated_query: &[f32; 128]) -> f32 {
        let mut polar_dot = 0.0f32;
        for (i, &qv) in rotated_query.iter().enumerate() {
            let word = i / 64;
            let bit = i % 64;
            let s = ((self.sign_bits[word] >> bit) & 1) as usize;
            let m = ((self.mag_bits[word] >> bit) & 1) as usize;
            polar_dot += qv * LLOYD_MAX_2BIT_CODEBOOK[(m << 1) | s];
        }

        let mut qjl_dot = 0.0f32;
        if self.qjl_scale != 0.0 {
            let mut qjl_q = *rotated_query;
            BinaryVector128::qjl_fwht_128(&mut qjl_q);
            for (i, &qu) in qjl_q.iter().enumerate() {
                let sign = if ((self.qjl_bits[i / 64] >> (i % 64)) & 1) == 1 {
                    1.0f32
                } else {
                    -1.0f32
                };
                qjl_dot += qu * sign;
            }
        }

        polar_dot * self.scale + qjl_dot * self.qjl_scale
    }

    /// Asymmetric dot product against an un-rotated `f32` query vector (applies Randomized FWHT to query).
    pub fn asymmetric_dot_product_with_wht(&self, query: &[f32]) -> f32 {
        let rotated_query = BinaryVector128::fwht_slice(query);
        self.asymmetric_dot_rotated(&rotated_query)
    }

    /// Reconstructs the Stage-1 rotated 128-dimensional `f32` vector including the inverse-projected QJL residual.
    pub fn dequantize_rotated(&self) -> [f32; 128] {
        let mut out = [0.0f32; 128];
        if self.qjl_scale != 0.0 {
            for (i, slot) in out.iter_mut().enumerate() {
                let sign = if ((self.qjl_bits[i / 64] >> (i % 64)) & 1) == 1 {
                    1.0f32
                } else {
                    -1.0f32
                };
                *slot = sign * self.qjl_scale;
            }
            // Inverse of (D_2 * H)^T? Note (H * D_2)^(-1) = D_2 * H since H = H^T and D_2 = D_2^(-1):
            BinaryVector128::fwht_128(&mut out);
            BinaryVector128::apply_mask_128(&mut out, &TURBOQUANT_QJL_SIGN_MASK);
        }
        for (i, slot) in out.iter_mut().enumerate() {
            let word = i / 64;
            let bit = i % 64;
            let s = ((self.sign_bits[word] >> bit) & 1) as usize;
            let m = ((self.mag_bits[word] >> bit) & 1) as usize;
            *slot += LLOYD_MAX_2BIT_CODEBOOK[(m << 1) | s] * self.scale;
        }
        out
    }

    /// Serializes into an exact 56-byte little-endian buffer.
    #[inline]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut buf = [0u8; Self::BYTE_SIZE];
        buf[0..8].copy_from_slice(&self.sign_bits[0].to_le_bytes());
        buf[8..16].copy_from_slice(&self.sign_bits[1].to_le_bytes());
        buf[16..24].copy_from_slice(&self.mag_bits[0].to_le_bytes());
        buf[24..32].copy_from_slice(&self.mag_bits[1].to_le_bytes());
        buf[32..40].copy_from_slice(&self.qjl_bits[0].to_le_bytes());
        buf[40..48].copy_from_slice(&self.qjl_bits[1].to_le_bytes());
        buf[48..52].copy_from_slice(&self.scale.to_le_bytes());
        buf[52..56].copy_from_slice(&self.qjl_scale.to_le_bytes());
        buf
    }

    /// Deserializes from a 56-byte little-endian buffer.
    #[inline]
    pub fn from_bytes(slice: &[u8]) -> Option<Self> {
        if slice.len() < Self::BYTE_SIZE {
            return None;
        }
        Some(Self {
            sign_bits: [
                u64::from_le_bytes(slice[0..8].try_into().ok()?),
                u64::from_le_bytes(slice[8..16].try_into().ok()?),
            ],
            mag_bits: [
                u64::from_le_bytes(slice[16..24].try_into().ok()?),
                u64::from_le_bytes(slice[24..32].try_into().ok()?),
            ],
            qjl_bits: [
                u64::from_le_bytes(slice[32..40].try_into().ok()?),
                u64::from_le_bytes(slice[40..48].try_into().ok()?),
            ],
            scale: f32::from_le_bytes(slice[48..52].try_into().ok()?),
            qjl_scale: f32::from_le_bytes(slice[52..56].try_into().ok()?),
        })
    }
}

/// Precomputed L1-cache Lookup Table (`32 KiB` PolarQuant + `2 KiB` QJL = `34 KiB`) for `turbovec`-style
/// Two-Stage FastScan over `PolarQuant2Fixed` vectors.
pub struct QueryLut2Bit {
    pub rotated_query: [f32; 128],
    pub query_binary: BinaryVector128,
    pub tables: [[f32; 256]; 32],
    pub qjl_tables: [[f32; 16]; 32],
}

impl QueryLut2Bit {
    /// Builds a Two-Stage (`PolarQuant + QJL`) FastScan lookup table from a Stage-1 rotated 128-D query vector.
    pub fn from_rotated_query(rotated_query: [f32; 128]) -> Self {
        let query_binary = BinaryVector128::from_f32_slice(&rotated_query);
        let mut tables = [[0.0f32; 256]; 32];

        for (slot, table) in tables.iter_mut().enumerate() {
            let base = slot * 4;
            let q0 = rotated_query[base];
            let q1 = rotated_query[base + 1];
            let q2 = rotated_query[base + 2];
            let q3 = rotated_query[base + 3];

            for (byte_idx, entry) in table.iter_mut().enumerate() {
                let s_nib = byte_idx & 0x0F;
                let m_nib = (byte_idx >> 4) & 0x0F;

                let c0 = LLOYD_MAX_2BIT_CODEBOOK[((m_nib & 1) << 1) | (s_nib & 1)];
                let c1 = LLOYD_MAX_2BIT_CODEBOOK[(((m_nib >> 1) & 1) << 1) | ((s_nib >> 1) & 1)];
                let c2 = LLOYD_MAX_2BIT_CODEBOOK[(((m_nib >> 2) & 1) << 1) | ((s_nib >> 2) & 1)];
                let c3 = LLOYD_MAX_2BIT_CODEBOOK[(((m_nib >> 3) & 1) << 1) | ((s_nib >> 3) & 1)];

                *entry = q0 * c0 + q1 * c1 + q2 * c2 + q3 * c3;
            }
        }

        let mut qjl_query = rotated_query;
        BinaryVector128::qjl_fwht_128(&mut qjl_query);
        let mut qjl_tables = [[0.0f32; 16]; 32];
        for (slot, qtable) in qjl_tables.iter_mut().enumerate() {
            let base = slot * 4;
            let u0 = qjl_query[base];
            let u1 = qjl_query[base + 1];
            let u2 = qjl_query[base + 2];
            let u3 = qjl_query[base + 3];

            for (nib, entry) in qtable.iter_mut().enumerate() {
                let s0 = if (nib & 1) != 0 { 1.0f32 } else { -1.0f32 };
                let s1 = if ((nib >> 1) & 1) != 0 {
                    1.0f32
                } else {
                    -1.0f32
                };
                let s2 = if ((nib >> 2) & 1) != 0 {
                    1.0f32
                } else {
                    -1.0f32
                };
                let s3 = if ((nib >> 3) & 1) != 0 {
                    1.0f32
                } else {
                    -1.0f32
                };
                *entry = u0 * s0 + u1 * s1 + u2 * s2 + u3 * s3;
            }
        }

        Self {
            rotated_query,
            query_binary,
            tables,
            qjl_tables,
        }
    }

    /// Rotates `query` with Randomized FWHT (`D · H`) and constructs the 32 KiB FastScan LUT.
    pub fn from_f32_query(query: &[f32]) -> Self {
        let rotated = BinaryVector128::fwht_slice(query);
        Self::from_rotated_query(rotated)
    }
}

/// 128-dimensional 4-bit Polarized Quantized Vector (64 bytes nibbles + 4 bytes scale = 68 bytes).
/// Retained for backwards-compatible deserialization of legacy caches and high-precision callers.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolarQuant4 {
    pub nibbles: Vec<u8>,
    pub scale: f32,
}

#[allow(dead_code)]
impl PolarQuant4 {
    fn to_fixed(&self) -> PolarQuant4Fixed {
        let mut nibbles = [0u8; 64];
        let len = self.nibbles.len().min(64);
        nibbles[..len].copy_from_slice(&self.nibbles[..len]);
        PolarQuant4Fixed {
            nibbles,
            scale: self.scale,
        }
    }

    pub fn from_f32_slice(slice: &[f32]) -> Self {
        PolarQuant4Fixed::from_f32_slice(slice).into()
    }

    #[inline]
    fn quantize_val(val: f32, scale: f32) -> u8 {
        if !val.is_finite() {
            return 8;
        }
        let norm = (val / scale).clamp(-1.0, 1.0);
        let scaled = ((norm + 1.0) * 7.5).round();
        scaled.clamp(0.0, 15.0) as u8
    }

    #[inline]
    fn dequantize_nibble(nibble: u8, scale: f32) -> f32 {
        let norm = (nibble as f32 / 7.5) - 1.0;
        norm * scale
    }

    pub fn asymmetric_dot_product(&self, query: &[f32]) -> f32 {
        self.to_fixed().asymmetric_dot_product(query)
    }

    pub fn from_f32_with_wht(slice: &[f32]) -> Self {
        PolarQuant4Fixed::from_f32_with_wht(slice).into()
    }

    pub fn asymmetric_dot_product_with_wht(&self, query: &[f32]) -> f32 {
        self.to_fixed().asymmetric_dot_product_with_wht(query)
    }

    pub fn dequantize(&self) -> Vec<f32> {
        self.to_fixed().dequantize().to_vec()
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

/// 128-dimensional 4-bit Polarized Quantized Vector with fixed-size inline storage (`68 bytes`).
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
    #[allow(clippy::needless_range_loop)]
    pub fn from_f32_slice(slice: &[f32]) -> Self {
        let max_abs = slice
            .iter()
            .take(128)
            .copied()
            .filter(|x| x.is_finite())
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

    pub fn from_f32_with_wht(slice: &[f32]) -> Self {
        let rotated = BinaryVector128::fwht_slice(slice);
        Self::from_f32_slice(&rotated)
    }

    pub fn asymmetric_dot_product_with_wht(&self, query: &[f32]) -> f32 {
        let rotated_query = BinaryVector128::fwht_slice(query);
        self.asymmetric_dot_product(&rotated_query)
    }

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

/// Contiguous 128-dimensional **Two-Stage TurboQuant (`PolarQuant + QJL`)** packed vector record
/// (`56 bytes` total, 8-byte aligned, zero struct padding waste).
///
/// Unifies Stage-1 1-bit Hamming screening (`quant2.sign_bits`, first 16 bytes) and Stage-2
/// 2-bit Gaussian Lloyd-Max + 1-bit `QJL` residual `QueryLut2Bit` FastScan scoring in a single
/// zero-copy, cache-line-friendly 56-byte record (down from 88 bytes in v1 and 512 bytes in raw `f32`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct TurbovecRecord {
    pub quant2: PolarQuant2Fixed,
}

impl TurbovecRecord {
    pub const BYTE_SIZE: usize = PolarQuant2Fixed::BYTE_SIZE;

    /// Creates a `TurbovecRecord` from an un-rotated `f32` slice without FWHT rotation.
    pub fn from_f32_slice(slice: &[f32]) -> Self {
        Self {
            quant2: PolarQuant2Fixed::from_f32_slice(slice),
        }
    }

    /// Creates a `TurbovecRecord` with Randomized Fast Walsh-Hadamard Transform (`D · H`) rotation.
    pub fn from_f32_with_wht(slice: &[f32]) -> Self {
        Self {
            quant2: PolarQuant2Fixed::from_f32_with_wht(slice),
        }
    }

    /// Wraps an already-quantized `PolarQuant2Fixed` directly.
    #[inline]
    pub const fn from_quant2(quant2: PolarQuant2Fixed) -> Self {
        Self { quant2 }
    }

    /// Returns the 16-byte 1-bit sign vector for Stage-1 Hamming screening.
    #[inline]
    pub const fn binary(&self) -> BinaryVector128 {
        self.quant2.binary_vector()
    }

    /// Calculates Hamming distance (number of differing bits) using hardware bitwise popcount.
    #[inline]
    pub fn hamming_distance(&self, other_binary: &BinaryVector128) -> u32 {
        self.quant2.hamming_distance(other_binary)
    }

    /// Stage-2 FastScan asymmetric dot product via precomputed L1 `QueryLut2Bit`.
    #[inline]
    pub fn score_with_lut(&self, lut: &QueryLut2Bit) -> f32 {
        self.quant2.score_with_lut(lut)
    }

    /// Direct scalar asymmetric dot product against a slice in the same basis as `self`.
    #[inline]
    pub fn asymmetric_dot_product(&self, query: &[f32]) -> f32 {
        let mut buf = [0.0f32; 128];
        let len = query.len().min(128);
        buf[..len].copy_from_slice(&query[..len]);
        self.quant2.asymmetric_dot_rotated(&buf)
    }

    /// Serializes the record into an exact 56-byte buffer.
    #[inline]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        self.quant2.to_bytes()
    }

    /// Deserializes a record from a 56-byte buffer.
    #[inline]
    pub fn from_bytes(slice: &[u8]) -> Option<Self> {
        Some(Self {
            quant2: PolarQuant2Fixed::from_bytes(slice)?,
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

        let raw_dot: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        let rot1 = BinaryVector128::fwht_slice(&v1);
        let rot2 = BinaryVector128::fwht_slice(&v2);
        let rot_dot: f32 = rot1.iter().zip(rot2.iter()).map(|(a, b)| a * b).sum();

        assert!(
            (raw_dot - rot_dot).abs() < 1e-4,
            "Randomized FWHT (D·H) must preserve dot product: raw {} vs rot {}",
            raw_dot,
            rot_dot
        );

        // Verify positive-orthant vector no longer concentrates all energy in slot 0
        let all_ones = [1.0f32 / (128.0f32).sqrt(); 128];
        let rotated_ones = BinaryVector128::fwht_slice(&all_ones);
        assert!(
            rotated_ones[0].abs() < 0.5,
            "Sign-flip mask D must disperse DC energy from index 0, got {}",
            rotated_ones[0]
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
    fn test_polar_quant2_lloyd_max_fidelity_and_lut_exactness() {
        let mut target = [0.0f32; 128];
        let mut query = [0.0f32; 128];

        for i in 0..128 {
            target[i] = (i as f32 * 0.23).sin() + (i as f32 * 0.41).cos() * 0.5;
            query[i] = target[i] + (i as f32 * 0.13).cos() * 0.18;
        }
        // Normalize both vectors as embeddings
        let norm_t: f32 = target.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_q: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
        for i in 0..128 {
            target[i] /= norm_t;
            query[i] /= norm_q;
        }

        let pq2 = PolarQuant2Fixed::from_f32_with_wht(&target);
        let lut = QueryLut2Bit::from_f32_query(&query);

        let lut_score = pq2.score_with_lut(&lut);
        let scalar_score = pq2.asymmetric_dot_product_with_wht(&query);
        assert!(
            (lut_score - scalar_score).abs() < 1e-5,
            "QueryLut2Bit FastScan must match scalar Two-Stage dot product: lut={} vs scalar={}",
            lut_score,
            scalar_score
        );

        let rot_query = BinaryVector128::fwht_slice(&query);
        let deq_rot = pq2.dequantize_rotated();
        let deq_score: f32 = rot_query
            .iter()
            .zip(deq_rot.iter())
            .map(|(a, b)| a * b)
            .sum();
        assert!(
            (deq_score - lut_score).abs() < 1e-5,
            "dequantize_rotated inverse QJL transform must match LUT score: deq={} vs lut={}",
            deq_score,
            lut_score
        );

        // Unbiased self-inner-product with QJL residual correction (~99.2% vs ~88.2% MMSE Stage-1 alone):
        let self_lut = QueryLut2Bit::from_f32_query(&target);
        let self_score = pq2.score_with_lut(&self_lut);
        let mut pq2_no_qjl = pq2;
        pq2_no_qjl.qjl_scale = 0.0;
        let stage1_self_score = pq2_no_qjl.score_with_lut(&self_lut);
        assert!(
            (self_score - 1.0).abs() < 0.03
                && (self_score - 1.0).abs() < (stage1_self_score - 1.0).abs(),
            "Two-Stage PolarQuant+QJL must tightly preserve self-inner-product (~1.0): two_stage={}, stage1_only={}",
            self_score,
            stage1_self_score
        );

        let exact_dot: f32 = target.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
        let rel_err = (exact_dot - lut_score).abs() / exact_dot.abs();
        assert!(
            rel_err < 0.04,
            "Two-Stage TurboQuant (PolarQuant+QJL) relative error should be < 4%, got exact={}, quant={}, err={}",
            exact_dot,
            lut_score,
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
        assert!(rel_err < 0.05);

        let pq: PolarQuant4 = pqf.into();
        let pqf2: PolarQuant4Fixed = pq.into();
        assert_eq!(pqf, pqf2);
    }

    #[test]
    fn test_turbovec_record_56_byte_two_stage_layout_and_roundtrip() {
        let mut target = [0.0f32; 128];
        for i in 0..128 {
            target[i] = (i as f32 * 0.19).cos();
        }

        let record = TurbovecRecord::from_f32_with_wht(&target);
        assert_eq!(record.hamming_distance(&record.binary()), 0);

        let lut = QueryLut2Bit::from_f32_query(&target);
        assert!(record.score_with_lut(&lut) > 0.0);

        let size = std::mem::size_of::<TurbovecRecord>();
        assert_eq!(
            size, 56,
            "Two-Stage TurbovecRecord (PolarQuant + QJL) must be exactly 56 bytes (9.1x compression vs 512B f32)"
        );

        let raw_bytes = record.to_bytes();
        let decoded = TurbovecRecord::from_bytes(&raw_bytes).unwrap();
        assert_eq!(record, decoded);

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: TurbovecRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }
}
