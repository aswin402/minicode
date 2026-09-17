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
}

impl CodeChunk {
    pub fn get_binary_vector(&self) -> crate::context::search::quantize::BinaryVector128 {
        self.binary_vector.unwrap_or_else(|| {
            crate::context::search::quantize::BinaryVector128::from_f32_slice(&self.vector)
        })
    }

    #[allow(dead_code)]
    pub fn get_polar_quant(&self) -> crate::context::search::quantize::PolarQuant4 {
        self.polar_quant.clone().unwrap_or_else(|| {
            crate::context::search::quantize::PolarQuant4::from_f32_slice(&self.vector)
        })
    }

    pub fn get_polar_quant_fixed(&self) -> crate::context::search::quantize::PolarQuant4Fixed {
        self.polar_quant_fixed.unwrap_or_else(|| {
            if let Some(ref pq) = self.polar_quant {
                pq.clone().into()
            } else {
                crate::context::search::quantize::PolarQuant4Fixed::from_f32_slice(&self.vector)
            }
        })
    }

    /// Computes similarity score against an f32 query vector using 4-bit asymmetric dot product
    /// (or full vector cosine similarity if available).
    pub fn similarity_to_query(&self, query_vec: &[f32]) -> f32 {
        if let Some(ref pqf) = self.polar_quant_fixed {
            pqf.asymmetric_dot_product(query_vec)
        } else if let Some(ref pq) = self.polar_quant {
            pq.asymmetric_dot_product(query_vec)
        } else if !self.vector.is_empty() {
            SemanticIndex::cosine_similarity(query_vec, &self.vector)
        } else if let Some(ref bv) = self.binary_vector {
            let q_bin =
                crate::context::search::quantize::BinaryVector128::from_f32_slice(query_vec);
            bv.cosine_similarity(&q_bin)
        } else {
            0.0
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

        for rel_path in rel_files {
            let path = workspace_root.join(&rel_path);
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !is_indexable_extension(ext) {
                continue;
            }

            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if let Some(&cached_mtime) = self.file_hashes.get(&rel_path) {
                if cached_mtime == mtime {
                    continue; // Unchanged file
                }
            }

            // Remove old chunks for this file
            self.chunks.retain(|c| c.file_path != rel_path);

            // Read and chunk file using AST-aware symbol chunker
            if let Ok(content) = fs::read_to_string(&path) {
                let chunks = chunk_source_code_ast(&rel_path, &content, &mut repomap, &path);
                self.chunks.extend(chunks);
                self.file_hashes.insert(rel_path, mtime);
                indexed_count += 1;
            }
        }

        self.save_cache(&cache_path);
        Ok(indexed_count)
    }

    /// Fast 2-stage search: stage 1 screens candidate chunks using SIMD/hardware popcount on 16-byte
    /// binary vectors; stage 2 calculates exact cosine similarity on the top candidates.
    pub fn search_fast(&self, query: &str, limit: usize) -> Vec<SemanticSearchResult> {
        if self.chunks.is_empty() {
            return Vec::new();
        }

        let query_vec = Self::embed(query);

        // Fast path: use zero-copy memory map if available
        if let Some(ref mmap) = self.mmap_index {
            let top_k = mmap.search(&query_vec, limit);
            let mut results = Vec::with_capacity(top_k.len());
            for (idx, score) in top_k {
                if score > 0.05 {
                    if let Some(chunk) = self.chunks.get(idx) {
                        results.push(SemanticSearchResult {
                            file_path: chunk.file_path.clone(),
                            start_line: chunk.start_line,
                            end_line: chunk.end_line,
                            similarity_score: score,
                            snippet: chunk.content.clone(),
                            symbol_name: chunk.symbol_name.clone(),
                            symbol_kind: chunk.symbol_kind.clone(),
                        });
                    }
                }
            }
            return results;
        }

        let query_bin =
            crate::context::search::quantize::BinaryVector128::from_f32_slice(&query_vec);

        // Stage 1: Fast popcount hamming distance screening
        let candidate_pool_size = (limit * 4).max(32).min(self.chunks.len());
        let mut candidates: Vec<(u32, &CodeChunk)> = self
            .chunks
            .iter()
            .map(|c| (c.get_binary_vector().hamming_distance(&query_bin), c))
            .collect();

        candidates.sort_by_key(|c| c.0);

        // Stage 2: 4-bit Asymmetric dot product scoring on top candidates
        let mut scored: Vec<(f32, &CodeChunk)> = candidates
            .into_iter()
            .take(candidate_pool_size)
            .map(|(_, chunk)| {
                let score = chunk.similarity_to_query(&query_vec);
                (score, chunk)
            })
            .filter(|(score, _)| *score > 0.05)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(limit)
            .map(|(score, chunk)| SemanticSearchResult {
                file_path: chunk.file_path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                similarity_score: score,
                snippet: chunk.content.clone(),
                symbol_name: chunk.symbol_name.clone(),
                symbol_kind: chunk.symbol_kind.clone(),
            })
            .collect()
    }

    /// Searches the indexed codebase for chunks semantically matching the query.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SemanticSearchResult> {
        if self.chunks.len() > 64 {
            return self.search_fast(query, limit);
        }

        let query_vec = Self::embed(query);
        let mut scored: Vec<(f32, &CodeChunk)> = self
            .chunks
            .iter()
            .map(|chunk| {
                let score = chunk.similarity_to_query(&query_vec);
                (score, chunk)
            })
            .filter(|(score, _)| *score > 0.05)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(limit)
            .map(|(score, chunk)| SemanticSearchResult {
                file_path: chunk.file_path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                similarity_score: score,
                snippet: chunk.content.clone(),
                symbol_name: chunk.symbol_name.clone(),
                symbol_kind: chunk.symbol_kind.clone(),
            })
            .collect()
    }

    /// Specifically searches for AST symbol definitions (functions, classes, structs) matching the query.
    pub fn search_symbols(&self, query: &str, limit: usize) -> Vec<SemanticSearchResult> {
        if self.chunks.is_empty() {
            return Vec::new();
        }

        let query_vec = Self::embed(query);
        let query_bin =
            crate::context::search::quantize::BinaryVector128::from_f32_slice(&query_vec);

        let symbol_chunks: Vec<&CodeChunk> = self
            .chunks
            .iter()
            .filter(|chunk| chunk.symbol_name.is_some())
            .collect();

        if symbol_chunks.is_empty() {
            return Vec::new();
        }

        // Fast 2-stage screening for symbol search: stage 1 popcount + name boost, stage 2 asymmetric dot product
        let candidate_pool_size = (limit * 4).max(32).min(symbol_chunks.len());
        let mut candidates: Vec<(u32, &CodeChunk)> = symbol_chunks
            .into_iter()
            .map(|c| {
                let dist = c.get_binary_vector().hamming_distance(&query_bin);
                let name_discount = if let Some(ref sym) = c.symbol_name {
                    if sym.eq_ignore_ascii_case(query) {
                        24
                    } else if sym.to_lowercase().contains(&query.to_lowercase()) {
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
                let base_score = chunk.similarity_to_query(&query_vec);
                let name_boost = if let Some(ref sym) = chunk.symbol_name {
                    if sym.eq_ignore_ascii_case(query) {
                        0.4
                    } else if sym.to_lowercase().contains(&query.to_lowercase()) {
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

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(limit)
            .map(|(score, chunk)| SemanticSearchResult {
                file_path: chunk.file_path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                similarity_score: score,
                snippet: chunk.content.clone(),
                symbol_name: chunk.symbol_name.clone(),
                symbol_kind: chunk.symbol_kind.clone(),
            })
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

        // 1. Load file hashes sidecar if present
        if let Ok(bytes) = fs::read(&hashes_path) {
            if let Ok(hashes) = serde_json::from_slice::<HashMap<String, u64>>(&bytes) {
                self.file_hashes = hashes;
            }
        }

        // 2. Try loading binary mmap index
        if bin_path.exists() {
            if let Ok(mmap) = crate::context::search::mmap_index::MmapTurbovecIndex::open(&bin_path)
            {
                let count = mmap.record_count();
                let mut loaded_chunks = Vec::with_capacity(count);
                for i in 0..count {
                    if let Some(meta) = mmap.get_metadata(i) {
                        let rec = mmap.get_record(i);
                        let binary_vec = rec.map(|r| r.binary);
                        let quant4 = rec.map(|r| r.quant4);
                        loaded_chunks.push(CodeChunk {
                            file_path: meta.file_path,
                            start_line: meta.start_line,
                            end_line: meta.end_line,
                            content: meta.content,
                            vector: Vec::new(),
                            symbol_name: meta.symbol_name,
                            symbol_kind: meta.symbol_kind,
                            binary_vector: binary_vec,
                            polar_quant: quant4.map(|q| q.into()),
                            polar_quant_fixed: quant4,
                        });
                    }
                }
                self.chunks = loaded_chunks;
                self.mmap_index = Some(mmap);
                return;
            }
        }

        // 3. Fallback: migrate from legacy JSON cache if it exists
        if let Ok(bytes) = fs::read(cache_path) {
            if let Ok(cache) = serde_json::from_slice::<SemanticCache>(&bytes) {
                self.chunks = cache.chunks;
                self.file_hashes = cache.file_hashes;
                // Save hashes sidecar
                if let Ok(hashes_bytes) = serde_json::to_vec(&self.file_hashes) {
                    let _ = fs::write(&hashes_path, hashes_bytes);
                }
                // Migrate legacy json to binary format immediately
                self.save_binary_cache(&bin_path);
                // Clean up legacy json after successful migration
                let _ = fs::remove_file(cache_path);
                if let Ok(mmap) =
                    crate::context::search::mmap_index::MmapTurbovecIndex::open(&bin_path)
                {
                    self.mmap_index = Some(mmap);
                }
            }
        }
    }

    fn save_cache(&mut self, cache_path: &Path) {
        let bin_path = cache_path.with_extension("bin");
        let hashes_path = cache_path.with_file_name("semantic_hashes.json");

        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Release existing memory mapping before overwriting
        self.mmap_index.take();

        // Save file hashes sidecar
        if let Ok(bytes) = serde_json::to_vec(&self.file_hashes) {
            let _ = fs::write(&hashes_path, bytes);
        }

        // Save binary index
        self.save_binary_cache(&bin_path);

        // Open newly created binary index into mmap
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
            let binary = chunk.get_binary_vector();
            let quant4 = chunk.get_polar_quant_fixed();
            records.push(TurbovecRecord { binary, quant4 });
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

/// Splits source code into AST symbol chunks if supported, falling back to sliding window chunking.
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

    // Try AST symbol extraction
    if let Ok(symbols) = extractor.extract_file_symbols(abs_path) {
        if !symbols.is_empty() {
            let mut chunks = Vec::new();
            for sym in symbols {
                let start = sym.line_number.saturating_sub(1).min(lines.len());
                let end = sym.end_line.min(lines.len()).max(start + 1);
                let chunk_lines = &lines[start..end];
                let chunk_text = chunk_lines.join("\n");

                // Boost vector embedding with symbol name and signature
                let boost_text = format!(
                    "{} {} {}\n{}",
                    sym.kind, sym.name, sym.signature, chunk_text
                );
                let vector = SemanticIndex::embed(&boost_text);

                let binary_vector =
                    crate::context::search::quantize::BinaryVector128::from_f32_slice(&vector);
                let polar_quant_fixed =
                    crate::context::search::quantize::PolarQuant4Fixed::from_f32_slice(&vector);
                let polar_quant: crate::context::search::quantize::PolarQuant4 =
                    polar_quant_fixed.into();
                chunks.push(CodeChunk {
                    file_path: file_path.to_string(),
                    start_line: start + 1,
                    end_line: end,
                    content: chunk_text,
                    vector,
                    symbol_name: Some(sym.name),
                    symbol_kind: Some(sym.kind),
                    binary_vector: Some(binary_vector),
                    polar_quant: Some(polar_quant),
                    polar_quant_fixed: Some(polar_quant_fixed),
                });
            }
            return chunks;
        }
    }

    // Fallback to sliding window chunking for non-AST or unstructured files
    chunk_source_code_sliding(file_path, content)
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
        let chunk_lines = &lines[start..end];
        let chunk_text = chunk_lines.join("\n");
        let vector = SemanticIndex::embed(&chunk_text);
        let binary_vector =
            crate::context::search::quantize::BinaryVector128::from_f32_slice(&vector);
        let polar_quant_fixed =
            crate::context::search::quantize::PolarQuant4Fixed::from_f32_slice(&vector);
        let polar_quant: crate::context::search::quantize::PolarQuant4 = polar_quant_fixed.into();

        chunks.push(CodeChunk {
            file_path: file_path.to_string(),
            start_line: start + 1,
            end_line: end,
            content: chunk_text,
            vector,
            symbol_name: None,
            symbol_kind: None,
            binary_vector: Some(binary_vector),
            polar_quant: Some(polar_quant),
            polar_quant_fixed: Some(polar_quant_fixed),
        });

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
}
