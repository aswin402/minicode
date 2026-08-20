use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const VECTOR_DIM: usize = 128;

/// A localized source code chunk with line numbers and dense embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeChunk {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub vector: Vec<f32>,
}

/// A search result from semantic code search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticSearchResult {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub similarity_score: f32,
    pub snippet: String,
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
            if token.len() >= 3 {
                for i in 0..=token.len() - 3 {
                    let gram = &token[i..i + 3];
                    let gh = hash_token(gram);
                    let gidx = (gh as usize) % VECTOR_DIM;
                    let gsign = if (gh >> 8) & 1 == 0 { 1.0 } else { -1.0 };
                    vec[gidx] += gsign * 0.8;
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

            // Read and chunk file
            if let Ok(content) = fs::read_to_string(&path) {
                let chunks = chunk_source_code(&rel_path, &content);
                self.chunks.extend(chunks);
                self.file_hashes.insert(rel_path, mtime);
                indexed_count += 1;
            }
        }

        self.save_cache(&cache_path);
        Ok(indexed_count)
    }

    /// Searches the indexed codebase for chunks semantically matching the query.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SemanticSearchResult> {
        let query_vec = Self::embed(query);
        let mut scored: Vec<(f32, &CodeChunk)> = self
            .chunks
            .iter()
            .map(|chunk| {
                let score = Self::cosine_similarity(&query_vec, &chunk.vector);
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
            })
            .collect()
    }

    fn cache_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".minicode")
            .join("cache")
            .join("semantic_index.json")
    }

    fn load_cache(&mut self, cache_path: &Path) {
        if let Ok(bytes) = fs::read(cache_path) {
            if let Ok(cache) = serde_json::from_slice::<SemanticCache>(&bytes) {
                self.chunks = cache.chunks;
                self.file_hashes = cache.file_hashes;
            }
        }
    }

    fn save_cache(&self, cache_path: &Path) {
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let cache = SemanticCache {
            file_hashes: self.file_hashes.clone(),
            chunks: self.chunks.clone(),
        };
        if let Ok(bytes) = serde_json::to_vec(&cache) {
            let _ = fs::write(cache_path, bytes);
        }
    }
}

/// Splits source code into sliding chunks of ~20–30 lines with line-number tracking.
fn chunk_source_code(file_path: &str, content: &str) -> Vec<CodeChunk> {
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

        chunks.push(CodeChunk {
            file_path: file_path.to_string(),
            start_line: start + 1,
            end_line: end,
            content: chunk_text,
            vector,
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
    }
}
