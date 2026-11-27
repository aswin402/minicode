use crate::context::search::quantize::{BinaryVector128, QueryLut2Bit, TurbovecRecord};
use crate::error::{ContextError, Result};
use std::collections::BinaryHeap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

const MAGIC_BYTES: &[u8; 8] = b"TURBOVEC";
const TOMBSTONE_MAGIC: &[u8; 8] = b"TVTOMB03";
/// Format v3: 56-byte Two-Stage TurboQuant (`PolarQuant + QJL`) records + Coarse Binary IVF + `.tomb` persistence.
const FORMAT_VERSION: u32 = 3;
const HEADER_SIZE: usize = 64;
const METADATA_ENTRY_SIZE: usize = 40;
const RECORD_SIZE: usize = TurbovecRecord::BYTE_SIZE; // 56 bytes
const IVF_CLUSTER_ENTRY_SIZE: usize = 24; // 16B BinaryVector128 centroid + u32 start + u32 len

/// Closed-form Coarse Binary IVF cost-model solver:
/// Given $N$ records, evaluates $K = \lfloor\sqrt{N}\rfloor$ clusters and logarithmic probe count
/// $p = \lceil \ln(N) \rceil$, activating IVF if and only if expected Hamming evaluations
/// $W_{\text{ivf}}(N, K, p) = K + \lceil N \cdot p / K \rceil$ strictly cuts flat-scan work ($N$) by $> 2\times$.
#[inline]
fn solve_optimal_ivf_params(record_count: usize) -> Option<(usize, usize)> {
    if record_count < 4 {
        return None;
    }
    let n_f = record_count as f64;
    let k = (n_f.sqrt().floor() as usize).min(256);
    if k < 2 {
        return None;
    }
    let p = (n_f.ln().ceil() as usize).max(4).min(k);
    let ivf_work = k + (record_count * p).div_ceil(k);
    (ivf_work * 2 < record_count).then_some((k, p))
}

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

/// Zero-copy memory-mapped Two-Stage TurboQuant (`PolarQuant + QJL`) vector index (`FORMAT_VERSION = 3`)
/// with a zero-rewrite in-memory LSM delta overlay.
///
/// Maps contiguous 56-byte `TurbovecRecord` bit-planes directly into virtual memory (`< 1ms` open),
/// routes large workspaces via Coarse Binary IVF (`BinaryVector128` $K$-Means clusters), supports
/// $O(1)$ persisted `.tomb` tombstone masking + in-memory LSM delta append for incremental edits
/// without rewriting `.bin` until compaction, and executes bounded-heap Stage-1 Hamming `POPCNT`
/// screening + 4-way register-blocked Stage-2 `QueryLut2Bit` L1 FastScan.
#[derive(Clone)]
pub struct MmapTurbovecIndex {
    mmap: Option<std::sync::Arc<memmap2::Mmap>>,
    data_buffer: Option<std::sync::Arc<Vec<u8>>>,
    record_count: usize,
    records_offset: usize,
    metadata_offset: usize,
    string_pool_offset: usize,
    ivf_offset: usize,
    cluster_count: usize,
    tombstones: Vec<u64>,
    deleted_count: usize,
    /// In-memory LSM delta overlay storing newly appended records at virtual indices `record_count..`.
    delta_records: Vec<TurbovecRecord>,
    delta_metadata: Vec<ChunkMetadata>,
    delta_tombstones: Vec<bool>,
    delta_deleted_count: usize,
}

impl MmapTurbovecIndex {
    /// Companion tombstone bitmap path (`<index_path>.tomb`).
    pub fn tombstone_path<P: AsRef<Path>>(index_path: P) -> PathBuf {
        let mut os = index_path.as_ref().as_os_str().to_os_string();
        os.push(".tomb");
        PathBuf::from(os)
    }

    /// Opens an existing Turbovec v3 binary index from disk via memory mapping and loads any persisted `.tomb` bitmap.
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
            Ok(m) => (Some(std::sync::Arc::new(m)), None),
            Err(_) => {
                let bytes = fs::read(path).map_err(|e| {
                    ContextError::VectorIndex(format!("Failed to read fallback index bytes: {}", e))
                })?;
                (None, Some(std::sync::Arc::new(bytes)))
            }
        };

        let slice = if let Some(ref m) = mmap {
            &m[..]
        } else if let Some(ref b) = data_buffer {
            &b[..]
        } else {
            return Err(ContextError::VectorIndex("No index data buffer available".into()).into());
        };

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
                "Unsupported Turbovec format version {} (expected {})",
                version, FORMAT_VERSION
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

        let ivf_offset = u64::from_le_bytes(
            slice[44..52]
                .try_into()
                .map_err(|_| ContextError::VectorIndex("Corrupted IVF offset".into()))?,
        ) as usize;

        let cluster_count = u32::from_le_bytes(
            slice[52..56]
                .try_into()
                .map_err(|_| ContextError::VectorIndex("Corrupted IVF cluster count".into()))?,
        ) as usize;

        let expected_records_end = record_count
            .checked_mul(RECORD_SIZE)
            .and_then(|sz| records_offset.checked_add(sz))
            .ok_or_else(|| ContextError::VectorIndex("Records offset overflow".into()))?;
        let expected_meta_end = record_count
            .checked_mul(METADATA_ENTRY_SIZE)
            .and_then(|sz| metadata_offset.checked_add(sz))
            .ok_or_else(|| ContextError::VectorIndex("Metadata offset overflow".into()))?;

        let expected_ivf_end = if cluster_count > 0 && ivf_offset > 0 {
            let table_bytes = cluster_count
                .checked_mul(IVF_CLUSTER_ENTRY_SIZE)
                .ok_or_else(|| ContextError::VectorIndex("IVF table overflow".into()))?;
            let postings_bytes = record_count
                .checked_mul(4)
                .ok_or_else(|| ContextError::VectorIndex("IVF postings overflow".into()))?;
            ivf_offset
                .checked_add(table_bytes)
                .and_then(|end| end.checked_add(postings_bytes))
                .ok_or_else(|| ContextError::VectorIndex("IVF section overflow".into()))?
        } else {
            0
        };

        if slice.len() < expected_records_end
            || slice.len() < expected_meta_end
            || slice.len() < string_pool_offset
            || slice.len() < expected_ivf_end
        {
            return Err(
                ContextError::VectorIndex("Index file truncated or corrupted".into()).into(),
            );
        }

        let tombstone_words = record_count.div_ceil(64);
        let (tombstones, deleted_count) = Self::load_tombstones_if_present(
            &Self::tombstone_path(path),
            record_count,
            tombstone_words,
        );

        Ok(Self {
            mmap,
            data_buffer,
            record_count,
            records_offset,
            metadata_offset,
            string_pool_offset,
            ivf_offset,
            cluster_count,
            tombstones,
            deleted_count,
            delta_records: Vec::new(),
            delta_metadata: Vec::new(),
            delta_tombstones: Vec::new(),
            delta_deleted_count: 0,
        })
    }

    fn load_tombstones_if_present(
        tomb_path: &Path,
        record_count: usize,
        tombstone_words: usize,
    ) -> (Vec<u64>, usize) {
        let Ok(bytes) = fs::read(tomb_path) else {
            return (vec![0u64; tombstone_words], 0);
        };
        if bytes.len() < 16 || &bytes[0..8] != TOMBSTONE_MAGIC {
            return (vec![0u64; tombstone_words], 0);
        }
        let Ok(rec_bytes) = bytes[8..12].try_into() else {
            return (vec![0u64; tombstone_words], 0);
        };
        let Ok(word_bytes) = bytes[12..16].try_into() else {
            return (vec![0u64; tombstone_words], 0);
        };
        let saved_records = u32::from_le_bytes(rec_bytes) as usize;
        let saved_words = u32::from_le_bytes(word_bytes) as usize;
        if saved_records != record_count
            || saved_words != tombstone_words
            || bytes.len() < 16 + tombstone_words * 8
        {
            return (vec![0u64; tombstone_words], 0);
        }

        let mut words = Vec::with_capacity(tombstone_words);
        for w in 0..tombstone_words {
            let start = 16 + w * 8;
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&bytes[start..start + 8]);
            words.push(u64::from_le_bytes(raw));
        }

        // Mask off any out-of-range trailing bits in the final u64 word
        let rem = record_count % 64;
        if rem != 0 {
            if let Some(last) = words.last_mut() {
                *last &= (1u64 << rem) - 1;
            }
        }

        let deleted = words.iter().map(|w| w.count_ones() as usize).sum();
        (words, deleted)
    }

    /// Atomically persists the in-memory tombstone bitmap to `<index_path>.tomb` in $O(N/64)$ bytes
    /// without rewriting the underlying `.bin` memory map.
    pub fn persist_tombstones<P: AsRef<Path>>(&self, index_path: P) -> Result<()> {
        let tomb_path = Self::tombstone_path(index_path.as_ref());
        if self.deleted_count == 0 {
            let _ = fs::remove_file(&tomb_path);
            return Ok(());
        }

        if let Some(parent) = tomb_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ContextError::VectorIndex(format!("Failed to create tombstone dir: {}", e))
            })?;
        }

        let temp_path = tomb_path.with_extension(format!(
            "tomb.tmp.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));

        let mut buf = Vec::with_capacity(16 + self.tombstones.len() * 8);
        buf.extend_from_slice(TOMBSTONE_MAGIC);
        buf.extend_from_slice(&(self.record_count as u32).to_le_bytes());
        buf.extend_from_slice(&(self.tombstones.len() as u32).to_le_bytes());
        for &word in &self.tombstones {
            buf.extend_from_slice(&word.to_le_bytes());
        }

        fs::write(&temp_path, &buf).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to write tombstone file: {}", e))
        })?;
        fs::rename(&temp_path, &tomb_path).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to atomically rename tombstone file: {}", e))
        })?;
        Ok(())
    }

    /// Total number of physical vector slots in the memory-mapped base segment.
    #[inline]
    pub fn record_count(&self) -> usize {
        self.record_count
    }

    /// Total number of addressable virtual slots (`base + LSM delta overlay`).
    #[inline]
    pub fn total_slot_count(&self) -> usize {
        self.record_count + self.delta_records.len()
    }

    /// Number of live (non-tombstoned) records currently in the in-memory LSM delta overlay.
    #[inline]
    pub fn delta_record_count(&self) -> usize {
        self.delta_records
            .len()
            .saturating_sub(self.delta_deleted_count)
    }

    /// Number of soft-deleted slots currently masked by tombstones (base + delta).
    #[inline]
    pub fn deleted_count(&self) -> usize {
        self.deleted_count + self.delta_deleted_count
    }

    /// Number of live (non-tombstoned) vector records in this index (base + delta overlay).
    #[inline]
    pub fn active_record_count(&self) -> usize {
        self.record_count.saturating_sub(self.deleted_count) + self.delta_record_count()
    }

    /// Appends new vector records and chunk metadata to the in-memory LSM delta overlay in $O(|\Delta|)$
    /// without rewriting the underlying `.bin` memory map.
    pub fn append_delta_records(
        &mut self,
        records: Vec<TurbovecRecord>,
        metadata: Vec<ChunkMetadata>,
    ) -> usize {
        let count = records.len().min(metadata.len());
        self.delta_records.extend(records.into_iter().take(count));
        self.delta_metadata.extend(metadata.into_iter().take(count));
        self.delta_tombstones
            .resize(self.delta_records.len(), false);
        count
    }

    /// Returns `true` when tombstone fragmentation ($> 25\%$ of base slots deleted) or LSM delta
    /// overlay growth ($> 256$ delta slots) warrants compacting into a fresh `.bin` segment.
    #[inline]
    pub fn needs_compaction(&self) -> bool {
        (self.record_count > 0 && self.deleted_count.saturating_mul(4) > self.record_count)
            || self.delta_records.len() > 256
    }

    /// Collects the set of unique file paths currently served from live LSM delta overlay entries.
    pub fn delta_file_paths(&self) -> std::collections::HashSet<&str> {
        let mut files = std::collections::HashSet::new();
        for (i, meta) in self.delta_metadata.iter().enumerate() {
            if !self.delta_tombstones.get(i).copied().unwrap_or(true) {
                files.insert(meta.file_path.as_str());
            }
        }
        files
    }

    /// Number of Coarse Binary IVF clusters (`0` when flat scan is used for small indices).
    #[inline]
    pub fn cluster_count(&self) -> usize {
        self.cluster_count
    }

    /// Marks `index` (base mmap or LSM delta overlay) as soft-deleted in $O(1)$.
    #[inline]
    pub fn mark_deleted(&mut self, index: usize) -> bool {
        if index < self.record_count {
            let word = index / 64;
            let mask = 1u64 << (index % 64);
            if let Some(slot) = self.tombstones.get_mut(word) {
                if (*slot & mask) == 0 {
                    *slot |= mask;
                    self.deleted_count += 1;
                    return true;
                }
            }
            return false;
        }
        let delta_idx = index - self.record_count;
        if let Some(slot) = self.delta_tombstones.get_mut(delta_idx) {
            if !*slot {
                *slot = true;
                self.delta_deleted_count += 1;
                return true;
            }
        }
        false
    }

    /// Zero-allocation file-level soft-delete across both the memory-mapped base segment and
    /// the in-memory LSM delta overlay, tombstoning every slot where `should_delete(file_path)` is `true`.
    pub fn mark_deleted_where_file<F>(&mut self, should_delete: F) -> usize
    where
        F: Fn(&str) -> bool,
    {
        let mut newly_deleted = 0usize;
        for idx in 0..self.record_count {
            if self.is_deleted(idx) {
                continue;
            }
            let should_mark = self.peek_file_path(idx).is_some_and(&should_delete);
            if should_mark && self.mark_deleted(idx) {
                newly_deleted += 1;
            }
        }
        for delta_idx in 0..self.delta_metadata.len() {
            if self
                .delta_tombstones
                .get(delta_idx)
                .copied()
                .unwrap_or(true)
            {
                continue;
            }
            let should_mark = should_delete(&self.delta_metadata[delta_idx].file_path);
            if should_mark && self.mark_deleted(self.record_count + delta_idx) {
                newly_deleted += 1;
            }
        }
        newly_deleted
    }

    fn peek_file_path(&self, index: usize) -> Option<&str> {
        if index >= self.record_count {
            return self
                .delta_metadata
                .get(index - self.record_count)
                .map(|m| m.file_path.as_str());
        }
        let meta_entry_offset = self.metadata_offset + index * METADATA_ENTRY_SIZE;
        let slice = self.slice();
        let entry = slice.get(meta_entry_offset..meta_entry_offset + 8)?;
        let pool = slice.get(self.string_pool_offset..)?;
        let fp_offset = u32::from_le_bytes(entry[0..4].try_into().ok()?) as usize;
        let fp_len = u32::from_le_bytes(entry[4..8].try_into().ok()?) as usize;
        let end = fp_offset.checked_add(fp_len)?;
        std::str::from_utf8(pool.get(fp_offset..end)?).ok()
    }

    /// Returns `true` if `index` (base or LSM delta overlay) has been soft-deleted.
    #[inline]
    pub fn is_deleted(&self, index: usize) -> bool {
        if index < self.record_count {
            let word = index / 64;
            let mask = 1u64 << (index % 64);
            return self
                .tombstones
                .get(word)
                .is_some_and(|&bits| (bits & mask) != 0);
        }
        self.delta_tombstones
            .get(index - self.record_count)
            .copied()
            .unwrap_or(true)
    }

    fn slice(&self) -> &[u8] {
        if let Some(ref m) = self.mmap {
            &m[..]
        } else if let Some(ref b) = self.data_buffer {
            &b[..]
        } else {
            &[]
        }
    }

    /// Reads only the 16-byte `sign_bits` (`BinaryVector128`) prefix of a record (base or delta) for Stage-1 Hamming screening.
    #[inline]
    pub fn get_binary_vector(&self, index: usize) -> Option<BinaryVector128> {
        if index < self.record_count {
            let offset = self.records_offset + index * RECORD_SIZE;
            let bytes = self.slice().get(offset..offset + 16)?;
            return Some(BinaryVector128 {
                bits: [
                    u64::from_le_bytes(bytes[0..8].try_into().ok()?),
                    u64::from_le_bytes(bytes[8..16].try_into().ok()?),
                ],
            });
        }
        self.delta_records
            .get(index - self.record_count)
            .map(TurbovecRecord::binary)
    }

    /// Retrieves a single 56-byte Two-Stage `TurbovecRecord` from the memory-mapped base or LSM delta overlay.
    #[inline]
    pub fn get_record(&self, index: usize) -> Option<TurbovecRecord> {
        if self.is_deleted(index) {
            return None;
        }
        if index < self.record_count {
            let offset = self.records_offset + index * RECORD_SIZE;
            let rec_slice = self.slice().get(offset..offset + RECORD_SIZE)?;
            return TurbovecRecord::from_bytes(rec_slice);
        }
        self.delta_records.get(index - self.record_count).copied()
    }

    fn consider_candidate(
        &self,
        idx: usize,
        query_binary: &BinaryVector128,
        candidate_pool_size: usize,
        has_tombstones: bool,
        top_candidates: &mut BinaryHeap<(u32, usize)>,
    ) {
        if has_tombstones && self.is_deleted(idx) {
            return;
        }
        if let Some(bin) = self.get_binary_vector(idx) {
            let dist = bin.hamming_distance(query_binary);
            if top_candidates.len() < candidate_pool_size {
                top_candidates.push((dist, idx));
            } else if let Some(&(worst_dist, _)) = top_candidates.peek() {
                if dist < worst_dist {
                    top_candidates.pop();
                    top_candidates.push((dist, idx));
                }
            }
        }
    }

    /// Performs Coarse-to-Fine `turbovec` v3 search across live records:
    /// - Stage 0 (when `cluster_count > 0`): Coarse Binary IVF routing over 16-byte centroids (`nprobe` posting lists).
    /// - Stage 1: Bounded max-heap (`<= 256` entries) hardware `POPCNT` Hamming screening over 16-byte sign planes.
    /// - Stage 2: L1-resident `QueryLut2Bit` FastScan reranking over 56-byte Two-Stage (`PolarQuant + QJL`) records.
    pub fn search(&self, query_vec: &[f32], k: usize) -> Vec<(usize, f32)> {
        let live_count = self.active_record_count();
        if live_count == 0 || k == 0 {
            return Vec::new();
        }

        let lut = QueryLut2Bit::from_f32_query(query_vec);
        let candidate_pool_size = (k * 4).clamp(16, 256).min(live_count);

        // Max-heap ordered by `(hamming_distance, index)` so `peek()` is the worst candidate in the top-M pool.
        let mut top_candidates: BinaryHeap<(u32, usize)> =
            BinaryHeap::with_capacity(candidate_pool_size + 1);
        let has_tombstones = self.deleted_count > 0 || self.delta_deleted_count > 0;

        let used_ivf = if self.cluster_count > 0 && self.ivf_offset > 0 {
            self.collect_ivf_candidates(
                &lut.query_binary,
                candidate_pool_size,
                has_tombstones,
                &mut top_candidates,
            ) && (top_candidates.len() + self.delta_record_count()) >= k.min(live_count)
        } else {
            false
        };

        if !used_ivf {
            top_candidates.clear();
            for i in 0..self.record_count {
                self.consider_candidate(
                    i,
                    &lut.query_binary,
                    candidate_pool_size,
                    has_tombstones,
                    &mut top_candidates,
                );
            }
        }

        // Always scan live in-memory LSM delta overlay slots so freshly edited files participate immediately
        for delta_idx in 0..self.delta_records.len() {
            self.consider_candidate(
                self.record_count + delta_idx,
                &lut.query_binary,
                candidate_pool_size,
                has_tombstones,
                &mut top_candidates,
            );
        }

        // Stage 2: 4-way register-blocked Two-Stage (PolarQuant + QJL) LUT FastScan reranking
        let mut candidate_recs: Vec<(usize, TurbovecRecord)> =
            Vec::with_capacity(top_candidates.len());
        for (_, idx) in top_candidates {
            if let Some(rec) = self.get_record(idx) {
                candidate_recs.push((idx, rec));
            }
        }

        let mut scored: Vec<(usize, f32)> = Vec::with_capacity(candidate_recs.len());
        let (chunks, remainder) = candidate_recs.as_chunks::<4>();
        for chunk in chunks {
            let scores = TurbovecRecord::score_batch4_with_lut(
                [&chunk[0].1, &chunk[1].1, &chunk[2].1, &chunk[3].1],
                &lut,
            );
            for lane in 0..4 {
                scored.push((chunk[lane].0, scores[lane]));
            }
        }
        for &(idx, ref rec) in remainder {
            scored.push((idx, rec.score_with_lut(&lut)));
        }

        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        scored.truncate(k);
        scored
    }

    fn collect_ivf_candidates(
        &self,
        query_binary: &BinaryVector128,
        candidate_pool_size: usize,
        has_tombstones: bool,
        top_candidates: &mut BinaryHeap<(u32, usize)>,
    ) -> bool {
        let slice = self.slice();
        let table_len = self.cluster_count * IVF_CLUSTER_ENTRY_SIZE;
        let Some(cluster_table) = slice.get(self.ivf_offset..self.ivf_offset + table_len) else {
            return false;
        };
        let postings_base = self.ivf_offset + table_len;

        let mut ranked_clusters: Vec<(u32, u32, u32)> = Vec::with_capacity(self.cluster_count);
        for c in 0..self.cluster_count {
            let base = c * IVF_CLUSTER_ENTRY_SIZE;
            let entry = &cluster_table[base..base + IVF_CLUSTER_ENTRY_SIZE];
            let Ok(b0) = entry[0..8].try_into() else {
                return false;
            };
            let Ok(b1) = entry[8..16].try_into() else {
                return false;
            };
            let Ok(st) = entry[16..20].try_into() else {
                return false;
            };
            let Ok(ln) = entry[20..24].try_into() else {
                return false;
            };
            let centroid = BinaryVector128 {
                bits: [u64::from_le_bytes(b0), u64::from_le_bytes(b1)],
            };
            let dist = centroid.hamming_distance(query_binary);
            ranked_clusters.push((dist, u32::from_le_bytes(st), u32::from_le_bytes(ln)));
        }

        let nprobe = solve_optimal_ivf_params(self.record_count)
            .map(|(_, p)| p)
            .unwrap_or_else(|| (self.record_count.max(2) as f64).ln().ceil() as usize)
            .min(self.cluster_count)
            .max(1);
        if nprobe < ranked_clusters.len() {
            ranked_clusters.select_nth_unstable_by_key(nprobe - 1, |item| item.0);
        }

        for &(_, list_start, list_len) in ranked_clusters.iter().take(nprobe) {
            let start_byte = postings_base + (list_start as usize) * 4;
            let end_byte = start_byte + (list_len as usize) * 4;
            let Some(postings_bytes) = slice.get(start_byte..end_byte) else {
                return false;
            };
            for &raw in postings_bytes.as_chunks::<4>().0 {
                let rec_idx = u32::from_le_bytes(raw) as usize;
                self.consider_candidate(
                    rec_idx,
                    query_binary,
                    candidate_pool_size,
                    has_tombstones,
                    top_candidates,
                );
            }
        }
        true
    }

    /// Loads chunk metadata directly from the memory map or LSM delta overlay on demand (returns `None` if tombstoned).
    pub fn get_metadata(&self, index: usize) -> Option<ChunkMetadata> {
        if self.is_deleted(index) {
            return None;
        }
        if index >= self.record_count {
            return self.delta_metadata.get(index - self.record_count).cloned();
        }

        let meta_entry_offset = self.metadata_offset + index * METADATA_ENTRY_SIZE;
        let slice = self.slice();
        let entry = slice.get(meta_entry_offset..meta_entry_offset + METADATA_ENTRY_SIZE)?;
        let pool = slice.get(self.string_pool_offset..)?;

        let read_u32 = |pos: usize| -> Option<usize> {
            Some(u32::from_le_bytes(entry.get(pos..pos + 4)?.try_into().ok()?) as usize)
        };
        let read_pool_str = |offset: usize, len: usize| -> Option<String> {
            let end = offset.checked_add(len)?;
            std::str::from_utf8(pool.get(offset..end)?)
                .ok()
                .map(|s| s.to_string())
        };

        let file_path = read_pool_str(read_u32(0)?, read_u32(4)?)?;
        let start_line = read_u32(8)?;
        let end_line = read_u32(12)?;
        let sym_name_offset = read_u32(16)?;
        let sym_name_len = read_u32(20)?;
        let sym_kind_offset = read_u32(24)?;
        let sym_kind_len = read_u32(28)?;
        let content = read_pool_str(read_u32(32)?, read_u32(36)?)?;

        let symbol_name = if sym_name_len > 0 {
            read_pool_str(sym_name_offset, sym_name_len)
        } else {
            None
        };

        let symbol_kind = if sym_kind_len > 0 {
            read_pool_str(sym_kind_offset, sym_kind_len)
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

    /// Builds deterministic Coarse Binary IVF partitions over 128-bit sign planes using
    /// convergence-driven majority-vote Binary $K$-Means when cost-optimal under `solve_optimal_ivf_params`.
    fn build_binary_ivf(records: &[TurbovecRecord]) -> (usize, Vec<u8>, Vec<u8>) {
        let n = records.len();
        let Some((k_clusters, _)) = solve_optimal_ivf_params(n) else {
            return (0, Vec::new(), Vec::new());
        };

        let mut centroids: Vec<BinaryVector128> = (0..k_clusters)
            .map(|c| records[(c * n) / k_clusters].binary())
            .collect();
        let mut assignments = vec![usize::MAX; n];

        // Iterate majority-vote Hamming medoids until assignment fixed-point convergence (bounded at 12)
        for _ in 0..12 {
            let mut changed = false;
            let mut counts = vec![0u32; k_clusters];
            let mut bit_votes = vec![[0u32; 128]; k_clusters];

            for (i, rec) in records.iter().enumerate() {
                let bin = rec.binary();
                let mut best_c = 0usize;
                let mut best_dist = u32::MAX;
                for (c, centroid) in centroids.iter().enumerate() {
                    let d = bin.hamming_distance(centroid);
                    if d < best_dist {
                        best_dist = d;
                        best_c = c;
                    }
                }
                if assignments[i] != best_c {
                    assignments[i] = best_c;
                    changed = true;
                }
                counts[best_c] += 1;
                let votes = &mut bit_votes[best_c];
                for bit in 0..64 {
                    votes[bit] += ((bin.bits[0] >> bit) & 1) as u32;
                    votes[64 + bit] += ((bin.bits[1] >> bit) & 1) as u32;
                }
            }

            if !changed {
                break;
            }

            for c in 0..k_clusters {
                let cluster_len = counts[c];
                if cluster_len == 0 {
                    continue;
                }
                let mut w0 = 0u64;
                let mut w1 = 0u64;
                let votes = &bit_votes[c];
                for bit in 0..64 {
                    if votes[bit] * 2 >= cluster_len {
                        w0 |= 1u64 << bit;
                    }
                    if votes[64 + bit] * 2 >= cluster_len {
                        w1 |= 1u64 << bit;
                    }
                }
                centroids[c] = BinaryVector128 { bits: [w0, w1] };
            }
        }

        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); k_clusters];
        for (i, &c) in assignments.iter().enumerate() {
            buckets[c].push(i as u32);
        }

        let mut cluster_table = Vec::with_capacity(k_clusters * IVF_CLUSTER_ENTRY_SIZE);
        let mut postings_bytes = Vec::with_capacity(n * 4);
        let mut running_start = 0u32;

        for (c, bucket) in buckets.iter().enumerate() {
            let len = bucket.len() as u32;
            cluster_table.extend_from_slice(&centroids[c].bits[0].to_le_bytes());
            cluster_table.extend_from_slice(&centroids[c].bits[1].to_le_bytes());
            cluster_table.extend_from_slice(&running_start.to_le_bytes());
            cluster_table.extend_from_slice(&len.to_le_bytes());
            for &rec_idx in bucket {
                postings_bytes.extend_from_slice(&rec_idx.to_le_bytes());
            }
            running_start += len;
        }

        (k_clusters, cluster_table, postings_bytes)
    }

    /// Serializes chunks into the 56-byte Two-Stage Turbovec v3 format (with optional Coarse Binary IVF) atomically.
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

        let temp_path = path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let file = File::create(&temp_path).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to create index file: {}", e))
        })?;
        let mut writer = BufWriter::new(file);

        let record_count = records.len();
        let records_offset = HEADER_SIZE;
        let metadata_offset = records_offset + record_count * RECORD_SIZE;

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
        let (cluster_count, cluster_table, postings_bytes) = Self::build_binary_ivf(records);
        let raw_after_pool = string_pool_offset + string_pool.len();
        let align_pad = (8 - (raw_after_pool % 8)) % 8;
        let ivf_offset = if cluster_count > 0 {
            raw_after_pool + align_pad
        } else {
            0
        };

        let mut header = [0u8; HEADER_SIZE];
        header[0..8].copy_from_slice(MAGIC_BYTES);
        header[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        header[12..16].copy_from_slice(&128u32.to_le_bytes());
        header[16..20].copy_from_slice(&(record_count as u32).to_le_bytes());
        header[20..28].copy_from_slice(&(records_offset as u64).to_le_bytes());
        header[28..36].copy_from_slice(&(metadata_offset as u64).to_le_bytes());
        header[36..44].copy_from_slice(&(string_pool_offset as u64).to_le_bytes());
        header[44..52].copy_from_slice(&(ivf_offset as u64).to_le_bytes());
        header[52..56].copy_from_slice(&(cluster_count as u32).to_le_bytes());

        writer
            .write_all(&header)
            .map_err(|e| ContextError::VectorIndex(format!("Failed to write header: {}", e)))?;

        for rec in records {
            writer
                .write_all(&rec.to_bytes())
                .map_err(|e| ContextError::VectorIndex(format!("Failed to write record: {}", e)))?;
        }

        writer.write_all(&meta_entries).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to write metadata table: {}", e))
        })?;

        writer.write_all(&string_pool).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to write string pool: {}", e))
        })?;

        if cluster_count > 0 {
            if align_pad > 0 {
                let zeros = [0u8; 8];
                writer.write_all(&zeros[..align_pad]).map_err(|e| {
                    ContextError::VectorIndex(format!("Failed to write IVF alignment pad: {}", e))
                })?;
            }
            writer.write_all(&cluster_table).map_err(|e| {
                ContextError::VectorIndex(format!("Failed to write IVF cluster table: {}", e))
            })?;
            writer.write_all(&postings_bytes).map_err(|e| {
                ContextError::VectorIndex(format!("Failed to write IVF postings: {}", e))
            })?;
        }

        writer
            .flush()
            .map_err(|e| ContextError::VectorIndex(format!("Failed to flush index: {}", e)))?;
        drop(writer);

        fs::rename(&temp_path, path).map_err(|e| {
            ContextError::VectorIndex(format!("Failed to atomically rename index: {}", e))
        })?;

        // Freshly compacted index supersedes any previous companion tombstone file
        let _ = fs::remove_file(Self::tombstone_path(path));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mmap_turbovec_v3_roundtrip_search_and_persisted_tombstones() {
        let temp_dir = std::env::temp_dir().join(format!(
            "turbovec_v3_test_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let _ = fs::create_dir_all(&temp_dir);
        let index_path = temp_dir.join("test_index.bin");

        let mut records = Vec::new();
        let mut metadata = Vec::new();

        for i in 0..50 {
            let content = format!(
                "pub fn fn_test_{}() {{ tracing::info!(\"distinct token key_{}\"); }}",
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

        MmapTurbovecIndex::create(&index_path, &records, &metadata).expect("create failed");

        let mut index = MmapTurbovecIndex::open(&index_path).expect("open failed");
        assert_eq!(index.record_count(), 50);
        assert_eq!(index.active_record_count(), 50);
        assert_eq!(index.cluster_count(), 0, "Flat scan below IVF_MIN_RECORDS");

        let meta0 = index.get_metadata(0).expect("metadata 0 missing");
        assert_eq!(meta0.file_path, "src/module_0.rs");
        assert_eq!(meta0.symbol_name.as_deref(), Some("fn_test_0"));

        let query_text = "fn_test_10 token key_10";
        let query = crate::context::search::semantic::SemanticIndex::embed(query_text);

        let results = index.search(&query, 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 10, "Record 10 should be the top match");
        assert!(
            results[0].1 > 0.3,
            "Top match should have strong similarity"
        );

        // Verify O(1) file-predicate soft-delete + persisted .tomb reload without rewriting .bin
        let deleted = index.mark_deleted_where_file(|fp| fp == "src/module_10.rs");
        assert_eq!(deleted, 1);
        assert!(index.is_deleted(10));
        assert!(index.get_metadata(10).is_none());
        index
            .persist_tombstones(&index_path)
            .expect("persist tombstones failed");
        drop(index);

        let reopened = MmapTurbovecIndex::open(&index_path).expect("reopen failed");
        assert_eq!(reopened.deleted_count(), 1);
        assert_eq!(reopened.active_record_count(), 49);
        assert!(reopened.is_deleted(10));
        let after_delete = reopened.search(&query, 5);
        assert!(
            after_delete.iter().all(|&(idx, _)| idx != 10),
            "Persisted tombstoned record 10 must stay excluded across reopen"
        );

        let _ = fs::remove_file(MmapTurbovecIndex::tombstone_path(&index_path));
        let _ = fs::remove_file(&index_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mmap_turbovec_v3_coarse_binary_ivf_routing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "turbovec_v3_ivf_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let _ = fs::create_dir_all(&temp_dir);
        let index_path = temp_dir.join("ivf_index.bin");

        let mut records = Vec::new();
        let mut metadata = Vec::new();

        for i in 0..324 {
            let content = format!(
                "pub struct Component_{idx} {{ pub handler_{idx}: usize, pub domain_tag_{bucket}: String }}",
                idx = i,
                bucket = i % 18
            );
            let vec = crate::context::search::semantic::SemanticIndex::embed(&content);
            records.push(TurbovecRecord::from_f32_with_wht(&vec));
            metadata.push(ChunkMetadata {
                file_path: format!("src/pkg_{}/comp_{}.rs", i % 18, i),
                start_line: 1,
                end_line: 20,
                symbol_name: Some(format!("Component_{}", i)),
                symbol_kind: Some("struct".to_string()),
                content,
            });
        }

        MmapTurbovecIndex::create(&index_path, &records, &metadata).expect("create ivf failed");
        let index = MmapTurbovecIndex::open(&index_path).expect("open ivf failed");
        assert_eq!(
            index.cluster_count(),
            18,
            "324 records should build sqrt(324)=18 Coarse Binary IVF clusters"
        );

        let query = crate::context::search::semantic::SemanticIndex::embed(
            "Component_217 handler_217 domain_tag_1",
        );
        let results = index.search(&query, 5);
        assert!(!results.is_empty());
        assert_eq!(
            results[0].0, 217,
            "Coarse Binary IVF must route query to exact target record 217"
        );

        let _ = fs::remove_file(&index_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mmap_turbovec_lsm_delta_overlay_and_compaction_trigger() {
        let temp_dir = std::env::temp_dir().join(format!(
            "turbovec_v3_lsm_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let _ = fs::create_dir_all(&temp_dir);
        let index_path = temp_dir.join("lsm_index.bin");

        let mut records = Vec::new();
        let mut metadata = Vec::new();
        for i in 0..40 {
            let content = format!("pub fn base_handler_{}() {{ /* base_{} */ }}", i, i);
            let vec = crate::context::search::semantic::SemanticIndex::embed(&content);
            records.push(TurbovecRecord::from_f32_with_wht(&vec));
            metadata.push(ChunkMetadata {
                file_path: format!("src/base_{}.rs", i),
                start_line: 1,
                end_line: 10,
                symbol_name: Some(format!("base_handler_{}", i)),
                symbol_kind: Some("function".to_string()),
                content,
            });
        }

        MmapTurbovecIndex::create(&index_path, &records, &metadata).expect("create failed");
        let mut index = MmapTurbovecIndex::open(&index_path).expect("open failed");
        assert!(!index.needs_compaction());

        // Tombstone old file and append new delta record without rewriting `.bin`
        let del = index.mark_deleted_where_file(|fp| fp == "src/base_7.rs");
        assert_eq!(del, 1);

        let updated_content =
            "pub fn quantum_flux_capacitor_handler() { /* freshly edited overlay symbol */ }"
                .to_string();
        let updated_vec = crate::context::search::semantic::SemanticIndex::embed(&updated_content);
        let appended = index.append_delta_records(
            vec![TurbovecRecord::from_f32_with_wht(&updated_vec)],
            vec![ChunkMetadata {
                file_path: "src/base_7.rs".to_string(),
                start_line: 1,
                end_line: 12,
                symbol_name: Some("quantum_flux_capacitor_handler".to_string()),
                symbol_kind: Some("function".to_string()),
                content: updated_content,
            }],
        );
        assert_eq!(appended, 1);
        assert_eq!(index.active_record_count(), 40);
        assert_eq!(index.delta_record_count(), 1);
        assert!(!index.needs_compaction());

        // Cheap O(1) Arc-backed clone preserves overlay visibility
        let cloned = index.clone();
        let q = crate::context::search::semantic::SemanticIndex::embed(
            "quantum_flux_capacitor_handler freshly edited overlay",
        );
        let hits = cloned.search(&q, 3);
        assert!(!hits.is_empty());
        assert_eq!(
            hits[0].0, 40,
            "Top hit must resolve to LSM delta virtual slot 40"
        );
        let meta = cloned
            .get_metadata(40)
            .expect("delta metadata must resolve");
        assert_eq!(
            meta.symbol_name.as_deref(),
            Some("quantum_flux_capacitor_handler")
        );

        // Verify tombstone fragmentation compaction threshold (> 25% deleted)
        for i in 0..12 {
            index.mark_deleted(i);
        }
        assert!(
            index.needs_compaction(),
            "Deleting >25% of base slots must trigger compaction"
        );

        let _ = fs::remove_file(&index_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
