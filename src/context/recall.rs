//! Persistent recall index for web content fetched by the agent.
//!
//! Every page pulled through `fetch_or_browse` is chunked, embedded with the
//! local deterministic hashing embedder (zero downloads, zero API calls) and
//! appended to `.minicode/recall/index.jsonl`. The `recall` tool then answers
//! "what have I already read about X?" with semantic + keyword ranking.

#![allow(dead_code)]

use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Maximum characters per stored chunk (roughly ~250 tokens).
const CHUNK_CHARS: usize = 1200;
/// Characters of overlap between consecutive chunks.
const CHUNK_OVERLAP: usize = 150;
/// Safety cap on total stored characters per document (1 MB).
const MAX_DOC_CHARS: usize = 1_000_000;

/// A single indexed document with its embedded chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallDoc {
    pub url: String,
    pub title: String,
    pub fetched_at: String,
    pub content_chars: usize,
    pub chunks: Vec<RecallChunk>,
}

/// A chunk of document text with its embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallChunk {
    pub text: String,
    pub vector: Vec<f32>,
}

/// A ranked recall hit returned to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallHit {
    pub url: String,
    pub title: String,
    pub score: f32,
    pub snippet: String,
    pub fetched_at: String,
}

pub struct RecallStore {
    index_path: PathBuf,
}

impl RecallStore {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            index_path: workspace_root
                .join(".minicode")
                .join("recall")
                .join("index.jsonl"),
        }
    }

    /// Loads every stored document. JSONL lines that fail to parse are skipped
    /// so a truncated write never poisons the whole index.
    pub fn load_all(&self) -> Vec<RecallDoc> {
        let Ok(content) = fs::read_to_string(&self.index_path) else {
            return Vec::new();
        };
        content
            .lines()
            .filter_map(|line| serde_json::from_str::<RecallDoc>(line).ok())
            .collect()
    }

    /// Chunks, embeds and upserts a document (replacing any previous version
    /// of the same URL). Returns the number of chunks stored.
    pub fn index_content(
        &self,
        url: &str,
        title: &str,
        content: &str,
        fetched_at: &str,
    ) -> Result<usize> {
        if content.trim().is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "index_content".to_string(),
                reason: "content is empty; nothing to index".to_string(),
            }
            .into());
        }

        let limit = content.floor_char_boundary(content.len().min(MAX_DOC_CHARS));
        let content = &content[..limit];
        let chunks: Vec<RecallChunk> = chunk_text(content)
            .into_iter()
            .map(|text| RecallChunk {
                vector: crate::context::semantic::SemanticIndex::embed(&text),
                text,
            })
            .collect();

        let doc = RecallDoc {
            url: url.to_string(),
            title: title.to_string(),
            fetched_at: fetched_at.to_string(),
            content_chars: content.len(),
            chunks,
        };

        self.upsert(doc)?;
        Ok(self
            .load_all()
            .iter()
            .find(|d| d.url == url)
            .map(|d| d.chunks.len())
            .unwrap_or(0))
    }

    /// Replaces the stored version of `doc.url`, keeping all other documents.
    fn upsert(&self, doc: RecallDoc) -> Result<()> {
        if let Some(parent) = self.index_path.parent() {
            fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        let mut docs: Vec<RecallDoc> = self
            .load_all()
            .into_iter()
            .filter(|d| d.url != doc.url)
            .collect();
        docs.push(doc);

        // Atomic-ish rewrite: temp file + rename so a crash cannot truncate.
        let tmp = self.index_path.with_extension("jsonl.tmp");
        {
            let mut f = fs::File::create(&tmp).map_err(|e| ToolError::FileOp {
                path: tmp.display().to_string(),
                source: e,
            })?;
            for d in &docs {
                writeln!(f, "{}", serde_json::to_string(d).unwrap_or_default()).map_err(|e| {
                    ToolError::FileOp {
                        path: tmp.display().to_string(),
                        source: std::io::Error::other(e),
                    }
                })?;
            }
            f.flush().ok();
        }
        fs::rename(&tmp, &self.index_path).map_err(|e| ToolError::FileOp {
            path: self.index_path.display().to_string(),
            source: e,
        })?;
        Ok(())
    }

    /// Ranks all stored chunks against `query` by embedding cosine similarity,
    /// with a small keyword-overlap boost. Falls back to pure keyword scoring
    /// when the query embeds to a zero vector.
    pub fn recall(&self, query: &str, max_results: usize) -> Vec<RecallHit> {
        let qvec = crate::context::semantic::SemanticIndex::embed(query);
        let qlower = query.to_lowercase();
        let qterms: Vec<&str> = qlower.split_whitespace().collect();
        let use_vector = qvec.iter().any(|v| *v != 0.0);

        let mut hits: Vec<RecallHit> = Vec::new();
        for doc in self.load_all() {
            for chunk in &doc.chunks {
                let mut score = if use_vector {
                    crate::context::semantic::SemanticIndex::cosine_similarity(&qvec, &chunk.vector)
                } else {
                    0.0
                };

                // Keyword overlap boosts and rescues zero-vector queries.
                let lower_chunk = chunk.text.to_lowercase();
                let matches = qterms.iter().filter(|t| lower_chunk.contains(**t)).count();
                if !qterms.is_empty() {
                    let kw = matches as f32 / qterms.len() as f32;
                    if !use_vector && matches > 0 {
                        score = kw;
                    } else {
                        score += 0.15 * kw;
                    }
                }

                if score <= 0.01 {
                    continue;
                }

                hits.push(RecallHit {
                    url: doc.url.clone(),
                    title: doc.title.clone(),
                    score: (score * 1000.0).round() / 1000.0,
                    snippet: chunk.text.chars().take(400).collect(),
                    fetched_at: doc.fetched_at.clone(),
                });
            }
        }

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.dedup_by(|a, b| a.url == b.url && a.snippet == b.snippet);
        hits.truncate(max_results);
        hits
    }
}

/// Split document into overlapping chunks. Tries to split on paragraphs or sentence boundaries.
fn chunk_text(content: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    if content.len() <= CHUNK_CHARS {
        if !content.trim().is_empty() {
            chunks.push(content.trim().to_string());
        }
        return chunks;
    }

    let mut start = 0usize;
    while start < content.len() {
        start = content.floor_char_boundary(start);
        let raw_end = (start + CHUNK_CHARS).min(content.len());
        let end = content.floor_char_boundary(raw_end);
        if end <= start {
            break;
        }
        let slice = &content[start..end];
        let cut = slice
            .rfind("\n\n")
            .map(|i| i + 2)
            .or_else(|| slice.rfind(". "))
            .unwrap_or(slice.len());
        let cut = if start + cut >= content.len() {
            slice.len()
        } else {
            cut
        };
        let cut_end = content.floor_char_boundary(start + cut);
        let piece = content[start..cut_end].trim();
        if !piece.is_empty() {
            chunks.push(piece.to_string());
        }
        if cut_end >= content.len() {
            break;
        }
        let advance = cut_end.saturating_sub(start);
        let overlap = CHUNK_OVERLAP.min(advance);
        let next_start = cut_end.saturating_sub(overlap);
        if next_start <= start {
            start = cut_end;
        } else {
            start = next_start;
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &Path) -> RecallStore {
        RecallStore::new(dir)
    }

    #[test]
    fn test_index_and_semantic_recall_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(dir.path());

        s.index_content(
            "https://docs.rs/tokio",
            "Tokio tutorial",
            "Tokio is an asynchronous runtime for Rust. It provides async timers, IO and task scheduling. Spawn tasks with tokio::spawn.",
            "2026-08-25",
        )
        .unwrap();
        s.index_content(
            "https://example.com/recipes",
            "Pancake recipes",
            "Mix flour, milk and eggs. Cook the pancake batter on a hot pan until golden brown.",
            "2026-08-25",
        )
        .unwrap();

        let hits = s.recall("async runtime spawn tasks rust tokio", 3);
        assert!(!hits.is_empty(), "expected recall hits");
        assert_eq!(hits[0].url, "https://docs.rs/tokio");
        assert!(hits[0].snippet.contains("Tokio"));

        let hits = s.recall("pancake batter flour milk", 3);
        assert_eq!(hits[0].url, "https://example.com/recipes");
    }

    #[test]
    fn test_upsert_replaces_same_url() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(dir.path());
        s.index_content("u1", "v1", "first version of the content", "t")
            .unwrap();
        s.index_content("u1", "v2", "second rewritten version of the content", "t")
            .unwrap();
        s.index_content("u2", "other", "unrelated document body", "t")
            .unwrap();

        let docs = s.load_all();
        assert_eq!(docs.len(), 2, "same-url upsert must replace, not duplicate");
        assert_eq!(s.recall("rewritten second version", 5)[0].title, "v2");
    }

    #[test]
    fn test_long_content_is_chunked() {
        let long = "paragraph about rust async ecosystems. ".repeat(80);
        let chunks = chunk_text(&long);
        assert!(chunks.len() >= 3, "long content must split into chunks");
        assert!(chunks.iter().all(|c| c.len() <= CHUNK_CHARS + 200));
    }

    #[test]
    fn test_empty_content_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(store(dir.path())
            .index_content("u", "t", "   ", "t")
            .is_err());
    }
}
