//! Bi-temporal architectural memory tracking system (inspired by Graphiti / temporal knowledge graphs).
//!
//! Tracks architectural invariants, dependencies, and component relationships with explicit
//! `valid_at` and `invalid_at` intervals to prevent stale architectural hallucinations.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A bi-temporal architectural fact recording relationships between codebase components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemporalFact {
    /// Subject component, struct, or module (e.g. "AgentLoop", "CodeGraph")
    pub subject: String,
    /// Relationship predicate (e.g. "depends_on", "implements", "routes_to", "manages")
    pub predicate: String,
    /// Target component, trait, or system (e.g. "ToolRegistry", "Provider")
    pub object: String,
    /// RFC3339 timestamp when this relationship was verified valid
    pub valid_at: String,
    /// RFC3339 timestamp when this relationship was invalidated/superseded (None if currently valid)
    pub invalid_at: Option<String>,
    /// Source file path where this architectural fact was discovered or declared
    pub source_file: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
}

impl TemporalFact {
    /// Checks if the fact is currently valid (i.e. has not been invalidated)
    pub fn is_valid(&self) -> bool {
        self.invalid_at.is_none()
    }
}

/// Store for persistent bi-temporal architectural facts in `.minicode/temporal_memory.json`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemporalMemoryStore {
    pub facts: Vec<TemporalFact>,
}

impl TemporalMemoryStore {
    pub const MEMORY_DIR_NAME: &'static str = ".minicode";
    pub const MEMORY_FILE_NAME: &'static str = "temporal_memory.json";

    /// Resolves canonical path to `.minicode/temporal_memory.json`
    pub fn file_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(Self::MEMORY_DIR_NAME)
            .join(Self::MEMORY_FILE_NAME)
    }

    /// Loads the temporal memory store from disk or returns an empty default store
    pub fn load_or_default(workspace_root: &Path) -> Self {
        let path = Self::file_path(workspace_root);
        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to read temporal memory; using default");
                Self::default()
            }
        }
    }

    /// Persists temporal memory store to `.minicode/temporal_memory.json` atomically
    pub fn save(&self, workspace_root: &Path) -> Result<()> {
        let dir = workspace_root.join(Self::MEMORY_DIR_NAME);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), error = %e, "Failed to create .minicode directory");
        }

        let path = Self::file_path(workspace_root);
        let tmp_path = dir.join(format!(
            "{}.tmp.{}",
            Self::MEMORY_FILE_NAME,
            uuid::Uuid::new_v4()
        ));

        let json = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        if let Ok(()) = std::fs::write(&tmp_path, json) {
            let _ = std::fs::rename(&tmp_path, &path);
        }

        Ok(())
    }

    /// Records or updates an architectural fact. If an older active fact with identical
    /// (subject, predicate) exists with a different object, it is automatically marked invalid.
    #[allow(dead_code)]
    pub fn record_fact(&mut self, subject: &str, predicate: &str, object: &str, source_file: &str) {
        let now = chrono::Utc::now().to_rfc3339();

        for fact in &mut self.facts {
            if fact.is_valid() && fact.subject == subject && fact.predicate == predicate {
                if fact.object == object {
                    // Identical fact already active; refresh source if updated
                    fact.source_file = source_file.to_string();
                    return;
                } else {
                    // Architectural evolution: invalidate prior fact
                    fact.invalid_at = Some(now.clone());
                }
            }
        }

        self.facts.push(TemporalFact {
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            valid_at: now,
            invalid_at: None,
            source_file: source_file.to_string(),
            confidence: 1.0,
        });
    }

    /// Invalidate active facts originating from a file that was modified
    pub fn invalidate_facts_for_file(&mut self, file_path: &str) -> usize {
        let now = chrono::Utc::now().to_rfc3339();
        let mut count = 0;
        for fact in &mut self.facts {
            if fact.is_valid()
                && (fact.source_file == file_path || fact.source_file.ends_with(file_path))
            {
                fact.invalid_at = Some(now.clone());
                count += 1;
            }
        }
        count
    }

    /// Query all currently valid facts, optionally filtered by subject
    pub fn query_valid_facts(&self, subject: Option<&str>) -> Vec<&TemporalFact> {
        self.facts
            .iter()
            .filter(|f| f.is_valid())
            .filter(|f| match subject {
                Some(s) => f.subject.eq_ignore_ascii_case(s),
                None => true,
            })
            .collect()
    }

    /// Formats currently valid architectural facts into a compact markdown representation
    /// for injection into the agent's reasoning context.
    pub fn format_active_context(&self) -> String {
        let valid_facts = self.query_valid_facts(None);
        if valid_facts.is_empty() {
            return String::new();
        }

        let mut out = String::from("<temporal_architecture_memory>\n");
        out.push_str("# Verified Active Architectural Invariants (Bi-Temporal Memory):\n");
        for fact in valid_facts {
            out.push_str(&format!(
                "- `{}` {} `{}` (source: `{}`)\n",
                fact.subject, fact.predicate, fact.object, fact.source_file
            ));
        }
        out.push_str("</temporal_architecture_memory>\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporal_memory_evolution_and_invalidation() {
        let mut store = TemporalMemoryStore::default();
        store.record_fact("Config", "loaded_by", "RawConfig", "src/config/mod.rs");

        let valid = store.query_valid_facts(Some("Config"));
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].object, "RawConfig");

        // Architectural change: Config is now loaded by DynamicConfig
        store.record_fact(
            "Config",
            "loaded_by",
            "DynamicConfig",
            "src/config/dynamic.rs",
        );

        let updated_valid = store.query_valid_facts(Some("Config"));
        assert_eq!(updated_valid.len(), 1);
        assert_eq!(updated_valid[0].object, "DynamicConfig");

        // Check total history: 2 facts, one invalidated
        assert_eq!(store.facts.len(), 2);
        assert!(store.facts[0].invalid_at.is_some());
        assert!(store.facts[1].invalid_at.is_none());

        // File-based invalidation
        let invalidated = store.invalidate_facts_for_file("src/config/dynamic.rs");
        assert_eq!(invalidated, 1);
        assert_eq!(store.query_valid_facts(Some("Config")).len(), 0);
    }
}
