//! Pure-Rust zero-dependency RFC 1951 DEFLATE / RFC 1950 zlib Inflate engine
//! and ZIP Central Directory archive reader (used by DOCX, XLSX, PPTX, EPUB, ZIP, and PDF `/FlateDecode`).

use crate::error::{ContextError, Result};
use std::collections::HashMap;

/// Default maximum decompressed output ceiling (16 MiB) preventing zip-bomb memory exhaustion.
pub const DEFAULT_MAX_INFLATE_BYTES: usize = 16 * 1024 * 1024;

/// RFC 1951 §3.2.5 length base values for symbols 257..=285.
const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];

/// RFC 1951 §3.2.5 length extra bit counts for symbols 257..=285.
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// RFC 1951 §3.2.5 distance base values for symbols 0..=29.
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

/// RFC 1951 §3.2.5 distance extra bit counts for symbols 0..=29.
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// RFC 1951 §3.2.7 code-length alphabet permutation order.
const CODE_LEN_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u64,
    bit_count: u8,
}

impl<'a> BitReader<'a> {
    #[inline]
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    #[inline]
    fn ensure_bits(&mut self, needed: u8) -> Result<()> {
        while self.bit_count < needed {
            let Some(&b) = self.data.get(self.byte_pos) else {
                return Err(
                    ContextError::Parse("Unexpected EOF in DEFLATE bitstream".into()).into(),
                );
            };
            self.byte_pos += 1;
            self.bit_buf |= u64::from(b) << self.bit_count;
            self.bit_count += 8;
        }
        Ok(())
    }

    #[inline]
    fn read_bits(&mut self, count: u8) -> Result<u32> {
        if count == 0 {
            return Ok(0);
        }
        self.ensure_bits(count)?;
        let val = (self.bit_buf & ((1u64 << count) - 1)) as u32;
        self.bit_buf >>= count;
        self.bit_count -= count;
        Ok(val)
    }

    #[inline]
    fn align_to_byte(&mut self) {
        let discard = self.bit_count % 8;
        self.bit_buf >>= discard;
        self.bit_count -= discard;
    }

    fn read_aligned_u16_le(&mut self) -> Result<u16> {
        let lo = self.read_bits(8)? as u16;
        let hi = self.read_bits(8)? as u16;
        Ok(lo | (hi << 8))
    }

    fn read_aligned_slice(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len.min(65536));
        // Drain any buffered complete bytes first
        while out.len() < len && self.bit_count >= 8 {
            out.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf >>= 8;
            self.bit_count -= 8;
        }
        let rem = len - out.len();
        if rem > 0 {
            let end = self
                .byte_pos
                .checked_add(rem)
                .ok_or_else(|| ContextError::Parse("DEFLATE stored block overflow".into()))?;
            let slice = self
                .data
                .get(self.byte_pos..end)
                .ok_or_else(|| ContextError::Parse("Truncated DEFLATE stored block".into()))?;
            out.extend_from_slice(slice);
            self.byte_pos = end;
        }
        Ok(out)
    }
}

/// Canonical Huffman table supporting up to 15-bit RFC 1951 codes.
struct HuffmanTable {
    counts: [u16; 16],
    symbols: Vec<u16>,
    max_bits: u8,
}

impl HuffmanTable {
    fn from_code_lengths(lengths: &[u8]) -> Result<Self> {
        let mut counts = [0u16; 16];
        let mut max_bits = 0u8;
        for &len in lengths {
            if len > 15 {
                return Err(ContextError::Parse("DEFLATE Huffman code length > 15".into()).into());
            }
            if len > 0 {
                counts[len as usize] += 1;
                if len > max_bits {
                    max_bits = len;
                }
            }
        }
        if max_bits == 0 {
            return Ok(Self {
                counts,
                symbols: Vec::new(),
                max_bits: 0,
            });
        }

        // Verify code space is not oversubscribed
        let mut left: i32 = 1;
        for &c in counts.iter().skip(1).take(max_bits as usize) {
            left = (left << 1) - i32::from(c);
            if left < 0 {
                return Err(
                    ContextError::Parse("Oversubscribed DEFLATE Huffman tree".into()).into(),
                );
            }
        }

        let mut offsets = [0usize; 16];
        let mut total = 0usize;
        for bits in 1..=15 {
            offsets[bits] = total;
            total += counts[bits] as usize;
        }

        let mut symbols = vec![0u16; total];
        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0 {
                let idx = offsets[len as usize];
                if idx < symbols.len() {
                    symbols[idx] = sym as u16;
                    offsets[len as usize] += 1;
                }
            }
        }

        Ok(Self {
            counts,
            symbols,
            max_bits,
        })
    }

    fn fixed_lit_len() -> Result<Self> {
        let mut lengths = [0u8; 288];
        for slot in lengths.iter_mut().take(144) {
            *slot = 8;
        }
        for slot in lengths.iter_mut().skip(144).take(112) {
            *slot = 9;
        }
        for slot in lengths.iter_mut().skip(256).take(24) {
            *slot = 7;
        }
        for slot in lengths.iter_mut().skip(280).take(8) {
            *slot = 8;
        }
        Self::from_code_lengths(&lengths)
    }

    fn fixed_dist() -> Result<Self> {
        let lengths = [5u8; 32];
        Self::from_code_lengths(&lengths)
    }

    #[inline]
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16> {
        if self.max_bits == 0 {
            return Err(ContextError::Parse("Empty DEFLATE Huffman table".into()).into());
        }
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: usize = 0;

        for bits in 1..=self.max_bits {
            code |= reader.read_bits(1)? as i32;
            let count = i32::from(self.counts[bits as usize]);
            if code - count < first {
                let sym_idx = index + (code - first) as usize;
                return self.symbols.get(sym_idx).copied().ok_or_else(|| {
                    ContextError::Parse("DEFLATE Huffman symbol index out of range".into()).into()
                });
            }
            index += count as usize;
            first = (first + count) << 1;
            code <<= 1;
        }

        Err(ContextError::Parse("Invalid DEFLATE Huffman code".into()).into())
    }
}

fn read_dynamic_tables(reader: &mut BitReader<'_>) -> Result<(HuffmanTable, HuffmanTable)> {
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;

    if hlit > 286 || hdist > 30 {
        return Err(
            ContextError::Parse("Invalid HLIT/HDIST in dynamic DEFLATE header".into()).into(),
        );
    }

    let mut code_len_lengths = [0u8; 19];
    for &idx in CODE_LEN_ORDER.iter().take(hclen) {
        code_len_lengths[idx] = reader.read_bits(3)? as u8;
    }
    let code_len_table = HuffmanTable::from_code_lengths(&code_len_lengths)?;

    let total_lens = hlit + hdist;
    let mut lengths = Vec::with_capacity(total_lens);
    while lengths.len() < total_lens {
        let sym = code_len_table.decode(reader)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let Some(&prev) = lengths.last() else {
                    return Err(ContextError::Parse(
                        "DEFLATE repeat symbol 16 without predecessor".into(),
                    )
                    .into());
                };
                let repeat = reader.read_bits(2)? as usize + 3;
                if lengths.len() + repeat > total_lens {
                    return Err(
                        ContextError::Parse("DEFLATE code-length repeat overflow".into()).into(),
                    );
                }
                lengths.resize(lengths.len() + repeat, prev);
            }
            17 => {
                let repeat = reader.read_bits(3)? as usize + 3;
                if lengths.len() + repeat > total_lens {
                    return Err(
                        ContextError::Parse("DEFLATE zero-repeat 17 overflow".into()).into(),
                    );
                }
                lengths.resize(lengths.len() + repeat, 0);
            }
            18 => {
                let repeat = reader.read_bits(7)? as usize + 11;
                if lengths.len() + repeat > total_lens {
                    return Err(
                        ContextError::Parse("DEFLATE zero-repeat 18 overflow".into()).into(),
                    );
                }
                lengths.resize(lengths.len() + repeat, 0);
            }
            _ => {
                return Err(
                    ContextError::Parse("Invalid DEFLATE code-length symbol".into()).into(),
                );
            }
        }
    }

    let lit_table = HuffmanTable::from_code_lengths(&lengths[..hlit])?;
    let dist_table = HuffmanTable::from_code_lengths(&lengths[hlit..])?;
    Ok((lit_table, dist_table))
}

fn inflate_huffman_block(
    reader: &mut BitReader<'_>,
    lit_table: &HuffmanTable,
    dist_table: &HuffmanTable,
    out: &mut Vec<u8>,
    max_out: usize,
) -> Result<()> {
    loop {
        let sym = lit_table.decode(reader)?;
        match sym {
            0..=255 => {
                if out.len() >= max_out {
                    return Err(ContextError::Parse(format!(
                        "DEFLATE output exceeded safety ceiling ({} bytes)",
                        max_out
                    ))
                    .into());
                }
                out.push(sym as u8);
            }
            256 => break,
            257..=285 => {
                let len_idx = (sym - 257) as usize;
                let extra_len = reader.read_bits(LEN_EXTRA[len_idx])? as usize;
                let length = LEN_BASE[len_idx] as usize + extra_len;

                let dist_sym = dist_table.decode(reader)? as usize;
                if dist_sym >= DIST_BASE.len() {
                    return Err(
                        ContextError::Parse("Invalid DEFLATE distance symbol".into()).into(),
                    );
                }
                let extra_dist = reader.read_bits(DIST_EXTRA[dist_sym])? as usize;
                let distance = DIST_BASE[dist_sym] as usize + extra_dist;

                if distance == 0 || distance > out.len() {
                    return Err(ContextError::Parse(
                        "DEFLATE back-reference before start of buffer".into(),
                    )
                    .into());
                }
                if out.len().saturating_add(length) > max_out {
                    return Err(ContextError::Parse(format!(
                        "DEFLATE LZ77 copy exceeded safety ceiling ({} bytes)",
                        max_out
                    ))
                    .into());
                }

                // Byte-by-byte copy preserves overlapping RLE repeats (when distance < length)
                for _ in 0..length {
                    let b = out[out.len() - distance];
                    out.push(b);
                }
            }
            _ => {
                return Err(
                    ContextError::Parse("Invalid DEFLATE literal/length symbol".into()).into(),
                );
            }
        }
    }
    Ok(())
}

/// Decompresses a raw RFC 1951 DEFLATE stream bounded by `max_out` bytes.
pub fn inflate_raw(input: &[u8], max_out: usize) -> Result<Vec<u8>> {
    let mut reader = BitReader::new(input);
    let mut out = Vec::with_capacity(input.len().saturating_mul(2).min(max_out).min(262_144));

    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;

        match btype {
            0 => {
                reader.align_to_byte();
                let len = reader.read_aligned_u16_le()?;
                let nlen = reader.read_aligned_u16_le()?;
                if len != !nlen {
                    return Err(ContextError::Parse(
                        "DEFLATE stored block LEN/NLEN mismatch".into(),
                    )
                    .into());
                }
                if out.len().saturating_add(len as usize) > max_out {
                    return Err(ContextError::Parse(format!(
                        "DEFLATE stored block exceeded safety ceiling ({} bytes)",
                        max_out
                    ))
                    .into());
                }
                let raw = reader.read_aligned_slice(len as usize)?;
                out.extend_from_slice(&raw);
            }
            1 => {
                let lit = HuffmanTable::fixed_lit_len()?;
                let dist = HuffmanTable::fixed_dist()?;
                inflate_huffman_block(&mut reader, &lit, &dist, &mut out, max_out)?;
            }
            2 => {
                let (lit, dist) = read_dynamic_tables(&mut reader)?;
                inflate_huffman_block(&mut reader, &lit, &dist, &mut out, max_out)?;
            }
            _ => {
                return Err(ContextError::Parse("Reserved DEFLATE block type 3".into()).into());
            }
        }

        if bfinal == 1 {
            break;
        }
    }

    Ok(out)
}

/// Decompresses an RFC 1950 zlib-wrapped DEFLATE stream (used by PDF `/FlateDecode` streams).
pub fn inflate_zlib(input: &[u8], max_out: usize) -> Result<Vec<u8>> {
    if input.len() < 2 {
        return Err(ContextError::Parse("Truncated zlib header".into()).into());
    }
    let cmf = input[0];
    let flg = input[1];
    let cm = cmf & 0x0F;
    if cm != 8 {
        return Err(
            ContextError::Parse(format!("Unsupported zlib compression method {}", cm)).into(),
        );
    }
    if ((u16::from(cmf) << 8) | u16::from(flg)) % 31 != 0 {
        return Err(ContextError::Parse("Invalid zlib header FCHECK".into()).into());
    }
    let fdict = (flg & 0x20) != 0;
    let payload_offset = if fdict { 6 } else { 2 };
    if input.len() < payload_offset {
        return Err(ContextError::Parse("Truncated zlib preset dictionary".into()).into());
    }
    inflate_raw(&input[payload_offset..], max_out)
}

// ============================================================================
// Pure-Rust ZIP Central Directory Archive Reader
// ============================================================================

const ZIP_EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
const ZIP_CDFH_SIG: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
const ZIP_LFH_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// Metadata parsed from a ZIP Central Directory File Header without decompressing entry bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntryInfo {
    pub name: String,
    pub compression_method: u16,
    pub compressed_size: usize,
    pub uncompressed_size: usize,
    pub local_header_offset: usize,
}

/// Zero-copy indexed reader over an in-memory ZIP archive (OOXML DOCX/XLSX/PPTX, EPUB, generic ZIP).
pub struct ZipArchive<'a> {
    data: &'a [u8],
    entries: Vec<ZipEntryInfo>,
    by_name: HashMap<String, usize>,
}

#[inline]
fn read_u16_le(slice: &[u8], offset: usize) -> Option<u16> {
    let bytes: [u8; 2] = slice.get(offset..offset + 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

#[inline]
fn read_u32_le(slice: &[u8], offset: usize) -> Option<u32> {
    let bytes: [u8; 4] = slice.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

impl<'a> ZipArchive<'a> {
    /// Parses the ZIP End-of-Central-Directory and Central Directory headers in $O(\text{entries})$.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < 22 {
            return Err(
                ContextError::Parse("ZIP archive smaller than minimum EOCD size".into()).into(),
            );
        }

        // Scan backward up to 65,557 bytes from EOF for EOCD signature `PK\x05\x06`
        let min_pos = data.len().saturating_sub(65_557);
        let mut eocd_pos = None;
        for pos in (min_pos..=data.len() - 22).rev() {
            if data.get(pos..pos + 4) == Some(&ZIP_EOCD_SIG) {
                let comment_len = read_u16_le(data, pos + 20).unwrap_or(0) as usize;
                if pos + 22 + comment_len == data.len() {
                    eocd_pos = Some(pos);
                    break;
                }
            }
        }

        let Some(eocd) = eocd_pos else {
            return Err(
                ContextError::Parse("ZIP End-of-Central-Directory not found".into()).into(),
            );
        };

        let total_entries = read_u16_le(data, eocd + 10).unwrap_or(0) as usize;
        let cd_size = read_u32_le(data, eocd + 12).unwrap_or(0) as usize;
        let cd_offset = read_u32_le(data, eocd + 16).unwrap_or(0) as usize;

        let cd_end = cd_offset
            .checked_add(cd_size)
            .filter(|&end| end <= data.len())
            .ok_or_else(|| ContextError::Parse("ZIP Central Directory out of bounds".into()))?;

        let mut entries = Vec::with_capacity(total_entries.min(4096));
        let mut by_name = HashMap::with_capacity(total_entries.min(4096));
        let mut cursor = cd_offset;

        while cursor + 46 <= cd_end && entries.len() < total_entries.max(65_535) {
            if data.get(cursor..cursor + 4) != Some(&ZIP_CDFH_SIG) {
                break;
            }
            let compression_method = read_u16_le(data, cursor + 10).unwrap_or(0);
            let compressed_size = read_u32_le(data, cursor + 20).unwrap_or(0) as usize;
            let uncompressed_size = read_u32_le(data, cursor + 24).unwrap_or(0) as usize;
            let name_len = read_u16_le(data, cursor + 28).unwrap_or(0) as usize;
            let extra_len = read_u16_le(data, cursor + 30).unwrap_or(0) as usize;
            let comment_len = read_u16_le(data, cursor + 32).unwrap_or(0) as usize;
            let local_header_offset = read_u32_le(data, cursor + 42).unwrap_or(0) as usize;

            let name_start = cursor + 46;
            let Some(name_end) = name_start.checked_add(name_len) else {
                break;
            };
            let Some(name_bytes) = data.get(name_start..name_end) else {
                break;
            };
            let name = String::from_utf8_lossy(name_bytes).replace('\\', "/");

            let idx = entries.len();
            by_name.insert(name.clone(), idx);
            entries.push(ZipEntryInfo {
                name,
                compression_method,
                compressed_size,
                uncompressed_size,
                local_header_offset,
            });

            let Some(next_cursor) = name_end
                .checked_add(extra_len)
                .and_then(|v| v.checked_add(comment_len))
            else {
                break;
            };
            cursor = next_cursor;
        }

        Ok(Self {
            data,
            entries,
            by_name,
        })
    }

    #[inline]
    pub fn entries(&self) -> &[ZipEntryInfo] {
        &self.entries
    }

    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Reads and decompresses a single ZIP entry (`Store` method 0 or `DEFLATE` method 8)
    /// bounded strictly by `max_bytes`.
    pub fn read_entry(&self, name: &str, max_bytes: usize) -> Result<Vec<u8>> {
        let normalized = name.trim_start_matches('/');
        let Some(&idx) = self.by_name.get(normalized) else {
            return Err(
                ContextError::Parse(format!("ZIP entry '{}' not found", normalized)).into(),
            );
        };
        let entry = &self.entries[idx];
        let lfh = entry.local_header_offset;
        if self.data.get(lfh..lfh + 4) != Some(&ZIP_LFH_SIG) {
            return Err(ContextError::Parse(format!(
                "Invalid ZIP Local File Header for '{}'",
                entry.name
            ))
            .into());
        }
        let lfh_name_len = read_u16_le(self.data, lfh + 26).unwrap_or(0) as usize;
        let lfh_extra_len = read_u16_le(self.data, lfh + 28).unwrap_or(0) as usize;
        let data_start = lfh
            .checked_add(30)
            .and_then(|v| v.checked_add(lfh_name_len))
            .and_then(|v| v.checked_add(lfh_extra_len))
            .ok_or_else(|| ContextError::Parse("ZIP local header offset overflow".into()))?;
        let data_end = data_start
            .checked_add(entry.compressed_size)
            .ok_or_else(|| ContextError::Parse("ZIP entry payload overflow".into()))?;
        let payload = self.data.get(data_start..data_end).ok_or_else(|| {
            ContextError::Parse(format!("Truncated ZIP payload for '{}'", entry.name))
        })?;

        match entry.compression_method {
            0 => {
                if payload.len() > max_bytes {
                    return Err(ContextError::Parse(format!(
                        "ZIP stored entry '{}' exceeds safety cap ({} bytes)",
                        entry.name, max_bytes
                    ))
                    .into());
                }
                Ok(payload.to_vec())
            }
            8 => inflate_raw(payload, max_bytes),
            other => Err(ContextError::Parse(format!(
                "Unsupported ZIP compression method {} for '{}'",
                other, entry.name
            ))
            .into()),
        }
    }

    /// Reads an entry as lossy UTF-8 text bounded by `max_bytes`.
    pub fn read_entry_string(&self, name: &str, max_bytes: usize) -> Result<String> {
        let bytes = self.read_entry(name, max_bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// Test utility constructing a valid in-memory ZIP archive (stored entries) for unit tests.
#[cfg(test)]
pub(crate) fn build_test_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut cd = Vec::new();

    for &(name, payload) in files {
        let offset = out.len() as u32;
        let name_bytes = name.as_bytes();
        let len = payload.len() as u32;

        // Local File Header
        out.extend_from_slice(&ZIP_LFH_SIG);
        out.extend_from_slice(&20u16.to_le_bytes()); // version
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method = Store
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&0u32.to_le_bytes()); // crc32 (unused by reader)
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(payload);

        // Central Directory File Header
        cd.extend_from_slice(&ZIP_CDFH_SIG);
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&len.to_le_bytes());
        cd.extend_from_slice(&len.to_le_bytes());
        cd.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&offset.to_le_bytes());
        cd.extend_from_slice(name_bytes);
    }

    let cd_offset = out.len() as u32;
    let cd_size = cd.len() as u32;
    out.extend_from_slice(&cd);

    // End of Central Directory
    out.extend_from_slice(&ZIP_EOCD_SIG);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inflate_zlib_fixed_and_dynamic_huffman_and_rle() {
        // 1. Real RFC 1950 zlib stream (BTYPE=1 Fixed Huffman + LZ77 overlapping match)
        let fixed_zlib: &[u8] = &[
            0x78, 0x9C, 0xF3, 0x48, 0xCD, 0xC9, 0xC9, 0xD7, 0x51, 0xC8, 0xCD, 0xCC, 0xCB, 0x4C,
            0xCE, 0x4F, 0x49, 0x55, 0x54, 0xF0, 0x20, 0x24, 0x00, 0x00, 0xB8, 0xC6, 0x11, 0x3C,
        ];
        let decompressed_fixed = inflate_zlib(fixed_zlib, 4096).unwrap();
        assert_eq!(
            String::from_utf8(decompressed_fixed).unwrap(),
            "Hello, minicode! Hello, minicode! Hello, minicode!"
        );

        // 2. Real RFC 1950 zlib stream (BTYPE=2 Dynamic Huffman + RLE repeats + back-references)
        let dynamic_zlib: &[u8] = &[
            0x78, 0xDA, 0xED, 0xD0, 0xC9, 0x4A, 0xC4, 0x40, 0x10, 0x06, 0xE0, 0x57, 0xF9, 0x3D,
            0x87, 0x40, 0xF6, 0x95, 0x20, 0xA3, 0x0E, 0x22, 0x6E, 0x41, 0x47, 0x10, 0x6F, 0x35,
            0x33, 0x21, 0x29, 0x4C, 0x4F, 0x9A, 0x49, 0x1A, 0xC4, 0xA7, 0xB7, 0xBA, 0xCD, 0x13,
            0x78, 0xF6, 0x50, 0x4D, 0x35, 0xB5, 0x7C, 0x50, 0x9B, 0x51, 0x0F, 0x84, 0xAB, 0x6E,
            0x21, 0xDC, 0x92, 0x52, 0x84, 0x9B, 0x6E, 0x94, 0x7C, 0xAB, 0x67, 0x1E, 0xA7, 0x13,
            0x3E, 0x6C, 0x61, 0x2B, 0xB1, 0x1B, 0x6C, 0x76, 0x37, 0xC9, 0x73, 0x4F, 0x5A, 0x13,
            0x1E, 0x48, 0xED, 0x8F, 0x84, 0x47, 0x83, 0x27, 0x83, 0x77, 0xC6, 0xB3, 0xE2, 0xC3,
            0x59, 0x26, 0x5A, 0xC6, 0xCB, 0x30, 0xE1, 0x95, 0x7B, 0x59, 0xB6, 0x23, 0x83, 0xB7,
            0x75, 0x55, 0x3B, 0x30, 0xAE, 0x25, 0xDA, 0xD9, 0x36, 0x77, 0x3D, 0x5D, 0x80, 0x1C,
            0xBE, 0xB7, 0x9B, 0x7B, 0x87, 0x1F, 0x1D, 0xDE, 0xAD, 0x13, 0xDF, 0xB6, 0x60, 0x63,
            0x71, 0x38, 0x5B, 0xFC, 0xD3, 0xE1, 0xE3, 0x2F, 0xAE, 0x0C, 0x4E, 0x06, 0x5F, 0x8C,
            0x69, 0xC5, 0x35, 0xE3, 0x2C, 0xF8, 0xEC, 0xF0, 0x45, 0x70, 0xB3, 0xAE, 0xD2, 0x02,
            0x1F, 0x24, 0xE4, 0x2B, 0xCD, 0x82, 0x5F, 0x22, 0x08, 0xA3, 0x38, 0x49, 0xB3, 0xBC,
            0x28, 0x11, 0x04, 0x41, 0x18, 0x86, 0x51, 0x14, 0xC5, 0x71, 0x9C, 0x24, 0x49, 0x9A,
            0xA6, 0x59, 0x96, 0xE5, 0x79, 0x5E, 0x14, 0x45, 0x59, 0x96, 0xF0, 0x7D, 0x1F, 0x4D,
            0xD3, 0xC0, 0xF3, 0x3C, 0x54, 0x55, 0x85, 0xBA, 0xAE, 0xB1, 0xF9, 0xBF, 0xDB, 0x9F,
            0xEE, 0xF6, 0x03, 0xC9, 0xC1, 0xC5, 0xFD,
        ];
        let expected_dyn = concat!(
            "Alpha Beta Gamma Delta Epsilon Zeta Eta Theta Iota Kappa Lambda Mu Nu Xi Omicron Pi Rho Sigma Tau Upsilon Phi Chi Psi Omega! ",
            "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega? ",
            "0123456789 000111222333444555666777888999 --- === +++ ::: ;;; ",
            "Alpha Beta Gamma Delta Epsilon Zeta Eta Theta Iota Kappa Lambda Mu Nu Xi Omicron Pi Rho Sigma Tau Upsilon Phi Chi Psi Omega! ",
            "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega? ",
            "0123456789 000111222333444555666777888999 --- === +++ ::: ;;; "
        );
        let decompressed_dyn = inflate_zlib(dynamic_zlib, 4096).unwrap();
        assert_eq!(String::from_utf8(decompressed_dyn).unwrap(), expected_dyn);

        // 3. Stored block (BTYPE=0, BFINAL=1) + zip-bomb ceiling enforcement
        let mut stored = vec![0x01u8]; // BFINAL=1, BTYPE=00
        let msg = b"pure-rust-deflate";
        let len = msg.len() as u16;
        stored.extend_from_slice(&len.to_le_bytes());
        stored.extend_from_slice(&(!len).to_le_bytes());
        stored.extend_from_slice(msg);
        assert_eq!(inflate_raw(&stored, 1024).unwrap(), msg);
        assert!(
            inflate_raw(&stored, 8).is_err(),
            "Safety cap must reject oversized streams"
        );
    }

    #[test]
    fn test_zip_archive_central_directory_and_entry_read() {
        let zip_bytes = build_test_zip(&[
            (
                "word/document.xml",
                b"<w:document><w:t>Hello</w:t></w:document>",
            ),
            ("notes.txt", b"line 1\nline 2"),
        ]);
        let zip = ZipArchive::parse(&zip_bytes).unwrap();
        assert_eq!(zip.entries().len(), 2);
        assert!(zip.contains("word/document.xml"));
        assert_eq!(
            zip.read_entry_string("notes.txt", 4096).unwrap(),
            "line 1\nline 2"
        );
    }
}
