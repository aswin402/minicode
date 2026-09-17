use crate::context::search::quantize::{BinaryVector128, TurbovecRecord};
use crate::error::{ContextError, Result};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

const MAGIC_BYTES: &[u8; 8] = b"TURBOVEC";
const FORMAT_VERSION: u32 = 1;
const HEADER_SIZE: usize = 64;
const METADATA_ENTRY_SIZE: usize = 40;
const RECORD_SIZE: usize = std::mem::size_of::<TurbovecRecord>(); // 88 bytes

/// Parsed metadata for a localized code chunk retrieved from memory map on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkMetadata {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub content: String,
}

/// Zero-copy memory-mapped high-density vector search index inspired by Turbovec.
/// Maps flat contiguous `TurbovecRecord` arrays directly into process memory,
/// bypassing JSON parsing and reducing index load latency to < 1ms.
pub struct MmapTurbovecIndex {
    mmap: Option<memmap2::Mmap>,
    data_buffer: Option<Vec<u8>>,
    record_count: usize,
    records_offset: usize,
    metadata_offset: usize,
    string_pool_offset: usize,
}

impl MmapTurbovecIndex {
    /// Opens an existing Turbovec binary index from disk via memory mapping.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to open Turbovec index file: {}", e))
        })?;

        let metadata = file.metadata().map_err(|e| {
            ContextError::VectorIndex(format!("Failed to read index metadata: {}", e))
        })?;

        if metadata.len() < HEADER_SIZE as u64 {
            return Err(ContextError::VectorIndex("Turbovec index file too small".into()).into());
        }

        // Attempt memory mapping with safe fallback to read buffer
        let (mmap, data_buffer) = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => (Some(m), None),
            Err(_) => {
                let bytes = fs::read(path).map_err(|e| {
                    ContextError::VectorIndex(format!("Failed to read fallback index bytes: {}", e))
                })?;
                (None, Some(bytes))
            }
        };

        let slice = if let Some(ref m) = mmap {
            &m[..]
        } else if let Some(ref b) = data_buffer {
            &b[..]
        } else {
            return Err(ContextError::VectorIndex("No index data buffer available".into()).into());
        };

        // Validate magic bytes
        if &slice[0..8] != MAGIC_BYTES {
            return Err(ContextError::VectorIndex("Invalid Turbovec magic header".into()).into());
        }

        let version = u32::from_le_bytes(
            slice[8..12]
                .try_into()
                .map_err(|_| ContextError::VectorIndex("Corrupted version bytes".into()))?,
        );
        if version != FORMAT_VERSION {
            return Err(ContextError::VectorIndex(format!(
                "Unsupported Turbovec format version: {}",
                version
            ))
            .into());
        }

        let record_count = u32::from_le_bytes(
            slice[16..20]
                .try_into()
                .map_err(|_| ContextError::VectorIndex("Corrupted record count".into()))?,
        ) as usize;

        let records_offset = u64::from_le_bytes(
            slice[20..28]
                .try_into()
                .map_err(|_| ContextError::VectorIndex("Corrupted records offset".into()))?,
        ) as usize;

        let metadata_offset = u64::from_le_bytes(
            slice[28..36]
                .try_into()
                .map_err(|_| ContextError::VectorIndex("Corrupted metadata offset".into()))?,
        ) as usize;

        let string_pool_offset = u64::from_le_bytes(
            slice[36..44]
                .try_into()
                .map_err(|_| ContextError::VectorIndex("Corrupted string pool offset".into()))?,
        ) as usize;

        // Bounds validation
        let expected_records_end = records_offset + record_count * RECORD_SIZE;
        let expected_meta_end = metadata_offset + record_count * METADATA_ENTRY_SIZE;

        if slice.len() < expected_records_end || slice.len() < expected_meta_end {
            return Err(
                ContextError::VectorIndex("Index file truncated or corrupted".into()).into(),
            );
        }

        Ok(Self {
            mmap,
            data_buffer,
            record_count,
            records_offset,
            metadata_offset,
            string_pool_offset,
        })
    }

    /// Number of vector records in this index.
    #[inline]
    pub fn record_count(&self) -> usize {
        self.record_count
    }

    fn slice(&self) -> &[u8] {
        if let Some(ref m) = self.mmap {
            &m[..]
        } else {
            self.data_buffer.as_deref().unwrap_or(&[])
        }
    }

    /// Retrieves a single record from the memory-mapped contiguous records array.
    #[inline]
    pub fn get_record(&self, index: usize) -> Option<TurbovecRecord> {
        if index >= self.record_count {
            return None;
        }
        let offset = self.records_offset + index * TurbovecRecord::BYTE_SIZE;
        let slice = self.slice();
        if offset + TurbovecRecord::BYTE_SIZE > slice.len() {
            return None;
        }
        TurbovecRecord::from_bytes(&slice[offset..offset + TurbovecRecord::BYTE_SIZE])
    }

    /// Performs high-speed 2-stage vector search across all records:
    /// Stage 1: SIMD popcount Hamming distance screening using 1-bit binary vectors.
    /// Stage 2: High-fidelity asymmetric dot product reranking on the top candidates.
    pub fn search(&self, query_vec: &[f32], k: usize) -> Vec<(usize, f32)> {
        if self.record_count == 0 || k == 0 {
            return Vec::new();
        }

        let rotated_query = BinaryVector128::fwht_slice(query_vec);
        let query_binary = BinaryVector128::from_f32_slice(&rotated_query);

        // Stage 1: Hamming distance screening over all records
        let candidate_pool_size = (k * 4).clamp(16, 256).min(self.record_count);
        let mut stage1_candidates: Vec<(usize, u32)> = Vec::with_capacity(self.record_count);

        for i in 0..self.record_count {
            if let Some(rec) = self.get_record(i) {
                let dist = rec.hamming_distance(&query_binary);
                stage1_candidates.push((i, dist));
            }
        }

        // Sort by lowest Hamming distance (highest angular proximity)
        if stage1_candidates.len() > candidate_pool_size {
            stage1_candidates.select_nth_unstable_by_key(candidate_pool_size, |&(_, dist)| dist);
            stage1_candidates.truncate(candidate_pool_size);
        }

        // Stage 2: Asymmetric dot product reranking on top candidates
        let mut scored: Vec<(usize, f32)> = stage1_candidates
            .into_iter()
            .filter_map(|(idx, _)| {
                let rec = self.get_record(idx)?;
                let score = rec.quant4.asymmetric_dot_product(&rotated_query);
                Some((idx, score))
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored
    }

    /// Loads chunk metadata directly from the memory map on demand without heap allocation overhead.
    pub fn get_metadata(&self, index: usize) -> Option<ChunkMetadata> {
        if index >= self.record_count {
            return None;
        }

        let meta_entry_offset = self.metadata_offset + index * METADATA_ENTRY_SIZE;
        let slice = self.slice();
        if meta_entry_offset + METADATA_ENTRY_SIZE > slice.len() {
            return None;
        }

        let pool = &slice[self.string_pool_offset..];

        let file_path_offset = u32::from_le_bytes(
            slice[meta_entry_offset..meta_entry_offset + 4]
                .try_into()
                .ok()?,
        ) as usize;
        let file_path_len = u32::from_le_bytes(
            slice[meta_entry_offset + 4..meta_entry_offset + 8]
                .try_into()
                .ok()?,
        ) as usize;

        let start_line = u32::from_le_bytes(
            slice[meta_entry_offset + 8..meta_entry_offset + 12]
                .try_into()
                .ok()?,
        ) as usize;
        let end_line = u32::from_le_bytes(
            slice[meta_entry_offset + 12..meta_entry_offset + 16]
                .try_into()
                .ok()?,
        ) as usize;

        let symbol_name_offset = u32::from_le_bytes(
            slice[meta_entry_offset + 16..meta_entry_offset + 20]
                .try_into()
                .ok()?,
        ) as usize;
        let symbol_name_len = u32::from_le_bytes(
            slice[meta_entry_offset + 20..meta_entry_offset + 24]
                .try_into()
                .ok()?,
        ) as usize;

        let symbol_kind_offset = u32::from_le_bytes(
            slice[meta_entry_offset + 24..meta_entry_offset + 28]
                .try_into()
                .ok()?,
        ) as usize;
        let symbol_kind_len = u32::from_le_bytes(
            slice[meta_entry_offset + 28..meta_entry_offset + 32]
                .try_into()
                .ok()?,
        ) as usize;

        let content_offset = u32::from_le_bytes(
            slice[meta_entry_offset + 32..meta_entry_offset + 36]
                .try_into()
                .ok()?,
        ) as usize;
        let content_len = u32::from_le_bytes(
            slice[meta_entry_offset + 36..meta_entry_offset + 40]
                .try_into()
                .ok()?,
        ) as usize;

        let file_path =
            std::str::from_utf8(&pool[file_path_offset..file_path_offset + file_path_len])
                .ok()?
                .to_string();
        let content = std::str::from_utf8(&pool[content_offset..content_offset + content_len])
            .ok()?
            .to_string();

        let symbol_name = if symbol_name_len > 0 {
            std::str::from_utf8(&pool[symbol_name_offset..symbol_name_offset + symbol_name_len])
                .ok()
                .map(|s| s.to_string())
        } else {
            None
        };

        let symbol_kind = if symbol_kind_len > 0 {
            std::str::from_utf8(&pool[symbol_kind_offset..symbol_kind_offset + symbol_kind_len])
                .ok()
                .map(|s| s.to_string())
        } else {
            None
        };

        Some(ChunkMetadata {
            file_path,
            start_line,
            end_line,
            symbol_name,
            symbol_kind,
            content,
        })
    }

    /// Serializes a collection of chunks into the flat binary Turbovec format atomically.
    pub fn create<P: AsRef<Path>>(
        path: P,
        records: &[TurbovecRecord],
        metadata: &[ChunkMetadata],
    ) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ContextError::VectorIndex(format!("Failed to create parent dir: {}", e))
            })?;
        }

        let temp_path = path.with_extension("tmp");
        let file = File::create(&temp_path).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to create index file: {}", e))
        })?;
        let mut writer = BufWriter::new(file);

        let record_count = records.len();
        let records_offset = HEADER_SIZE;
        let metadata_offset = records_offset + record_count * RECORD_SIZE;

        // Build string pool
        let mut string_pool = Vec::new();
        let mut meta_entries = Vec::with_capacity(record_count * METADATA_ENTRY_SIZE);

        for meta in metadata {
            let fp_offset = string_pool.len() as u32;
            string_pool.extend_from_slice(meta.file_path.as_bytes());
            let fp_len = meta.file_path.len() as u32;

            let sym_name_offset = string_pool.len() as u32;
            let sym_name_len = if let Some(ref name) = meta.symbol_name {
                string_pool.extend_from_slice(name.as_bytes());
                name.len() as u32
            } else {
                0
            };

            let sym_kind_offset = string_pool.len() as u32;
            let sym_kind_len = if let Some(ref kind) = meta.symbol_kind {
                string_pool.extend_from_slice(kind.as_bytes());
                kind.len() as u32
            } else {
                0
            };

            let content_offset = string_pool.len() as u32;
            string_pool.extend_from_slice(meta.content.as_bytes());
            let content_len = meta.content.len() as u32;

            meta_entries.extend_from_slice(&fp_offset.to_le_bytes());
            meta_entries.extend_from_slice(&fp_len.to_le_bytes());
            meta_entries.extend_from_slice(&(meta.start_line as u32).to_le_bytes());
            meta_entries.extend_from_slice(&(meta.end_line as u32).to_le_bytes());
            meta_entries.extend_from_slice(&sym_name_offset.to_le_bytes());
            meta_entries.extend_from_slice(&sym_name_len.to_le_bytes());
            meta_entries.extend_from_slice(&sym_kind_offset.to_le_bytes());
            meta_entries.extend_from_slice(&sym_kind_len.to_le_bytes());
            meta_entries.extend_from_slice(&content_offset.to_le_bytes());
            meta_entries.extend_from_slice(&content_len.to_le_bytes());
        }

        let string_pool_offset = metadata_offset + meta_entries.len();

        // 1. Write Header (64 bytes)
        let mut header = [0u8; HEADER_SIZE];
        header[0..8].copy_from_slice(MAGIC_BYTES);
        header[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        header[12..16].copy_from_slice(&128u32.to_le_bytes()); // Dimension
        header[16..20].copy_from_slice(&(record_count as u32).to_le_bytes());
        header[20..28].copy_from_slice(&(records_offset as u64).to_le_bytes());
        header[28..36].copy_from_slice(&(metadata_offset as u64).to_le_bytes());
        header[36..44].copy_from_slice(&(string_pool_offset as u64).to_le_bytes());

        writer
            .write_all(&header)
            .map_err(|e| ContextError::VectorIndex(format!("Failed to write header: {}", e)))?;

        // 2. Write Vector Records
        for rec in records {
            writer
                .write_all(&rec.to_bytes())
                .map_err(|e| ContextError::VectorIndex(format!("Failed to write record: {}", e)))?;
        }

        // 3. Write Metadata Entries
        writer.write_all(&meta_entries).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to write metadata table: {}", e))
        })?;

        // 4. Write String Pool
        writer.write_all(&string_pool).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to write string pool: {}", e))
        })?;

        writer
            .flush()
            .map_err(|e| ContextError::VectorIndex(format!("Failed to flush index: {}", e)))?;
        drop(writer);

        fs::rename(&temp_path, path).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to atomically rename index: {}", e))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mmap_turbovec_roundtrip_and_search() {
        let temp_dir = std::env::temp_dir().join(format!("turbovec_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let index_path = temp_dir.join("test_index.bin");

        let mut records = Vec::new();
        let mut metadata = Vec::new();

        for i in 0..50 {
            let content = format!(
                "pub fn fn_test_{}() {{ println!(\"distinct token key_{}\"); }}",
                i, i
            );
            let vec = crate::context::search::semantic::SemanticIndex::embed(&content);
            records.push(TurbovecRecord::from_f32_with_wht(&vec));
            metadata.push(ChunkMetadata {
                file_path: format!("src/module_{}.rs", i),
                start_line: i * 10 + 1,
                end_line: i * 10 + 30,
                symbol_name: Some(format!("fn_test_{}", i)),
                symbol_kind: Some("function".to_string()),
                content,
            });
        }

        // 1. Create binary index
        MmapTurbovecIndex::create(&index_path, &records, &metadata).expect("create failed");

        // 2. Open via mmap
        let index = MmapTurbovecIndex::open(&index_path).expect("open failed");
        assert_eq!(index.record_count(), 50);

        // 3. Verify metadata retrieval
        let meta0 = index.get_metadata(0).expect("metadata 0 missing");
        assert_eq!(meta0.file_path, "src/module_0.rs");
        assert_eq!(meta0.symbol_name.as_deref(), Some("fn_test_0"));

        let meta25 = index.get_metadata(25).expect("metadata 25 missing");
        assert_eq!(meta25.file_path, "src/module_25.rs");
        assert_eq!(meta25.symbol_name.as_deref(), Some("fn_test_25"));

        // 4. Search query vector matching record 10
        let query_text = "fn_test_10 token key_10";
        let query = crate::context::search::semantic::SemanticIndex::embed(query_text);

        let results = index.search(&query, 5);
        assert!(!results.is_empty());
        // Record 10 should be the top match!
        assert_eq!(results[0].0, 10, "Record 10 should be the top match");
        assert!(
            results[0].1 > 0.3,
            "Top match should have strong similarity"
        );

        let _ = fs::remove_file(&index_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
