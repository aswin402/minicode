//! Decision Record logging system with deterministic provenance (inspired by Semantica / ADRs).
//!
//! Automatically logs architectural, design, and code choices with context, rationale,
//! alternatives considered, and verification status to `.minicode/decisions.jsonl`.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// A single architectural or implementation decision record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionRecord {
    /// Unique identifier for this decision (e.g. "DEC-1727349123-1")
    pub id: String,
    /// RFC3339 timestamp
    pub timestamp: String,
    /// Turn ID during which the decision was made
    pub turn_id: usize,
    /// User requirement or architectural problem being addressed
    pub intent: String,
    /// Specific architectural or implementation choice made
    pub decision: String,
    /// Rationale explaining why this solution was chosen
    pub rationale: String,
    /// Alternative solutions considered and reasons rejected
    #[serde(default)]
    pub alternatives_considered: Vec<String>,
    /// Files created, modified, or impacted by this decision
    #[serde(default)]
    pub files_affected: Vec<String>,
    /// Whether this decision was verified with compiler/test checks
    pub verified: bool,
}

pub struct DecisionLogger;

impl DecisionLogger {
    pub const DECISIONS_DIR_NAME: &'static str = ".minicode";
    pub const DECISIONS_FILE_NAME: &'static str = "decisions.jsonl";

    /// Resolves canonical path to `.minicode/decisions.jsonl`
    pub fn file_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(Self::DECISIONS_DIR_NAME)
            .join(Self::DECISIONS_FILE_NAME)
    }

    /// Appends a new decision record to `.minicode/decisions.jsonl`
    pub fn append(workspace_root: &Path, record: &DecisionRecord) -> Result<()> {
        let dir = workspace_root.join(Self::DECISIONS_DIR_NAME);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), error = %e, "Failed to create .minicode directory");
        }

        let path = Self::file_path(workspace_root);
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;

        let line = serde_json::to_string(record)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Reads recent decisions from `.minicode/decisions.jsonl`
    pub fn load_recent(workspace_root: &Path, limit: usize) -> Result<Vec<DecisionRecord>> {
        let path = Self::file_path(workspace_root);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for line_res in reader.lines() {
            let Ok(l) = line_res else { break };
            let trimmed = l.trim();
            if !trimmed.is_empty() {
                if let Ok(rec) = serde_json::from_str::<DecisionRecord>(trimmed) {
                    records.push(rec);
                }
            }
        }

        if records.len() > limit {
            let start = records.len() - limit;
            records = records.split_off(start);
        }

        Ok(records)
    }

    /// Formats recent decision records into a concise summary block for agent context
    pub fn format_recent_for_context(workspace_root: &Path, limit: usize) -> String {
        let recent = Self::load_recent(workspace_root, limit).unwrap_or_default();
        if recent.is_empty() {
            return String::new();
        }

        let mut out = String::from("<recent_decision_records>\n");
        out.push_str("# Provenance of Recent Architectural Decisions:\n");
        for rec in recent {
            out.push_str(&format!(
                "- [{}] `{}`: {} (Rationale: {}; Files: {})\n",
                rec.id,
                rec.intent,
                rec.decision,
                rec.rationale,
                rec.files_affected.join(", ")
            ));
        }
        out.push_str("</recent_decision_records>\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_logger_append_and_load() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_decision_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rec = DecisionRecord {
            id: "DEC-001".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            turn_id: 1,
            intent: "Add temporal knowledge graph".to_string(),
            decision: "Store bi-temporal facts in .minicode/temporal_memory.json".to_string(),
            rationale: "Pure Rust serialization avoids external DB overhead".to_string(),
            alternatives_considered: vec!["SQLite".to_string()],
            files_affected: vec!["src/context/temporal_memory.rs".to_string()],
            verified: true,
        };

        DecisionLogger::append(&temp_dir, &rec).unwrap();
        let loaded = DecisionLogger::load_recent(&temp_dir, 5).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "DEC-001");
        assert_eq!(loaded[0].intent, "Add temporal knowledge graph");

        let context_str = DecisionLogger::format_recent_for_context(&temp_dir, 5);
        assert!(context_str.contains("DEC-001"));
        assert!(context_str.contains("temporal knowledge graph"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
