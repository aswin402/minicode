use crate::context::semantic::SemanticIndex;
use crate::error::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// An individual episodic memory record representing a solved problem or architectural learning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodicItem {
    pub id: String,
    pub session_id: String,
    pub timestamp: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub code_references: Vec<String>,
    pub vector: Vec<f32>,
}

/// A scored search result from episodic memory recall.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoredEpisode {
    pub item: EpisodicItem,
    pub score: f32,
}

/// Persistent Episodic Vector Memory for cross-session knowledge recall.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EpisodicMemory {
    pub episodes: Vec<EpisodicItem>,
}

impl EpisodicMemory {
    pub fn new() -> Self {
        Self {
            episodes: Vec::new(),
        }
    }

    /// Records a new session episode, embedding its content into a dense vector.
    pub fn record_episode(
        &mut self,
        title: &str,
        summary: &str,
        tags: Vec<String>,
        code_references: Vec<String>,
        session_id: &str,
    ) -> String {
        let id = format!("ep-{}", uuid::Uuid::new_v4().simple());
        let full_text = format!("{} {} {}", title, summary, tags.join(" "));
        let vector = SemanticIndex::embed(&full_text);

        let item = EpisodicItem {
            id: id.clone(),
            session_id: session_id.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            title: title.to_string(),
            summary: summary.to_string(),
            tags,
            code_references,
            vector,
        };

        self.episodes.push(item);
        id
    }

    /// Performs hybrid search (dense semantic similarity + keyword overlap).
    pub fn search(&self, query: &str, limit: usize) -> Vec<ScoredEpisode> {
        if self.episodes.is_empty() {
            return Vec::new();
        }

        let query_vec = SemanticIndex::embed(query);
        let query_lower = query.to_lowercase();
        let query_tokens: HashSet<&str> = query_lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .collect();

        let mut scored: Vec<ScoredEpisode> = self
            .episodes
            .iter()
            .map(|item| {
                // 1. Semantic cosine similarity (0.0 to 1.0)
                let sem_score = SemanticIndex::cosine_similarity(&query_vec, &item.vector);

                // 2. Keyword overlap score
                let text_lower = format!("{} {} {}", item.title, item.summary, item.tags.join(" "))
                    .to_lowercase();
                let matching_tokens = query_tokens
                    .iter()
                    .filter(|&&t| text_lower.contains(t))
                    .count();
                let kw_score = if !query_tokens.is_empty() {
                    matching_tokens as f32 / query_tokens.len() as f32
                } else {
                    0.0
                };

                // 3. Hybrid weighted score
                let final_score = (0.7 * sem_score) + (0.3 * kw_score);

                ScoredEpisode {
                    item: item.clone(),
                    score: final_score,
                }
            })
            .collect();

        // Sort descending by score
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.into_iter().take(limit).collect()
    }

    /// Persists episodic memory to `.minicode/episodic_memory.json`
    pub fn save(&self, workspace_root: &Path) -> Result<()> {
        let dir = workspace_root.join(".minicode");
        fs::create_dir_all(&dir)?;
        let path = dir.join("episodic_memory.json");
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Loads episodic memory from `.minicode/episodic_memory.json`
    pub fn load(workspace_root: &Path) -> Result<Self> {
        let path = workspace_root
            .join(".minicode")
            .join("episodic_memory.json");
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path)?;
        let mem: Self = serde_json::from_str(&content)?;
        Ok(mem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_episodic_memory_record_and_search() {
        let mut memory = EpisodicMemory::new();

        memory.record_episode(
            "Fix Tree-sitter ABI Segfault",
            "Aligned all tree-sitter grammars to ABI version 14 in Cargo.toml.",
            vec!["tree-sitter".to_string(), "segfault".to_string()],
            vec!["Cargo.toml".to_string()],
            "sess-1",
        );

        memory.record_episode(
            "Fast Diff Compactor",
            "Folded large diff lines in git output to save LLM context window tokens.",
            vec!["git".to_string(), "compactor".to_string()],
            vec!["src/tools/compactor.rs".to_string()],
            "sess-2",
        );

        let results = memory.search("tree-sitter crash", 2);
        assert!(!results.is_empty());
        assert_eq!(results[0].item.title, "Fix Tree-sitter ABI Segfault");
        assert!(results[0].score > 0.3);
    }

    #[test]
    fn test_episodic_memory_save_and_load() {
        let dir = tempdir().unwrap();
        let mut memory = EpisodicMemory::new();

        memory.record_episode(
            "Architecture Review",
            "Actor-critic verification loop implemented.",
            vec!["critic".to_string()],
            vec!["src/agent/critic.rs".to_string()],
            "sess-3",
        );

        memory.save(dir.path()).unwrap();

        let loaded = EpisodicMemory::load(dir.path()).unwrap();
        assert_eq!(loaded.episodes.len(), 1);
        assert_eq!(loaded.episodes[0].title, "Architecture Review");
    }
}
