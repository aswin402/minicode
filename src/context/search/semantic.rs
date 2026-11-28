use crate::context::repomap::RepoMapExtractor;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const VECTOR_DIM: usize = 128;

/// A localized source code chunk with line numbers, AST symbol metadata, and dense embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeChunk {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vector: Vec<f32>,
    #[serde(default)]
    pub symbol_name: Option<String>,
    #[serde(default)]
    pub symbol_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_vector: Option<crate::context::search::quantize::BinaryVector128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polar_quant: Option<crate::context::search::quantize::PolarQuant4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polar_quant_fixed: Option<crate::context::search::quantize::PolarQuant4Fixed>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polar_quant2: Option<crate::context::search::quantize::PolarQuant2Fixed>,
}

enum ChunkVectorRepr<'a> {
    PolarQuant2(crate::context::search::quantize::PolarQuant2Fixed),
    DenseF32(&'a [f32]),
    PolarQuant4Fixed(&'a crate::context::search::quantize::PolarQuant4Fixed),
    PolarQuant4(&'a crate::context::search::quantize::PolarQuant4),
    Binary128(&'a crate::context::search::quantize::BinaryVector128),
    Empty,
}

impl CodeChunk {
    #[inline]
    fn best_vector_repr(&self) -> ChunkVectorRepr<'_> {
        if let Some(pq2) = self.polar_quant2 {
            ChunkVectorRepr::PolarQuant2(pq2)
        } else if !self.vector.is_empty() {
            ChunkVectorRepr::DenseF32(&self.vector)
        } else if let Some(ref pqf) = self.polar_quant_fixed {
            ChunkVectorRepr::PolarQuant4Fixed(pqf)
        } else if let Some(ref pq) = self.polar_quant {
            ChunkVectorRepr::PolarQuant4(pq)
        } else if let Some(ref bv) = self.binary_vector {
            ChunkVectorRepr::Binary128(bv)
        } else {
            ChunkVectorRepr::Empty
        }
    }

    pub fn get_polar_quant2(&self) -> crate::context::search::quantize::PolarQuant2Fixed {
        use crate::context::search::quantize::PolarQuant2Fixed;
        match self.best_vector_repr() {
            ChunkVectorRepr::PolarQuant2(pq2) => pq2,
            ChunkVectorRepr::DenseF32(v) => PolarQuant2Fixed::from_f32_with_wht(v),
            ChunkVectorRepr::PolarQuant4Fixed(pqf) => {
                PolarQuant2Fixed::from_f32_with_wht(&pqf.dequantize())
            }
            ChunkVectorRepr::PolarQuant4(pq) => {
                PolarQuant2Fixed::from_f32_with_wht(&pq.dequantize())
            }
            ChunkVectorRepr::Binary128(_) | ChunkVectorRepr::Empty => PolarQuant2Fixed::default(),
        }
    }

    pub fn get_binary_vector(&self) -> crate::context::search::quantize::BinaryVector128 {
        if let Some(pq2) = self.polar_quant2 {
            pq2.binary_vector()
        } else if let Some(bv) = self.binary_vector {
            bv
        } else {
            crate::context::search::quantize::BinaryVector128::from_f32_with_wht(&self.vector)
        }
    }

    #[allow(dead_code)]
    pub fn get_polar_quant(&self) -> crate::context::search::quantize::PolarQuant4 {
        self.polar_quant.clone().unwrap_or_else(|| {
            crate::context::search::quantize::PolarQuant4::from_f32_slice(&self.vector)
        })
    }

    #[allow(dead_code)]
    pub fn get_polar_quant_fixed(&self) -> crate::context::search::quantize::PolarQuant4Fixed {
        self.polar_quant_fixed.unwrap_or_else(|| {
            if let Some(ref pq) = self.polar_quant {
                pq.clone().into()
            } else {
                crate::context::search::quantize::PolarQuant4Fixed::from_f32_slice(&self.vector)
            }
        })
    }

    /// FastScan 2-bit asymmetric dot product using a precomputed `QueryLut2Bit`.
    #[inline]
    pub fn score_with_lut(&self, lut: &crate::context::search::quantize::QueryLut2Bit) -> f32 {
        match self.polar_quant2 {
            Some(pq2) => pq2.score_with_lut(lut),
            None => self.get_polar_quant2().score_with_lut(lut),
        }
    }

    /// Computes similarity score against an un-rotated `f32` query vector using 2-bit Lloyd-Max FastScan
    /// (or full `f32` cosine similarity if legacy raw vector is present).
    pub fn similarity_to_query(&self, query_vec: &[f32]) -> f32 {
        match self.best_vector_repr() {
            ChunkVectorRepr::PolarQuant2(pq2) => pq2.asymmetric_dot_product_with_wht(query_vec),
            ChunkVectorRepr::DenseF32(v) => SemanticIndex::cosine_similarity(query_vec, v),
            ChunkVectorRepr::PolarQuant4Fixed(pqf) => pqf.asymmetric_dot_product(query_vec),
            ChunkVectorRepr::PolarQuant4(pq) => pq.asymmetric_dot_product(query_vec),
            ChunkVectorRepr::Binary128(bv) => {
                let q_bin =
                    crate::context::search::quantize::BinaryVector128::from_f32_slice(query_vec);
                bv.cosine_similarity(&q_bin)
            }
            ChunkVectorRepr::Empty => 0.0,
        }
    }
}

/// A search result from semantic code search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticSearchResult {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub similarity_score: f32,
    pub snippet: String,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
}

/// Persistent cache format for the semantic vector index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticCache {
    pub file_hashes: HashMap<String, u64>,
    pub chunks: Vec<CodeChunk>,
}

pub struct SemanticIndex {
    pub chunks: Vec<CodeChunk>,
    pub file_hashes: HashMap<String, u64>,
    pub mmap_index: Option<crate::context::search::mmap_index::MmapTurbovecIndex>,
}

type CachedSemanticEntry = (PathBuf, std::time::Instant, std::sync::Arc<SemanticIndex>);
static GLOBAL_SEMANTIC_INDEX_CACHE: std::sync::OnceLock<
    std::sync::RwLock<Option<CachedSemanticEntry>>,
> = std::sync::OnceLock::new();

impl Default for SemanticIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticIndex {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            file_hashes: HashMap::new(),
            mmap_index: None,
        }
    }

    /// Invalidates the process-wide cached `SemanticIndex`.
    pub fn invalidate_cache() {
        if let Some(cache) = GLOBAL_SEMANTIC_INDEX_CACHE.get() {
            if let Ok(mut guard) = cache.write() {
                *guard = None;
            }
        }
        crate::context::search::index_bus::WorkspaceIndexBus::clear_consumer_queue(
            crate::context::search::index_bus::IndexConsumer::Semantic,
        );
    }

    /// Applies an $O(|\Delta|)$ incremental AST-chunk update for dirty workspace files without
    /// re-walking or re-embedding unchanged files.
    pub fn apply_incremental_delta(
        &mut self,
        workspace_root: &Path,
        deltas: &[(String, crate::context::search::index_bus::FileMutationOp)],
    ) -> usize {
        use crate::context::search::index_bus::FileMutationOp;
        if deltas.is_empty() {
            return 0;
        }

        let mut dirty_set = std::collections::HashSet::with_capacity(deltas.len());
        for (rel, _) in deltas {
            let norm = rel.trim_start_matches("./").replace('\\', "/");
            if !norm.is_empty() {
                dirty_set.insert(norm);
            }
        }
        if dirty_set.is_empty() {
            return 0;
        }

        // Drop stale chunks and hashes for dirty/deleted files
        self.chunks.retain(|c| !dirty_set.contains(&c.file_path));
        for rel in &dirty_set {
            self.file_hashes.remove(rel);
        }

        let mut repomap = RepoMapExtractor::new();
        let mut updated_files = 0usize;
        let mut newly_chunked: Vec<CodeChunk> = Vec::new();

        for (rel, op) in deltas {
            if *op == FileMutationOp::Delete {
                continue;
            }
            let norm = rel.trim_start_matches("./").replace('\\', "/");
            if norm.is_empty() {
                continue;
            }
            let abs_path = workspace_root.join(&norm);
            let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !is_indexable_extension(ext) || !abs_path.is_file() {
                continue;
            }
            let mtime = fs::metadata(&abs_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if let Ok(content) = fs::read_to_string(&abs_path) {
                let chunks = chunk_source_code_ast(&norm, &content, &mut repomap, &abs_path);
                newly_chunked.extend(chunks);
                self.file_hashes.insert(norm, mtime);
                updated_files += 1;
            }
        }

        // Zero-Rewrite LSM Delta Overlay fast-path: tombstone modified/deleted base slots in O(1),
        // append freshly quantized records to the in-memory delta overlay, and avoid rewriting `.bin`
        // until compaction thresholds (`>25%` deleted or `>256` delta records) are exceeded.
        let mut served_by_lsm_overlay = false;
        let cache_path = Self::cache_path(workspace_root);
        let bin_path = cache_path.with_extension("bin");
        let hashes_path = cache_path.with_file_name("semantic_hashes.json");

        if let Some(ref mut mmap) = self.mmap_index {
            use crate::context::search::mmap_index::ChunkMetadata;
            use crate::context::search::quantize::TurbovecRecord;

            mmap.mark_deleted_where_file(|fp| dirty_set.contains(fp));

            let mut delta_records = Vec::with_capacity(newly_chunked.len());
            let mut delta_metadata = Vec::with_capacity(newly_chunked.len());
            for chunk in &newly_chunked {
                delta_records.push(TurbovecRecord::from_quant2(chunk.get_polar_quant2()));
                delta_metadata.push(ChunkMetadata {
                    file_path: chunk.file_path.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    symbol_name: chunk.symbol_name.clone(),
                    symbol_kind: chunk.symbol_kind.clone(),
                    content: chunk.content.clone(),
                });
            }
            mmap.append_delta_records(delta_records, delta_metadata);

            if !mmap.needs_compaction() {
                served_by_lsm_overlay = true;
                if workspace_root.join(".minicode").is_dir() && bin_path.exists() {
                    let _ = mmap.persist_tombstones(&bin_path);
                    // Omit files residing solely in the in-memory delta overlay from disk hashes
                    // so a cold process restart automatically re-embeds those few delta files.
                    let delta_files: std::collections::HashSet<String> = mmap
                        .delta_file_paths()
                        .into_iter()
                        .map(str::to_string)
                        .collect();
                    let disk_hashes: HashMap<&String, &u64> = self
                        .file_hashes
                        .iter()
                        .filter(|(k, _)| !delta_files.contains(k.as_str()))
                        .collect();
                    if let Ok(bytes) = serde_json::to_vec(&disk_hashes) {
                        let _ = fs::write(&hashes_path, bytes);
                    }
                }
            }
        }

        self.chunks.extend(newly_chunked);

        if !served_by_lsm_overlay {
            self.mmap_index = None;
            if workspace_root.join(".minicode").is_dir() {
                self.save_cache(&cache_path);
            }
        }
        updated_files
    }

    /// Returns a shared in-memory `Arc<SemanticIndex>` for `workspace_root`, reusing the cached
    /// index across calls and applying $O(|\Delta|)$ incremental delta updates via `WorkspaceIndexBus`.
    pub fn get_or_build_cached(workspace_root: &Path) -> Result<std::sync::Arc<Self>> {
        use crate::context::search::index_bus::{IndexConsumer, WorkspaceIndexBus};
        const MAX_INCREMENTAL_DELTAS: usize = 64;

        let canonical_root =
            fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
        let cache = GLOBAL_SEMANTIC_INDEX_CACHE.get_or_init(|| std::sync::RwLock::new(None));
        let dirty = WorkspaceIndexBus::drain_dirty(workspace_root, IndexConsumer::Semantic);

        if dirty.is_empty() {
            if let Ok(guard) = cache.read() {
                if let Some((ref cached_root, built_at, ref arc_idx)) = *guard {
                    if cached_root == &canonical_root
                        && built_at.elapsed() < std::time::Duration::from_secs(30)
                    {
                        return Ok(std::sync::Arc::clone(arc_idx));
                    }
                }
            }
        } else if dirty.len() <= MAX_INCREMENTAL_DELTAS {
            if let Ok(mut guard) = cache.write() {
                if let Some((ref cached_root, ref mut built_at, ref mut arc_idx)) = *guard {
                    if cached_root == &canonical_root {
                        let mut patched = Self {
                            chunks: arc_idx.chunks.clone(),
                            file_hashes: arc_idx.file_hashes.clone(),
                            mmap_index: arc_idx.mmap_index.clone(),
                        };
                        patched.apply_incremental_delta(workspace_root, &dirty);
                        let arc_patched = std::sync::Arc::new(patched);
                        *arc_idx = std::sync::Arc::clone(&arc_patched);
                        *built_at = std::time::Instant::now();
                        return Ok(arc_patched);
                    }
                }
            }
        }

        let mut built = Self::new();
        built.build_index(workspace_root)?;
        let arc_built = std::sync::Arc::new(built);
        if let Ok(mut guard) = cache.write() {
            *guard = Some((
                canonical_root,
                std::time::Instant::now(),
                std::sync::Arc::clone(&arc_built),
            ));
        }
        Ok(arc_built)
    }

    /// Embeds a text snippet into a normalized dense vector (128 dimensions) using
    /// deterministic character 3-gram and subword hashing projection (Model2Vec/FastText style).
    pub fn embed(text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; VECTOR_DIM];
        let lower = text.to_lowercase();
        let tokens: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .collect();

        if tokens.is_empty() {
            return vec;
        }

        for token in &tokens {
            // Project full token
            let h = hash_token(token);
            let idx = (h as usize) % VECTOR_DIM;
            let sign = if (h >> 16) & 1 == 0 { 1.0 } else { -1.0 };
            vec[idx] += sign * 1.5;

            // Project 3-grams
            if token.is_ascii() {
                if token.len() >= 3 {
                    for i in 0..=token.len() - 3 {
                        let gram = &token[i..i + 3];
                        let gh = hash_token(gram);
                        let gidx = (gh as usize) % VECTOR_DIM;
                        let gsign = if (gh >> 8) & 1 == 0 { 1.0 } else { -1.0 };
                        vec[gidx] += gsign * 0.8;
                    }
                }
            } else {
                let chars: Vec<char> = token.chars().collect();
                if chars.len() >= 3 {
                    for window in chars.windows(3) {
                        let gram: String = window.iter().collect();
                        let gh = hash_token(&gram);
                        let gidx = (gh as usize) % VECTOR_DIM;
                        let gsign = if (gh >> 8) & 1 == 0 { 1.0 } else { -1.0 };
                        vec[gidx] += gsign * 0.8;
                    }
                }
            }
        }

        // Normalize vector to unit length (L2 norm)
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-6 {
            for val in &mut vec {
                *val /= norm;
            }
        }

        vec
    }

    /// Computes cosine similarity between two unit-normalized vectors.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        dot.clamp(-1.0, 1.0)
    }

    /// Builds or incrementally updates the semantic vector index from the workspace directory.
    pub fn build_index(&mut self, workspace_root: &Path) -> Result<usize> {
        let cache_path = Self::cache_path(workspace_root);
        self.load_cache(&cache_path);

        let mut indexed_count = 0;
        let rel_files =
            crate::context::walker::WorkspaceWalker::new(workspace_root).collect_relative_files();
        let mut repomap = RepoMapExtractor::new();

        let mut active_files = std::collections::HashSet::with_capacity(rel_files.len());
        let mut dirty_files = std::collections::HashSet::new();
        let mut new_chunks = Vec::new();

        for rel_path in rel_files {
            let path = workspace_root.join(&rel_path);
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !is_indexable_extension(ext) {
                continue;
            }
            active_files.insert(rel_path.clone());

            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };

            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if self.file_hashes.get(&rel_path) == Some(&mtime) {
                continue; // Unchanged file
            }

            // Read and chunk file using AST-aware symbol chunker
            if let Ok(content) = fs::read_to_string(&path) {
                let chunks = chunk_source_code_ast(&rel_path, &content, &mut repomap, &path);
                dirty_files.insert(rel_path.clone());
                new_chunks.extend(chunks);
                self.file_hashes.insert(rel_path, mtime);
                indexed_count += 1;
            }
        }

        let hashes_before = self.file_hashes.len();
        self.file_hashes.retain(|k, _| active_files.contains(k));
        let pruned_deleted = self.file_hashes.len() != hashes_before;

        if !dirty_files.is_empty() || pruned_deleted {
            self.chunks.retain(|c| {
                active_files.contains(&c.file_path) && !dirty_files.contains(&c.file_path)
            });
            self.chunks.extend(new_chunks);
        }

        // Fast delete-only path: mask removed files in O(1) via persisted .tomb without rewriting .bin
        // (automatically compacts via save_cache once tombstones exceed 25% of physical slots).
        if indexed_count == 0 && dirty_files.is_empty() && pruned_deleted {
            if let Some(ref mut mmap) = self.mmap_index {
                mmap.mark_deleted_where_file(|fp| !active_files.contains(fp));
                if mmap.deleted_count() * 4 <= mmap.record_count() {
                    let bin_path = cache_path.with_extension("bin");
                    let hashes_path = cache_path.with_file_name("semantic_hashes.json");
                    if let Ok(bytes) = serde_json::to_vec(&self.file_hashes) {
                        let _ = fs::write(&hashes_path, bytes);
                    }
                    if mmap.persist_tombstones(&bin_path).is_ok() {
                        return Ok(0);
                    }
                }
            }
        }

        if indexed_count > 0 || pruned_deleted || self.mmap_index.is_none() {
            self.save_cache(&cache_path);
        }
        Ok(indexed_count)
    }

    fn chunk_to_search_result(chunk: &CodeChunk, similarity_score: f32) -> SemanticSearchResult {
        SemanticSearchResult {
            file_path: chunk.file_path.clone(),
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            similarity_score,
            snippet: chunk.content.clone(),
            symbol_name: chunk.symbol_name.clone(),
            symbol_kind: chunk.symbol_kind.clone(),
        }
    }

    /// Fast 2-stage search: stage 1 screens candidate chunks using hardware `POPCNT` on 16-byte
    /// sign bit-planes (with Coarse Binary IVF on large workspaces); stage 2 scores top candidates
    /// via `QueryLut2Bit` Two-Stage (`PolarQuant + QJL`) L1 FastScan.
    pub fn search_fast(&self, query: &str, limit: usize) -> Vec<SemanticSearchResult> {
        if self.chunks.is_empty() && self.mmap_index.is_none() {
            return Vec::new();
        }

        let query_vec = Self::embed(query);

        // Fast path: use zero-copy Two-Stage TurboQuant v3 memory map if available
        if let Some(ref mmap) = self.mmap_index {
            let top_k = mmap.search(&query_vec, limit);
            let mut results = Vec::with_capacity(top_k.len());
            for (idx, score) in top_k {
                if score > 0.05 {
                    if let Some(meta) = mmap.get_metadata(idx) {
                        results.push(SemanticSearchResult {
                            file_path: meta.file_path,
                            start_line: meta.start_line,
                            end_line: meta.end_line,
                            similarity_score: score,
                            snippet: meta.content,
                            symbol_name: meta.symbol_name,
                            symbol_kind: meta.symbol_kind,
                        });
                    }
                }
            }
            return results;
        }

        let lut = crate::context::search::quantize::QueryLut2Bit::from_f32_query(&query_vec);

        // Stage 1: Bounded max-heap hardware popcount Hamming screening
        let candidate_pool_size = (limit * 4).clamp(32, 256).min(self.chunks.len());
        let mut top_candidates: std::collections::BinaryHeap<(u32, usize)> =
            std::collections::BinaryHeap::with_capacity(candidate_pool_size + 1);

        for (idx, chunk) in self.chunks.iter().enumerate() {
            let dist = chunk
                .get_binary_vector()
                .hamming_distance(&lut.query_binary);
            if top_candidates.len() < candidate_pool_size {
                top_candidates.push((dist, idx));
            } else if let Some(&(worst_dist, _)) = top_candidates.peek() {
                if dist < worst_dist {
                    top_candidates.pop();
                    top_candidates.push((dist, idx));
                }
            }
        }

        // Stage 2: 2-bit Lloyd-Max LUT FastScan scoring on bounded candidate pool
        let mut scored: Vec<(f32, &CodeChunk)> = top_candidates
            .into_iter()
            .filter_map(|(_, idx)| {
                let chunk = self.chunks.get(idx)?;
                let score = chunk.score_with_lut(&lut);
                (score > 0.05).then_some((score, chunk))
            })
            .collect();

        scored.sort_by(|a, b| b.0.total_cmp(&a.0));

        scored
            .into_iter()
            .take(limit)
            .map(|(score, chunk)| Self::chunk_to_search_result(chunk, score))
            .collect()
    }

    /// Searches the indexed codebase for chunks semantically matching the query.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SemanticSearchResult> {
        if self.mmap_index.is_some() || self.chunks.len() > 64 {
            return self.search_fast(query, limit);
        }

        let query_vec = Self::embed(query);
        let lut = crate::context::search::quantize::QueryLut2Bit::from_f32_query(&query_vec);
        let mut scored: Vec<(f32, &CodeChunk)> = self
            .chunks
            .iter()
            .map(|chunk| (chunk.score_with_lut(&lut), chunk))
            .filter(|(score, _)| *score > 0.05)
            .collect();

        scored.sort_by(|a, b| b.0.total_cmp(&a.0));

        scored
            .into_iter()
            .take(limit)
            .map(|(score, chunk)| Self::chunk_to_search_result(chunk, score))
            .collect()
    }

    /// Specifically searches for AST symbol definitions (functions, classes, structs) matching the query.
    pub fn search_symbols(&self, query: &str, limit: usize) -> Vec<SemanticSearchResult> {
        if self.chunks.is_empty() {
            return Vec::new();
        }

        let query_vec = Self::embed(query);
        let lut = crate::context::search::quantize::QueryLut2Bit::from_f32_query(&query_vec);
        let query_lower = query.to_lowercase();

        let symbol_chunks: Vec<&CodeChunk> = self
            .chunks
            .iter()
            .filter(|chunk| chunk.symbol_name.is_some())
            .collect();

        if symbol_chunks.is_empty() {
            return Vec::new();
        }

        // Fast 2-stage screening for symbol search: stage 1 popcount + name boost, stage 2 2-bit LUT dot product
        let candidate_pool_size = (limit * 4).clamp(32, 256).min(symbol_chunks.len());
        let mut candidates: Vec<(u32, &CodeChunk)> = symbol_chunks
            .into_iter()
            .map(|c| {
                let dist = c.get_binary_vector().hamming_distance(&lut.query_binary);
                let name_discount = if let Some(ref sym) = c.symbol_name {
                    if sym.eq_ignore_ascii_case(query) {
                        24
                    } else if sym.to_lowercase().contains(&query_lower) {
                        12
                    } else {
                        0
                    }
                } else {
                    0
                };
                (dist.saturating_sub(name_discount), c)
            })
            .collect();

        candidates.sort_by_key(|c| c.0);

        let mut scored: Vec<(f32, &CodeChunk)> = candidates
            .into_iter()
            .take(candidate_pool_size)
            .map(|(_, chunk)| {
                let base_score = chunk.score_with_lut(&lut);
                let name_boost = if let Some(ref sym) = chunk.symbol_name {
                    if sym.eq_ignore_ascii_case(query) {
                        0.4
                    } else if sym.to_lowercase().contains(&query_lower) {
                        0.2
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                (base_score + name_boost, chunk)
            })
            .collect();

        scored.sort_by(|a, b| b.0.total_cmp(&a.0));

        scored
            .into_iter()
            .take(limit)
            .map(|(score, chunk)| Self::chunk_to_search_result(chunk, score))
            .collect()
    }

    pub fn cache_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".minicode")
            .join("cache")
            .join("semantic_index.json")
    }

    #[allow(dead_code)]
    pub fn binary_cache_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".minicode")
            .join("cache")
            .join("semantic_index.bin")
    }

    #[allow(dead_code)]
    pub fn hashes_cache_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".minicode")
            .join("cache")
            .join("semantic_hashes.json")
    }

    fn load_cache(&mut self, cache_path: &Path) {
        let bin_path = cache_path.with_extension("bin");
        let hashes_path = cache_path.with_file_name("semantic_hashes.json");

        // 1. Try loading binary 2-bit Turbovec v2 mmap index first
        if bin_path.exists() {
            if let Ok(mmap) = crate::context::search::mmap_index::MmapTurbovecIndex::open(&bin_path)
            {
                if let Ok(bytes) = fs::read(&hashes_path) {
                    if let Ok(hashes) = serde_json::from_slice::<HashMap<String, u64>>(&bytes) {
                        self.file_hashes = hashes;
                    }
                }
                let count = mmap.record_count();
                let mut loaded_chunks = Vec::with_capacity(count);
                for i in 0..count {
                    if let Some(meta) = mmap.get_metadata(i) {
                        let quant2 = mmap.get_record(i).map(|r| r.quant2);
                        loaded_chunks.push(CodeChunk {
                            file_path: meta.file_path,
                            start_line: meta.start_line,
                            end_line: meta.end_line,
                            content: meta.content,
                            vector: Vec::new(),
                            symbol_name: meta.symbol_name,
                            symbol_kind: meta.symbol_kind,
                            binary_vector: None,
                            polar_quant: None,
                            polar_quant_fixed: None,
                            polar_quant2: quant2,
                        });
                    }
                }
                self.chunks = loaded_chunks;
                self.mmap_index = Some(mmap);
                return;
            }
            // Stale v1/v2 binary format detected: remove so build_index cleanly regenerates v3
            let _ = fs::remove_file(
                crate::context::search::mmap_index::MmapTurbovecIndex::tombstone_path(&bin_path),
            );
            let _ = fs::remove_file(&bin_path);
        }

        // 2. Fallback: migrate from legacy JSON cache if it exists
        if let Ok(bytes) = fs::read(cache_path) {
            if let Ok(cache) = serde_json::from_slice::<SemanticCache>(&bytes) {
                self.chunks = cache
                    .chunks
                    .into_iter()
                    .map(|mut c| {
                        let pq2 = c.get_polar_quant2();
                        c.vector.clear();
                        c.vector.shrink_to_fit();
                        c.binary_vector = None;
                        c.polar_quant = None;
                        c.polar_quant_fixed = None;
                        c.polar_quant2 = Some(pq2);
                        c
                    })
                    .collect();
                self.file_hashes = cache.file_hashes;
                if let Ok(hashes_bytes) = serde_json::to_vec(&self.file_hashes) {
                    let _ = fs::write(&hashes_path, hashes_bytes);
                }
                self.save_binary_cache(&bin_path);
                let _ = fs::remove_file(cache_path);
                if let Ok(mmap) =
                    crate::context::search::mmap_index::MmapTurbovecIndex::open(&bin_path)
                {
                    self.mmap_index = Some(mmap);
                }
                return;
            }
        }

        // Ensure stale file_hashes never suppress re-indexing when no chunk store was loaded
        self.file_hashes.clear();
    }

    fn save_cache(&mut self, cache_path: &Path) {
        let bin_path = cache_path.with_extension("bin");
        let hashes_path = cache_path.with_file_name("semantic_hashes.json");

        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        self.mmap_index.take();

        if let Ok(bytes) = serde_json::to_vec(&self.file_hashes) {
            let _ = fs::write(&hashes_path, bytes);
        }

        self.save_binary_cache(&bin_path);

        if let Ok(mmap) = crate::context::search::mmap_index::MmapTurbovecIndex::open(&bin_path) {
            self.mmap_index = Some(mmap);
        }
    }

    pub fn save_binary_cache(&self, bin_path: &Path) {
        use crate::context::search::mmap_index::{ChunkMetadata, MmapTurbovecIndex};
        use crate::context::search::quantize::TurbovecRecord;

        let mut records = Vec::with_capacity(self.chunks.len());
        let mut metadata = Vec::with_capacity(self.chunks.len());

        for chunk in &self.chunks {
            records.push(TurbovecRecord::from_quant2(chunk.get_polar_quant2()));
            metadata.push(ChunkMetadata {
                file_path: chunk.file_path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                symbol_name: chunk.symbol_name.clone(),
                symbol_kind: chunk.symbol_kind.clone(),
                content: chunk.content.clone(),
            });
        }

        let _ = MmapTurbovecIndex::create(bin_path, &records, &metadata);
    }
}

fn build_quantized_chunk(
    file_path: &str,
    start_line: usize,
    end_line: usize,
    content: String,
    embed_text: &str,
    symbol_name: Option<String>,
    symbol_kind: Option<String>,
) -> CodeChunk {
    let vector = SemanticIndex::embed(embed_text);
    let polar_quant2 =
        crate::context::search::quantize::PolarQuant2Fixed::from_f32_with_wht(&vector);
    CodeChunk {
        file_path: file_path.to_string(),
        start_line,
        end_line,
        content,
        vector: Vec::new(),
        symbol_name,
        symbol_kind,
        binary_vector: None,
        polar_quant: None,
        polar_quant_fixed: None,
        polar_quant2: Some(polar_quant2),
    }
}

const MAX_AST_CHUNK_LINES: usize = 60;
const AST_CHILD_WINDOW_LINES: usize = 35;
const AST_CHILD_OVERLAP_LINES: usize = 8;
const MAX_METADATA_BUDGET_RATIO: f32 = 0.25;

/// Formats a compact metadata header (`zvec-grep` `vectorContentForFragment` pattern)
/// prepended before embedding a code chunk, capped at 25% of the chunk's character budget.
fn format_code_vector_metadata(
    kind: &str,
    name: &str,
    scope: Option<&str>,
    signature: &str,
    doc_comment: Option<&str>,
    max_total_chars: usize,
) -> String {
    let budget = ((max_total_chars as f32) * MAX_METADATA_BUDGET_RATIO) as usize;
    let budget = budget.max(96);

    let mut lines = Vec::with_capacity(4);
    lines.push(format!("symbol: {} {}", kind, name));
    if let Some(sc) = scope.filter(|s| !s.is_empty()) {
        lines.push(format!("scope: {}", sc));
    }
    let clean_sig = signature.split_whitespace().collect::<Vec<_>>().join(" ");
    if !clean_sig.is_empty() {
        lines.push(format!("signature: {}", clean_sig));
    }
    if let Some(doc) = doc_comment.filter(|d| !d.trim().is_empty()) {
        let clean_doc = doc.split_whitespace().collect::<Vec<_>>().join(" ");
        lines.push(format!("doc: {}", clean_doc));
    }

    let joined = lines.join("\n");
    if joined.len() <= budget {
        joined
    } else {
        let mut truncated = String::with_capacity(budget);
        for ch in joined.chars() {
            if truncated.len() + ch.len_utf8() + 3 > budget {
                truncated.push_str("...");
                break;
            }
            truncated.push(ch);
        }
        truncated
    }
}

/// Builds a compact structural outline (`zvec-grep` `codeEntityOutline` pattern)
/// for a container (`impl`, `class`, `trait`, `interface`) or large function.
fn build_symbol_outline_text(
    sym: &crate::context::repomap::SymbolDef,
    all_symbols: &[crate::context::repomap::SymbolDef],
    body_lines: &[&str],
) -> String {
    let mut out = String::new();
    if let Some(ref doc) = sym.doc_comment {
        out.push_str(&format!("/// {}\n", doc));
    }
    out.push_str(&sym.signature);

    // Collect nested member declarations if this is a container (`impl`, `class`, `trait`, `interface`)
    let members: Vec<&crate::context::repomap::SymbolDef> = all_symbols
        .iter()
        .filter(|other| {
            other.line_number > sym.line_number
                && other.end_line <= sym.end_line
                && other.kind != "import"
        })
        .take(32)
        .collect();

    if !members.is_empty() {
        out.push_str("\n\nmembers:");
        for m in members {
            out.push_str(&format!("\n- {} {}: {}", m.kind, m.name, m.signature));
        }
    } else {
        // Extract lightweight outgoing function call identifiers from body lines
        let calls = extract_lightweight_calls(body_lines, &sym.name, 24);
        if !calls.is_empty() {
            out.push_str(&format!("\n\ncalls: {}", calls.join(", ")));
        }
    }
    out
}

/// Extracts up to `max_calls` distinct called function/method identifiers from a code slice,
/// stripping string literals and comments first so text inside strings/comments is never misparsed.
pub(crate) fn extract_lightweight_calls(
    lines: &[&str],
    self_name: &str,
    max_calls: usize,
) -> Vec<String> {
    const CALL_STOPWORDS: &[&str] = &[
        "if",
        "for",
        "while",
        "match",
        "switch",
        "return",
        "sizeof",
        "typeof",
        "Some",
        "None",
        "Ok",
        "Err",
        "vec",
        "format",
        "println",
        "eprintln",
        "assert",
        "assert_eq",
        "pub",
        "fn",
        "def",
        "function",
        "catch",
    ];
    let mut calls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in lines {
        let code_only = crate::utils::strings::strip_code_strings_and_comments(line);
        let trimmed = code_only.trim();
        if trimmed.is_empty() {
            continue;
        }
        let bytes = trimmed.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'(' && i > 0 {
                let mut end = i;
                while end > 0 && bytes[end - 1].is_ascii_whitespace() {
                    end -= 1;
                }
                let mut start = end;
                while start > 0 {
                    let b = bytes[start - 1];
                    if b.is_ascii_alphanumeric() || b == b'_' || b == b':' || b == b'.' || b == b'!'
                    {
                        start -= 1;
                    } else {
                        break;
                    }
                }
                if start < end {
                    let raw = &trimmed[start..end];
                    let cand = raw
                        .trim_start_matches('.')
                        .trim_start_matches(':')
                        .trim_end_matches('!');
                    let short_name = cand.rsplit([':', '.']).next().unwrap_or(cand);
                    if short_name.len() >= 2
                        && short_name != self_name
                        && short_name
                            .bytes()
                            .next()
                            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                        && !CALL_STOPWORDS.contains(&short_name)
                    {
                        let display = if cand.contains("::") {
                            cand.to_string()
                        } else {
                            short_name.to_string()
                        };
                        if seen.insert(display.clone()) {
                            calls.push(display);
                            if calls.len() >= max_calls {
                                return calls;
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }
    calls
}

/// Splits source code into AST symbol chunks (with `zvec-grep` metadata headers, parent-child
/// outline chunks for large symbols, and fence-aware Markdown section extraction), falling back
/// to sliding window chunking.
pub fn chunk_source_code_ast(
    file_path: &str,
    content: &str,
    extractor: &mut RepoMapExtractor,
    abs_path: &Path,
) -> Vec<CodeChunk> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if matches!(ext, "md" | "mdx") {
        let md_chunks = chunk_markdown_sections(file_path, content);
        if !md_chunks.is_empty() {
            return md_chunks;
        }
    }

    // Try AST symbol extraction from in-memory content (avoiding redundant disk read)
    if let Ok(symbols) = extractor.extract_symbols_from_str(ext, content) {
        if !symbols.is_empty() {
            let mut chunks = Vec::with_capacity(symbols.len());
            for sym in &symbols {
                if sym.kind == "import" {
                    continue;
                }
                let start = sym.line_number.saturating_sub(1).min(lines.len());
                let end = sym.end_line.min(lines.len()).max(start + 1);
                let sym_lines = &lines[start..end];
                let chunk_text = sym_lines.join("\n");

                // Find enclosing scope (e.g. `impl Foo` or `class Bar`)
                let enclosing_scope = symbols
                    .iter()
                    .find(|parent| {
                        parent.line_number < sym.line_number
                            && parent.end_line >= sym.end_line
                            && matches!(
                                parent.kind.as_str(),
                                "impl" | "class" | "trait" | "interface" | "struct"
                            )
                    })
                    .map(|p| p.name.as_str());

                let qualified_name = match enclosing_scope {
                    Some(scope_name) if !sym.name.contains("::") => {
                        format!("{}::{}", scope_name, sym.name)
                    }
                    _ => sym.name.clone(),
                };

                let meta_header = format_code_vector_metadata(
                    &sym.kind,
                    &qualified_name,
                    enclosing_scope,
                    &sym.signature,
                    sym.doc_comment.as_deref(),
                    chunk_text.len().max(256),
                );

                let span_lines = end.saturating_sub(start);
                let is_container =
                    matches!(sym.kind.as_str(), "impl" | "class" | "trait" | "interface");

                if span_lines <= MAX_AST_CHUNK_LINES && !is_container {
                    let boost_text = format!("{}\n{}", meta_header, chunk_text);
                    chunks.push(build_quantized_chunk(
                        file_path,
                        start + 1,
                        end,
                        chunk_text,
                        &boost_text,
                        Some(qualified_name),
                        Some(sym.kind.clone()),
                    ));
                } else {
                    // 1. Emit Major Outline Chunk (`zvec-grep` parent outline entity)
                    let outline_text = build_symbol_outline_text(sym, &symbols, sym_lines);
                    let outline_embed = format!("{}\n{}", meta_header, outline_text);
                    chunks.push(build_quantized_chunk(
                        file_path,
                        start + 1,
                        end,
                        outline_text,
                        &outline_embed,
                        Some(qualified_name.clone()),
                        Some(sym.kind.clone()),
                    ));

                    // 2. For oversized non-container functions/methods, emit overlapping child windows
                    if !is_container && span_lines > MAX_AST_CHUNK_LINES {
                        let mut win_start = start;
                        while win_start < end {
                            let win_end = (win_start + AST_CHILD_WINDOW_LINES).min(end);
                            let win_text = lines[win_start..win_end].join("\n");
                            let win_embed = format!("{}\n{}", meta_header, win_text);
                            chunks.push(build_quantized_chunk(
                                file_path,
                                win_start + 1,
                                win_end,
                                win_text,
                                &win_embed,
                                Some(qualified_name.clone()),
                                Some(sym.kind.clone()),
                            ));
                            if win_end == end {
                                break;
                            }
                            win_start += AST_CHILD_WINDOW_LINES - AST_CHILD_OVERLAP_LINES;
                        }
                    }
                }
            }
            if !chunks.is_empty() {
                return chunks;
            }
        }
    }

    // Fallback to sliding window chunking for non-AST or unstructured files
    chunk_source_code_sliding(file_path, content)
}

/// A parsed Markdown heading section with its 1-indexed line boundaries and heading breadcrumb stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownSectionSpan {
    pub heading: String,
    pub level: usize,
    pub breadcrumb: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// Extracts fence-aware Markdown heading sections (`#`..`######` and Setext `===`/`---`)
/// with hierarchical `H1 > H2 > H3` breadcrumbs (`zvec-grep` `MarkdownExtractor` pattern).
pub fn extract_markdown_sections(content: &str) -> Vec<MarkdownSectionSpan> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    struct RawHeading {
        level: usize,
        text: String,
        line_idx: usize,
    }

    let mut headings: Vec<RawHeading> = Vec::new();
    let mut active_fence: Option<&str> = None;
    let mut idx = 0;

    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim_start();

        if let Some(fence_marker) = active_fence {
            if trimmed.starts_with(fence_marker) {
                active_fence = None;
            }
            idx += 1;
            continue;
        }

        if trimmed.starts_with("```") {
            active_fence = Some("```");
            idx += 1;
            continue;
        }
        if trimmed.starts_with("~~~") {
            active_fence = Some("~~~");
            idx += 1;
            continue;
        }

        // Check ATX heading: `#{1,6} <title>`
        let hash_count = trimmed.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hash_count) && trimmed.as_bytes().get(hash_count) == Some(&b' ') {
            let title = trimmed[hash_count..].trim().trim_end_matches('#').trim();
            if !title.is_empty() {
                headings.push(RawHeading {
                    level: hash_count,
                    text: title.to_string(),
                    line_idx: idx,
                });
                idx += 1;
                continue;
            }
        }

        // Check Setext heading (`===` for H1, `---` for H2)
        if !trimmed.is_empty() && idx + 1 < lines.len() {
            let next_trimmed = lines[idx + 1].trim();
            if next_trimmed.len() >= 3
                && (next_trimmed.bytes().all(|b| b == b'=')
                    || next_trimmed.bytes().all(|b| b == b'-'))
            {
                let level = if next_trimmed.starts_with('=') { 1 } else { 2 };
                headings.push(RawHeading {
                    level,
                    text: trimmed.to_string(),
                    line_idx: idx,
                });
                idx += 2;
                continue;
            }
        }

        idx += 1;
    }

    if headings.is_empty() {
        return Vec::new();
    }

    let mut sections = Vec::with_capacity(headings.len() + 1);
    if headings[0].line_idx > 0
        && lines[..headings[0].line_idx]
            .iter()
            .any(|l| !l.trim().is_empty())
    {
        sections.push(MarkdownSectionSpan {
            heading: "Preamble".to_string(),
            level: 1,
            breadcrumb: "Preamble".to_string(),
            start_line: 1,
            end_line: headings[0].line_idx,
        });
    }

    let mut stack: Vec<(usize, String)> = Vec::new();
    for (i, h) in headings.iter().enumerate() {
        while stack.last().is_some_and(|(lvl, _)| *lvl >= h.level) {
            stack.pop();
        }
        let mut crumb_parts: Vec<&str> = stack.iter().map(|(_, t)| t.as_str()).collect();
        crumb_parts.push(&h.text);
        let breadcrumb = crumb_parts.join(" > ");

        let end_line = if i + 1 < headings.len() {
            headings[i + 1].line_idx
        } else {
            lines.len()
        };

        sections.push(MarkdownSectionSpan {
            heading: h.text.clone(),
            level: h.level,
            breadcrumb,
            start_line: h.line_idx + 1,
            end_line: end_line.max(h.line_idx + 1),
        });

        stack.push((h.level, h.text.clone()));
    }

    sections
}

/// Chunks Markdown documents by heading sections with `heading` + `scope` breadcrumb metadata.
pub fn chunk_markdown_sections(file_path: &str, content: &str) -> Vec<CodeChunk> {
    let sections = extract_markdown_sections(content);
    if sections.is_empty() {
        return chunk_source_code_sliding(file_path, content);
    }

    let lines: Vec<&str> = content.lines().collect();
    let mut chunks = Vec::with_capacity(sections.len());

    for sec in sections {
        let start = sec.start_line.saturating_sub(1).min(lines.len());
        let end = sec.end_line.min(lines.len()).max(start + 1);
        let sec_lines = &lines[start..end];
        let sec_text = sec_lines.join("\n");
        if sec_text.trim().is_empty() {
            continue;
        }

        let meta_header = format!(
            "heading: {}\nheading_level: {}\nscope: {}",
            sec.heading, sec.level, sec.breadcrumb
        );
        let kind = format!("h{}", sec.level);
        let span = end.saturating_sub(start);

        if span <= MAX_AST_CHUNK_LINES {
            let embed_text = format!("{}\n{}", meta_header, sec_text);
            chunks.push(build_quantized_chunk(
                file_path,
                start + 1,
                end,
                sec_text,
                &embed_text,
                Some(sec.breadcrumb),
                Some(kind),
            ));
        } else {
            let mut win_start = start;
            while win_start < end {
                let win_end = (win_start + AST_CHILD_WINDOW_LINES).min(end);
                let win_text = lines[win_start..win_end].join("\n");
                let embed_text = format!("{}\n{}", meta_header, win_text);
                chunks.push(build_quantized_chunk(
                    file_path,
                    win_start + 1,
                    win_end,
                    win_text,
                    &embed_text,
                    Some(sec.breadcrumb.clone()),
                    Some(kind.clone()),
                ));
                if win_end == end {
                    break;
                }
                win_start += AST_CHILD_WINDOW_LINES - AST_CHILD_OVERLAP_LINES;
            }
        }
    }

    chunks
}

/// Splits source code into sliding chunks of ~25 lines with line-number tracking.
pub fn chunk_source_code_sliding(file_path: &str, content: &str) -> Vec<CodeChunk> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let chunk_size = 25;
    let overlap = 8;
    let mut chunks = Vec::new();

    let mut start = 0;
    while start < lines.len() {
        let end = (start + chunk_size).min(lines.len());
        let chunk_text = lines[start..end].join("\n");
        let chunk = build_quantized_chunk(
            file_path,
            start + 1,
            end,
            chunk_text.clone(),
            &chunk_text,
            None,
            None,
        );
        chunks.push(chunk);

        if end == lines.len() {
            break;
        }
        start += chunk_size - overlap;
    }

    chunks
}

fn is_indexable_extension(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "py"
            | "js"
            | "ts"
            | "jsx"
            | "tsx"
            | "go"
            | "c"
            | "cpp"
            | "h"
            | "md"
            | "toml"
            | "json"
    )
}

fn hash_token(s: &str) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for b in s.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_semantic_embedding_and_similarity() {
        let v1 = SemanticIndex::embed(
            "function authenticateUser(username, password) { verifyToken(); }",
        );
        let v2 = SemanticIndex::embed(
            "fn login_account(user: &str, pass: &str) -> bool { check_auth() }",
        );
        let v3 = SemanticIndex::embed(
            "const renderHtmlCanvas = (ctx, width, height) => ctx.fillRect(0, 0, width, height);",
        );

        let sim_auth = SemanticIndex::cosine_similarity(&v1, &v2);
        let sim_other = SemanticIndex::cosine_similarity(&v1, &v3);

        assert!(
            sim_auth > sim_other,
            "Auth snippets should be more similar than canvas snippet ({} vs {})",
            sim_auth,
            sim_other
        );
    }

    #[test]
    fn test_semantic_index_build_and_search() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        let src_dir = ws.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        let auth_file = src_dir.join("auth.rs");
        fs::write(
            &auth_file,
            "pub fn login(user: &str) -> bool {\n    println!(\"authenticating user\");\n    true\n}",
        )
        .unwrap();

        let canvas_file = src_dir.join("canvas.rs");
        fs::write(
            &canvas_file,
            "pub fn draw_circle(radius: f64) {\n    println!(\"drawing visual shape\");\n}",
        )
        .unwrap();

        let mut index = SemanticIndex::new();
        let count = index.build_index(ws).unwrap();
        assert_eq!(count, 2);

        let results = index.search("how to log in or authenticate user", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].file_path, "src/auth.rs");

        let sym_results = index.search_symbols("login", 5);
        assert!(!sym_results.is_empty());
        assert_eq!(sym_results[0].symbol_name.as_deref(), Some("login"));
    }

    #[test]
    fn test_legacy_json_migration_and_binary_index() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let cache_dir = ws.join(".minicode").join("cache");
        fs::create_dir_all(&cache_dir).unwrap();

        // 1. Create a legacy JSON cache
        let legacy_json_path = cache_dir.join("semantic_index.json");
        let bin_path = cache_dir.join("semantic_index.bin");
        let hashes_path = cache_dir.join("semantic_hashes.json");

        let chunk = CodeChunk {
            file_path: "src/lib.rs".into(),
            start_line: 1,
            end_line: 10,
            content: "pub fn authenticate_system() -> bool { true }".into(),
            vector: SemanticIndex::embed("pub fn authenticate_system() -> bool { true }"),
            symbol_name: Some("authenticate_system".into()),
            symbol_kind: Some("function".into()),
            binary_vector: None,
            polar_quant: None,
            polar_quant_fixed: None,
            polar_quant2: None,
        };

        let mut file_hashes = HashMap::new();
        file_hashes.insert("src/lib.rs".into(), 12345678);

        let legacy_cache = SemanticCache {
            file_hashes: file_hashes.clone(),
            chunks: vec![chunk],
        };

        let json_bytes = serde_json::to_vec(&legacy_cache).unwrap();
        fs::write(&legacy_json_path, json_bytes).unwrap();
        assert!(legacy_json_path.exists());
        assert!(!bin_path.exists());

        // 2. Load index - should trigger migration
        let mut index = SemanticIndex::new();
        index.load_cache(&legacy_json_path);

        // Verify JSON was deleted, and binary + hashes files were created
        assert!(
            !legacy_json_path.exists(),
            "Legacy JSON should be removed after migration"
        );
        assert!(bin_path.exists(), "Binary index file must be created");
        assert!(hashes_path.exists(), "Hashes sidecar file must be created");
        assert_eq!(index.chunks.len(), 1);
        assert_eq!(
            index.chunks[0].symbol_name.as_deref(),
            Some("authenticate_system")
        );
        assert!(
            index.mmap_index.is_some(),
            "Mmap index should be initialized"
        );

        // 3. Verify fast search works via mmap
        let res = index.search("authenticate", 1);
        assert!(!res.is_empty());
        assert_eq!(res[0].file_path, "src/lib.rs");
    }

    #[test]
    fn test_fence_aware_markdown_sections_and_breadcrumbs() {
        let md = "\
# System Architecture
Introductory overview.

```bash
# This bash comment inside a code fence must NOT be treated as a heading!
cargo test
```

## Retrieval Pipeline
### Adaptive Recall
Details on recall expansion.
";
        let sections = extract_markdown_sections(md);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].breadcrumb, "System Architecture");
        assert_eq!(sections[0].level, 1);
        assert_eq!(
            sections[1].breadcrumb,
            "System Architecture > Retrieval Pipeline"
        );
        assert_eq!(sections[1].level, 2);
        assert_eq!(
            sections[2].breadcrumb,
            "System Architecture > Retrieval Pipeline > Adaptive Recall"
        );
        assert_eq!(sections[2].level, 3);
    }

    #[test]
    fn test_ast_chunking_enclosing_scope_and_outline_members() {
        let mut extractor = RepoMapExtractor::new();
        let rust_code = "\
pub struct AuthEngine;

impl AuthEngine {
    /// Verifies a JWT bearer token
    pub fn verify_token(&self, token: &str) -> bool {
        check_signature(token)
    }
}
";
        let chunks = chunk_source_code_ast(
            "src/auth.rs",
            rust_code,
            &mut extractor,
            Path::new("src/auth.rs"),
        );
        assert!(chunks
            .iter()
            .any(|c| c.symbol_name.as_deref() == Some("AuthEngine::verify_token")));
        let impl_outline = chunks
            .iter()
            .find(|c| c.symbol_kind.as_deref() == Some("impl"))
            .expect("impl outline chunk should exist");
        assert!(impl_outline.content.contains("members:"));
        assert!(impl_outline.content.contains("verify_token"));
    }
}
