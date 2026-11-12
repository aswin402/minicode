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

const QUERY_STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "into", "that", "this", "these", "those", "all", "any",
    "some", "fix", "add", "update", "change", "make", "use", "using", "file", "files", "code",
    "typo", "docs", "doc", "readme", "please", "can", "you", "how", "what", "where", "when",
];

/// Cross-cutting architectural transformation stems (matched as discrete tokens, never substrings).
const ARCHITECTURAL_SCOPE_STEMS: &[&str] = &[
    "refactor",
    "migrat",
    "architectur",
    "concurrenc",
    "async",
    "distribut",
    "parallel",
    "secur",
    "cryptograph",
    "protocol",
    "consensus",
    "transact",
    "schema",
    "multi",
    "pipeline",
    "rewrit",
];

impl TaskComplexityScorer {
    /// Analyzes task scope, AST `CodeGraph` symbol/caller topology, PageRank centrality, and
    /// transitive blast radius to score complexity (`1..=10`) without substring heuristics.
    pub fn score_task(
        workspace_root: &Path,
        task_description: &str,
    ) -> Result<TaskComplexityScore> {
        let task_lower = task_description.to_ascii_lowercase();
        let raw_tokens: Vec<&str> = task_lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.' && c != '/')
            .filter(|w| w.len() >= 2)
            .collect();

        let mut query_words: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut query_stems: std::collections::HashSet<String> = std::collections::HashSet::new();
        for &tok in &raw_tokens {
            let trimmed = tok.trim_matches('.');
            if trimmed.len() < 3 || QUERY_STOPWORDS.contains(&trimmed) {
                continue;
            }
            query_words.insert(trimmed.to_string());
            let stem = crate::vault::models::VaultSkill::normalize_stem(trimmed);
            if stem.len() >= 3 && !QUERY_STOPWORDS.contains(&stem) {
                query_stems.insert(stem.to_string());
            }
            if trimmed.contains('_') {
                for part in trimmed.split('_').filter(|p| p.len() >= 3) {
                    if !QUERY_STOPWORDS.contains(&part) {
                        query_words.insert(part.to_string());
                        query_stems.insert(
                            crate::vault::models::VaultSkill::normalize_stem(part).to_string(),
                        );
                    }
                }
            }
        }

        // 1. Structural file & AST symbol localization
        let all_files =
            crate::context::walker::WorkspaceWalker::new(workspace_root).collect_relative_files();
        let mut predicted_set: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();

        for rel_path in &all_files {
            let rel_norm = rel_path.replace('\\', "/");
            let rel_lower = rel_norm.to_ascii_lowercase();

            // Exact relative path / filename mention in prompt
            if query_words.contains(&rel_lower) {
                predicted_set.insert(rel_norm);
                continue;
            }

            let path_obj = Path::new(&rel_lower);
            let stem_matched = path_obj
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| {
                    query_words.contains(stem)
                        || query_stems
                            .contains(crate::vault::models::VaultSkill::normalize_stem(stem))
                        || stem
                            .split(['_', '-', '.'])
                            .filter(|seg| seg.len() >= 3)
                            .any(|seg| {
                                query_words.contains(seg)
                                    || query_stems.contains(
                                        crate::vault::models::VaultSkill::normalize_stem(seg),
                                    )
                            })
                });
            if stem_matched {
                predicted_set.insert(rel_norm);
            }
        }

        // 2. Query AST CodeGraph for symbol matches, PageRank centrality, and transitive blast radius
        let graph_opt = crate::context::graph::CodeGraph::get_or_build_cached(workspace_root).ok();
        let mut matched_symbol_count = 0usize;
        let mut max_centrality_ratio = 0.0_f64;
        let mut impacted_modules: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut has_cyclic_dependency = false;

        if let Some(ref graph) = graph_opt {
            let symbol_ranks = graph.compute_symbol_pagerank(&[]);
            let total_nodes = symbol_ranks.len().max(1) as f64;
            let uniform_pr = 1.0 / total_nodes;

            for (sym_node, pr) in &symbol_ranks {
                if sym_node.kind == crate::context::graph::SymbolKind::File {
                    continue;
                }
                let sym_lower = sym_node.name.to_ascii_lowercase();
                let sym_stem = crate::vault::models::VaultSkill::normalize_stem(&sym_lower);
                let sym_hit = query_words.contains(&sym_lower)
                    || (sym_stem.len() >= 3 && query_stems.contains(sym_stem))
                    || sym_lower
                        .split('_')
                        .filter(|p| p.len() >= 4)
                        .any(|p| query_words.contains(p));

                if sym_hit {
                    matched_symbol_count += 1;
                    let rel = sym_node
                        .file_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(&sym_node.file_path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    predicted_set.insert(rel);
                    let ratio = *pr / uniform_pr;
                    if ratio > max_centrality_ratio {
                        max_centrality_ratio = ratio;
                    }
                }
            }

            for rel_file in &predicted_set {
                impacted_modules.insert(rel_file.clone());
                if let Ok(report) = graph.get_blast_radius(rel_file, workspace_root) {
                    if report.in_cyclic_dependency {
                        has_cyclic_dependency = true;
                    }
                    for dep in report
                        .direct_dependents
                        .into_iter()
                        .chain(report.transitive_dependents)
                    {
                        impacted_modules.insert(dep.replace('\\', "/"));
                    }
                }
            }
        }

        let predicted_files: Vec<String> = predicted_set.into_iter().collect();
        let external_dependents = impacted_modules.len().saturating_sub(predicted_files.len());
        let blast_radius = impacted_modules.len().max(predicted_files.len()).max(1);

        // 3. Multi-factor structural complexity score (1..=10)
        let mut raw_score = 1u32;

        // Factor A: Predicted target file fan-out
        if predicted_files.len() >= crate::constants::COMPLEXITY_HIGH_FILE_COUNT_THRESHOLD {
            raw_score += 3;
        } else if predicted_files.len() >= crate::constants::COMPLEXITY_MEDIUM_FILE_COUNT_THRESHOLD
            || predicted_files.len() >= 2
        {
            raw_score += 2;
        } else if !predicted_files.is_empty() {
            raw_score += 1;
        }

        // Factor B: Transitive CodeGraph blast radius + SCC cycle coupling
        if external_dependents >= 6 {
            raw_score += 3;
        } else if external_dependents >= 2 {
            raw_score += 2;
        } else if external_dependents >= 1 {
            raw_score += 1;
        }
        if has_cyclic_dependency {
            raw_score += 1;
        }

        // Factor C: AST symbol surface & PageRank hub centrality
        if matched_symbol_count >= 4 {
            raw_score += 2;
        } else if matched_symbol_count >= 1 {
            raw_score += 1;
        }
        if max_centrality_ratio >= 2.0 {
            raw_score += 1;
        }

        // Factor D: Discrete architectural transformation breadth (tokenized stems, zero substring false positives)
        let arch_breadth_hits = raw_tokens
            .iter()
            .filter(|&&tok| {
                let stem = crate::vault::models::VaultSkill::normalize_stem(tok);
                ARCHITECTURAL_SCOPE_STEMS
                    .iter()
                    .any(|&arch| stem.starts_with(arch) || tok.starts_with(arch))
            })
            .count();
        raw_score += (arch_breadth_hits as u32).min(4);

        let domain_spread =
            crate::context::search::intent_filter::IntentClassifier::detect(task_description).len();
        if domain_spread >= 3 {
            raw_score += 1;
        }

        let score = raw_score.clamp(1, 10);
        let risk_level = match score {
            1..=3 => "LOW",
            4..=6 => "MEDIUM",
            7..=8 => "HIGH",
            _ => "CRITICAL",
        }
        .to_string();

        let estimated_tokens = (blast_radius * crate::constants::COMPLEXITY_TOKENS_PER_FILE
            + crate::constants::COMPLEXITY_BASE_TOKENS)
            .max(crate::constants::COMPLEXITY_MIN_ESTIMATED_TOKENS);

        // Subtask decomposition recommendations
        let mut subtask_recommendations = Vec::new();
        if score >= 7 {
            subtask_recommendations.push(format!(
                "Stage 1: Analyze AST impact and symbol contracts across {} target / {} connected modules",
                predicted_files.len().max(1),
                blast_radius
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
