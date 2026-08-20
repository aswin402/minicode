use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskComplexityScore {
    pub task: String,
    pub score: u32,
    pub risk_level: String,
    pub estimated_tokens: usize,
    pub predicted_files: Vec<String>,
    pub blast_radius: usize,
    pub subtask_recommendations: Vec<String>,
}

impl TaskComplexityScore {
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 🎯 Task Complexity Assessment: Score {}/10 ({})\n\n",
            self.score, self.risk_level
        );

        out.push_str(&format!("📋 **Task:** {}\n\n", self.task));
        out.push_str(&format!(
            "📊 **Estimated Context:** ~{} tokens | **Blast Radius:** {} connected modules\n\n",
            self.estimated_tokens, self.blast_radius
        ));

        if !self.predicted_files.is_empty() {
            out.push_str("📁 **Predicted Relevant Files:**\n");
            for f in &self.predicted_files {
                out.push_str(&format!("- `{}`\n", f));
            }
            out.push('\n');
        }

        if !self.subtask_recommendations.is_empty() {
            out.push_str("💡 **Recommended Task Decomposition:**\n");
            for (idx, sub) in self.subtask_recommendations.iter().enumerate() {
                out.push_str(&format!("{}. {}\n", idx + 1, sub));
            }
            out.push('\n');
        }

        out
    }
}

pub struct TaskComplexityScorer;

impl TaskComplexityScorer {
    /// Analyzes task scope, codebase symbol graph, and blast radius to score complexity (1-10)
    pub fn score_task(
        workspace_root: &Path,
        task_description: &str,
    ) -> Result<TaskComplexityScore> {
        let task_lower = task_description.to_lowercase();
        let words: Vec<&str> = task_lower
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
            .filter(|w| w.len() > 2)
            .collect();

        // 1. Identify predicted relevant files from workspace
        let mut predicted_files = Vec::new();
        let walker = ignore::WalkBuilder::new(workspace_root)
            .hidden(true)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if path.is_file() {
                let rel_path = path
                    .strip_prefix(workspace_root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string();

                if rel_path.starts_with("target/") || rel_path.starts_with(".git/") {
                    continue;
                }

                // Check if file name matches any task word
                let rel_lower = rel_path.to_lowercase();
                for word in &words {
                    if rel_lower.contains(word) && !predicted_files.contains(&rel_path) {
                        predicted_files.push(rel_path.clone());
                        break;
                    }
                }
            }
        }

        // 2. Complexity heuristic scoring
        let mut raw_score = 1u32;

        // Keywords indicating cross-cutting refactoring or high-risk changes
        let high_risk_terms = [
            "refactor",
            "rewrite",
            "migrate",
            "database",
            "security",
            "auth",
            "concurrency",
            "async",
            "protocol",
            "architecture",
            "breaking",
            "all",
        ];
        let medium_risk_terms = [
            "add",
            "create",
            "implement",
            "update",
            "fix",
            "test",
            "support",
            "tool",
            "parse",
            "format",
            "render",
            "endpoint",
            "cache",
        ];

        for term in &high_risk_terms {
            if task_lower.contains(term) {
                raw_score += 2;
            }
        }
        for term in &medium_risk_terms {
            if task_lower.contains(term) {
                raw_score += 1;
            }
        }

        // Scale with file count
        if predicted_files.len() >= 5 {
            raw_score += 3;
        } else if predicted_files.len() >= 2 {
            raw_score += 1;
        }

        let score = raw_score.min(10);
        let risk_level = match score {
            1..=3 => "LOW",
            4..=6 => "MEDIUM",
            7..=8 => "HIGH",
            _ => "CRITICAL",
        }
        .to_string();

        let blast_radius = (predicted_files.len() * 2).max(1);
        let estimated_tokens = (predicted_files.len() * 1200 + 1500).max(2000);

        // Subtask decomposition recommendations
        let mut subtask_recommendations = Vec::new();
        if score >= 7 {
            subtask_recommendations.push(format!(
                "Stage 1: Analyze AST impact and symbol contracts across {} files",
                predicted_files.len().max(1)
            ));
            subtask_recommendations.push(
                "Stage 2: Implement core logic and data structures with isolated unit tests"
                    .to_string(),
            );
            subtask_recommendations.push(
                "Stage 3: Wire interfaces, update caller modules, and verify compilation"
                    .to_string(),
            );
            subtask_recommendations.push(
                "Stage 4: Run end-to-end integration tests and check architectural boundaries"
                    .to_string(),
            );
        } else if score >= 4 {
            subtask_recommendations
                .push("Stage 1: Implement feature changes and write unit tests".to_string());
            subtask_recommendations
                .push("Stage 2: Run verification suite (`cargo test` / lint)".to_string());
        } else {
            subtask_recommendations
                .push("Atomic implementation: execute direct edit and verify".to_string());
        }

        Ok(TaskComplexityScore {
            task: task_description.to_string(),
            score,
            risk_level,
            estimated_tokens,
            predicted_files,
            blast_radius,
            subtask_recommendations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_task_complexity_scoring_low() {
        let dir = tempdir().unwrap();
        let res = TaskComplexityScorer::score_task(dir.path(), "fix typo in docs").unwrap();
        assert!(res.score <= 3);
        assert_eq!(res.risk_level, "LOW");
    }

    #[test]
    fn test_task_complexity_scoring_high_refactor() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("auth.rs"), "pub fn auth() {}\n").unwrap();
        fs::write(src.join("session.rs"), "pub fn session() {}\n").unwrap();
        fs::write(src.join("database.rs"), "pub fn db() {}\n").unwrap();

        let res = TaskComplexityScorer::score_task(
            dir.path(),
            "Refactor all auth and database async concurrency architecture",
        )
        .unwrap();

        assert!(res.score >= 7);
        assert!(res.risk_level == "HIGH" || res.risk_level == "CRITICAL");
        assert!(!res.subtask_recommendations.is_empty());
    }
}
