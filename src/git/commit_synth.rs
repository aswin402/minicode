//! Automated Semantic Commit Synthesis & Conventional Changelog Generator
//!
//! Analyzes git working tree diffs, modified symbols, and file classifications
//! to synthesize atomic Conventional Commits and Keep-a-Changelog release entries.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{GitError, MinicodeError, Result};
use crate::git::service::GitService;

/// Conventional Commit Types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitType {
    Feat,
    Fix,
    Refactor,
    Test,
    Docs,
    Chore,
    Perf,
    Build,
    Ci,
}

impl CommitType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Feat => "feat",
            Self::Fix => "fix",
            Self::Refactor => "refactor",
            Self::Test => "test",
            Self::Docs => "docs",
            Self::Chore => "chore",
            Self::Perf => "perf",
            Self::Build => "build",
            Self::Ci => "ci",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Feat => "✨ feat",
            Self::Fix => "🐛 fix",
            Self::Refactor => "♻️ refactor",
            Self::Test => "🧪 test",
            Self::Docs => "📝 docs",
            Self::Chore => "🔧 chore",
            Self::Perf => "⚡ perf",
            Self::Build => "📦 build",
            Self::Ci => "👷 ci",
        }
    }
}

/// A single synthesized conventional commit proposal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SynthesizedCommitProposal {
    pub commit_type: CommitType,
    pub scope: Option<String>,
    pub summary: String,
    pub body: Option<String>,
    pub breaking_change: Option<String>,
    pub affected_files: Vec<String>,
}

impl SynthesizedCommitProposal {
    /// Formats the single-line commit title (e.g. `feat(context/flaky): add statistical variance analysis`)
    pub fn format_title(&self) -> String {
        let scope_part = match &self.scope {
            Some(s) if !s.is_empty() => format!("({})", s),
            _ => String::new(),
        };
        let breaking_marker = if self.breaking_change.is_some() {
            "!"
        } else {
            ""
        };
        format!(
            "{}{}{}: {}",
            self.commit_type.as_str(),
            scope_part,
            breaking_marker,
            self.summary
        )
    }

    /// Formats full git commit message with body and breaking change notes
    pub fn format_full_message(&self) -> String {
        let mut msg = self.format_title();
        if let Some(ref body) = self.body {
            msg.push_str("\n\n");
            msg.push_str(body);
        }
        if let Some(ref breaking) = self.breaking_change {
            msg.push_str("\n\nBREAKING CHANGE: ");
            msg.push_str(breaking);
        }
        msg
    }
}

/// Detailed report with both unified and atomic commit proposals plus changelog snippet
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitSynthesisReport {
    pub unified_proposal: SynthesizedCommitProposal,
    pub atomic_proposals: Vec<SynthesizedCommitProposal>,
    pub changelog_markdown: String,
    pub affected_files: Vec<String>,
    pub insertions: usize,
    pub deletions: usize,
}

impl CommitSynthesisReport {
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# 📝 Synthesized Semantic Commits & Release Changelog\n\n");
        out.push_str(&format!(
            "- **Impact Footprint:** {} file(s) changed (+{} / -{})\n\n",
            self.affected_files.len(),
            self.insertions,
            self.deletions
        ));

        out.push_str("### 🚀 Unified Conventional Commit (Squashed / Single Turn)\n");
        out.push_str(&format!(
            "```\n{}\n```\n\n",
            self.unified_proposal.format_full_message()
        ));

        if self.atomic_proposals.len() > 1 {
            out.push_str("### 🧩 Atomic Commit Sequence (Multi-Step Option)\n");
            for (idx, p) in self.atomic_proposals.iter().enumerate() {
                out.push_str(&format!(
                    "{}. **`{}`** — {} file(s) (`{}`)\n",
                    idx + 1,
                    p.format_title(),
                    p.affected_files.len(),
                    p.affected_files.join(", ")
                ));
            }
            out.push('\n');
        }

        out.push_str("### 📜 Release Changelog Entry (Keep-a-Changelog Format)\n");
        out.push_str(&format!("```markdown\n{}\n```\n", self.changelog_markdown));

        out
    }
}

/// Core engine for semantic commit synthesis and conventional changelog generation
pub struct SemanticCommitSynthesizer;

impl SemanticCommitSynthesizer {
    /// Analyzes git diff text, changed files, and optional task hints to synthesize commits
    pub fn analyze_diff(
        diff_text: &str,
        affected_files: &[String],
        task_hint: Option<&str>,
    ) -> CommitSynthesisReport {
        let mut insertions = 0;
        let mut deletions = 0;
        let mut extracted_symbols = Vec::new();

        for line in diff_text.lines() {
            if line.starts_with('+') && !line.starts_with("+++") {
                insertions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            } else if line.starts_with("@@") {
                // Extract symbol hints from hunk headers: e.g. "@@ ... @@ fn parse_query("
                if let Some(tail) = line.split("@@").nth(2) {
                    let trimmed = tail.trim();
                    if !trimmed.is_empty() {
                        let cleaned = trimmed
                            .split('(')
                            .next()
                            .unwrap_or(trimmed)
                            .trim_start_matches("pub ")
                            .trim_start_matches("async ")
                            .trim_start_matches("fn ")
                            .trim_start_matches("struct ")
                            .trim_start_matches("enum ")
                            .trim_start_matches("impl ")
                            .trim();
                        if !cleaned.is_empty() && !extracted_symbols.contains(&cleaned.to_string())
                        {
                            extracted_symbols.push(cleaned.to_string());
                        }
                    }
                }
            }
        }

        let primary_scope = Self::detect_primary_scope(affected_files);
        let commit_type = Self::detect_commit_type(affected_files, diff_text, task_hint);

        let summary = if let Some(hint) = task_hint {
            Self::clean_task_summary(hint)
        } else if !extracted_symbols.is_empty() {
            let sym_list = if extracted_symbols.len() <= 2 {
                extracted_symbols.join(" and ")
            } else {
                format!(
                    "{} and {} others",
                    extracted_symbols[0],
                    extracted_symbols.len() - 1
                )
            };
            format!("update {}", sym_list)
        } else if affected_files.len() == 1 {
            let fname = Path::new(&affected_files[0])
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&affected_files[0]);
            format!("update {}", fname)
        } else if !affected_files.is_empty() {
            format!(
                "update {} components across workspace",
                affected_files.len()
            )
        } else {
            "update workspace".to_string()
        };

        // Construct body bullet points from modified files and symbols
        let mut body_lines = Vec::new();
        for file in affected_files {
            body_lines.push(format!("- update `{}`", file));
        }
        let body = if !body_lines.is_empty() {
            Some(body_lines.join("\n"))
        } else {
            None
        };

        let unified_proposal = SynthesizedCommitProposal {
            commit_type,
            scope: primary_scope.clone(),
            summary: summary.clone(),
            body: body.clone(),
            breaking_change: None,
            affected_files: affected_files.to_vec(),
        };

        // Construct atomic proposals partitioned by functional domain
        let atomic_proposals = Self::partition_atomic_proposals(affected_files, task_hint);

        // Generate Keep-a-Changelog Markdown release draft
        let changelog_markdown = Self::generate_changelog_draft(
            commit_type,
            &summary,
            affected_files,
            &primary_scope,
            None,
        );

        CommitSynthesisReport {
            unified_proposal,
            atomic_proposals,
            changelog_markdown,
            affected_files: affected_files.to_vec(),
            insertions,
            deletions,
        }
    }

    /// Asynchronously inspects workspace git working tree and synthesizes semantic commits
    pub async fn synthesize(
        workspace_root: &Path,
        paths: Option<&[String]>,
        task_hint: Option<&str>,
    ) -> Result<CommitSynthesisReport> {
        let git = GitService::new(workspace_root.to_path_buf());
        if !git.is_git_repo().await {
            return Err(MinicodeError::Git(GitError::NotARepo {
                path: workspace_root.display().to_string(),
            }));
        }

        let status = git.get_status().await?;
        let mut affected: Vec<String> = Vec::new();

        if let Some(target_paths) = paths {
            for p in target_paths {
                if !affected.contains(p) {
                    affected.push(p.clone());
                }
            }
        } else {
            for f in status
                .staged
                .iter()
                .chain(status.unstaged.iter())
                .chain(status.untracked.iter())
            {
                if !affected.contains(f) {
                    affected.push(f.clone());
                }
            }
        }

        // Fetch diff text
        let diff_args = if !status.staged.is_empty() && status.unstaged.is_empty() {
            vec!["diff", "--cached"]
        } else {
            vec!["diff", "HEAD"]
        };

        let diff_text = git.run_git(&diff_args).await.unwrap_or_default();
        Ok(Self::analyze_diff(&diff_text, &affected, task_hint))
    }

    /// Asynchronously stages files and creates a git commit using the synthesized message
    pub async fn execute_commit(
        workspace_root: &Path,
        message: &str,
        paths: Option<&[String]>,
    ) -> Result<String> {
        let git = GitService::new(workspace_root.to_path_buf());
        let commit_svc = crate::git::commit::GitCommitService::new(&git);
        commit_svc.commit(message, paths).await
    }

    /// Extracts clean primary scope from changed file paths
    pub fn detect_primary_scope(files: &[String]) -> Option<String> {
        if files.is_empty() {
            return None;
        }

        let mut scopes = Vec::new();
        for f in files {
            if let Some(scope) = Self::scope_from_path(f) {
                if !scopes.contains(&scope) {
                    scopes.push(scope);
                }
            }
        }

        if scopes.len() == 1 {
            Some(scopes[0].clone())
        } else if scopes.is_empty() {
            None
        } else {
            // Pick the most common or core scope
            Some(scopes[0].clone())
        }
    }

    fn scope_from_path(path: &str) -> Option<String> {
        let clean = path.replace('\\', "/");
        if clean.starts_with("src/context/") {
            let rest = clean.trim_start_matches("src/context/");
            let module = rest
                .split('/')
                .next()
                .unwrap_or(rest)
                .trim_end_matches(".rs");
            Some(format!("context/{}", module))
        } else if clean.starts_with("src/tools/") {
            let rest = clean.trim_start_matches("src/tools/");
            let module = rest
                .split('/')
                .next()
                .unwrap_or(rest)
                .trim_end_matches(".rs");
            Some(format!("tools/{}", module))
        } else if clean.starts_with("src/agent/") {
            Some("agent".to_string())
        } else if clean.starts_with("src/ui/") {
            Some("ui".to_string())
        } else if clean.starts_with("src/git/") {
            Some("git".to_string())
        } else if clean.starts_with("src/session/") {
            Some("session".to_string())
        } else if clean.starts_with("tests/") {
            Some("tests".to_string())
        } else if clean.starts_with("onpkg_docs/") || clean.ends_with(".md") {
            Some("docs".to_string())
        } else if clean.ends_with(".toml") || clean.ends_with(".lock") {
            Some("deps".to_string())
        } else if clean.starts_with(".github/") {
            Some("ci".to_string())
        } else {
            Path::new(&clean)
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        }
    }

    /// Detects CommitType using keyword heuristics from task hints and file distribution
    pub fn detect_commit_type(
        files: &[String],
        diff_text: &str,
        task_hint: Option<&str>,
    ) -> CommitType {
        let has_src = files.iter().any(|f| f.starts_with("src/"));
        let has_tests_only = !files.is_empty()
            && files
                .iter()
                .all(|f| f.starts_with("tests/") || f.ends_with("_test.rs"));
        let has_docs_only = !files.is_empty()
            && files.iter().all(|f| {
                f.ends_with(".md") || f.starts_with("docs/") || f.starts_with("onpkg_docs/")
            });
        let has_build_only = !files.is_empty()
            && files
                .iter()
                .all(|f| f.ends_with(".toml") || f.ends_with(".lock"));

        if has_tests_only {
            return CommitType::Test;
        }
        if has_docs_only {
            return CommitType::Docs;
        }
        if has_build_only {
            return CommitType::Build;
        }

        if let Some(hint) = task_hint {
            let lower = hint.to_lowercase();
            let words: Vec<&str> = lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
                .collect();
            let has_word = |target: &str| words.contains(&target);
            let has_any_word = |targets: &[&str]| targets.iter().any(|t| has_word(t));

            if has_any_word(&[
                "fix", "fixes", "fixed", "bug", "bugs", "patch", "patches", "issue", "issues",
                "error", "errors", "hotfix",
            ]) {
                return CommitType::Fix;
            }
            if has_any_word(&["perf", "speed", "optimize", "optimization", "accelerate"]) {
                return CommitType::Perf;
            }
            if has_any_word(&[
                "refactor",
                "refactored",
                "refactoring",
                "clean",
                "cleanup",
                "simplify",
                "restructure",
            ]) {
                return CommitType::Refactor;
            }
            if (has_any_word(&["test", "tests", "testing"])
                || lower.contains("unit test")
                || lower.contains("integration test"))
                && !has_src
            {
                return CommitType::Test;
            }
            if has_any_word(&["doc", "docs", "documentation", "readme", "changelog"]) && !has_src {
                return CommitType::Docs;
            }
            if has_any_word(&["ci", "pipeline", "workflow"]) && !has_src {
                return CommitType::Ci;
            }
            if has_any_word(&[
                "build",
                "cargo",
                "dep",
                "deps",
                "dependency",
                "dependencies",
            ]) && !has_src
            {
                return CommitType::Build;
            }
            if has_any_word(&[
                "feat",
                "feature",
                "add",
                "added",
                "implement",
                "implemented",
                "support",
                "supported",
            ]) {
                return CommitType::Feat;
            }
        }

        // Check if diff contains fix patterns
        if diff_text.contains("fix")
            || diff_text.contains("workaround")
            || diff_text.contains("panic")
        {
            return CommitType::Fix;
        }

        CommitType::Feat
    }

    /// Sanitizes task hint into a clean lowercase conventional commit summary
    pub fn clean_task_summary(hint: &str) -> String {
        let trimmed = hint.trim();
        let cleaned = trimmed
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .trim_start_matches("feat:")
            .trim_start_matches("fix:")
            .trim_start_matches("refactor:")
            .trim_start_matches("test:")
            .trim_start_matches("docs:")
            .trim();

        let summary_str = if cleaned.is_empty() {
            "update codebase".to_string()
        } else {
            let mut chars = cleaned.chars();
            let first = chars.next().unwrap_or(' ').to_ascii_lowercase();
            format!("{}{}", first, chars.as_str())
        };

        if summary_str.chars().count() > crate::constants::MAX_COMMIT_SUMMARY_LEN {
            summary_str
                .chars()
                .take(crate::constants::MAX_COMMIT_SUMMARY_LEN)
                .collect()
        } else {
            summary_str
        }
    }

    /// Partitions changed files into logical atomic commit steps
    fn partition_atomic_proposals(
        files: &[String],
        task_hint: Option<&str>,
    ) -> Vec<SynthesizedCommitProposal> {
        let mut proposals = Vec::new();

        let mut core_code = Vec::new();
        let mut test_files = Vec::new();
        let mut doc_files = Vec::new();
        let mut build_files = Vec::new();

        for f in files {
            if f.starts_with("tests/") || f.ends_with("_test.rs") {
                test_files.push(f.clone());
            } else if f.ends_with(".md") || f.starts_with("onpkg_docs/") {
                doc_files.push(f.clone());
            } else if f.ends_with(".toml") || f.ends_with(".lock") {
                build_files.push(f.clone());
            } else {
                core_code.push(f.clone());
            }
        }

        let base_summary = task_hint
            .map(Self::clean_task_summary)
            .unwrap_or_else(|| "feature implementation".to_string());

        if !core_code.is_empty() {
            let scope = Self::detect_primary_scope(&core_code);
            proposals.push(SynthesizedCommitProposal {
                commit_type: CommitType::Feat,
                scope,
                summary: format!("implement {}", base_summary),
                body: Some(
                    core_code
                        .iter()
                        .map(|f| format!("- add/update `{}`", f))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                breaking_change: None,
                affected_files: core_code,
            });
        }

        if !test_files.is_empty() {
            proposals.push(SynthesizedCommitProposal {
                commit_type: CommitType::Test,
                scope: Some("tests".to_string()),
                summary: format!("add comprehensive tests for {}", base_summary),
                body: Some(
                    test_files
                        .iter()
                        .map(|f| format!("- verify via `{}`", f))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                breaking_change: None,
                affected_files: test_files,
            });
        }

        if !doc_files.is_empty() {
            proposals.push(SynthesizedCommitProposal {
                commit_type: CommitType::Docs,
                scope: Some("docs".to_string()),
                summary: format!("document {}", base_summary),
                body: Some(
                    doc_files
                        .iter()
                        .map(|f| format!("- update `{}`", f))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                breaking_change: None,
                affected_files: doc_files,
            });
        }

        if !build_files.is_empty() {
            proposals.push(SynthesizedCommitProposal {
                commit_type: CommitType::Build,
                scope: Some("deps".to_string()),
                summary: "update dependencies and configuration".to_string(),
                body: Some(
                    build_files
                        .iter()
                        .map(|f| format!("- configure `{}`", f))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                breaking_change: None,
                affected_files: build_files,
            });
        }

        if proposals.is_empty() {
            proposals.push(SynthesizedCommitProposal {
                commit_type: CommitType::Chore,
                scope: None,
                summary: "update workspace".to_string(),
                body: None,
                breaking_change: None,
                affected_files: files.to_vec(),
            });
        }

        proposals
    }

    /// Generates Keep-a-Changelog compatible release notes
    pub fn generate_changelog_draft(
        commit_type: CommitType,
        summary: &str,
        files: &[String],
        scope: &Option<String>,
        version: Option<&str>,
    ) -> String {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let ver = version.unwrap_or("Unreleased");
        let mut out = format!("## [{}] — {}\n\n", ver, today);

        let scope_prefix = match scope {
            Some(s) if !s.is_empty() => format!("**{}**: ", s),
            _ => String::new(),
        };

        match commit_type {
            CommitType::Feat => {
                out.push_str("### Added\n");
                out.push_str(&format!("- {}{}\n", scope_prefix, summary));
                if !files.is_empty() {
                    out.push_str(&format!("  - Affected: {}\n", files.join(", ")));
                }
            }
            CommitType::Fix => {
                out.push_str("### Fixed\n");
                out.push_str(&format!("- {}{}\n", scope_prefix, summary));
            }
            CommitType::Refactor | CommitType::Perf => {
                out.push_str("### Changed\n");
                out.push_str(&format!("- {}{}\n", scope_prefix, summary));
            }
            _ => {
                out.push_str("### Changed\n");
                out.push_str(&format!("- {}{}\n", scope_prefix, summary));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conventional_commit_format_title() {
        let proposal = SynthesizedCommitProposal {
            commit_type: CommitType::Feat,
            scope: Some("context/flaky".to_string()),
            summary: "add statistical variance analysis".to_string(),
            body: None,
            breaking_change: None,
            affected_files: vec!["src/context/flaky.rs".to_string()],
        };

        assert_eq!(
            proposal.format_title(),
            "feat(context/flaky): add statistical variance analysis"
        );
        assert_eq!(proposal.commit_type.badge(), "✨ feat");
    }

    #[test]
    fn test_conventional_commit_breaking_change() {
        let proposal = SynthesizedCommitProposal {
            commit_type: CommitType::Fix,
            scope: Some("tools".to_string()),
            summary: "redesign dispatch return contract".to_string(),
            body: Some("Detailed breaking explanation".to_string()),
            breaking_change: Some("Changes tool result type to Result<String>".to_string()),
            affected_files: vec!["src/tools/mod.rs".to_string()],
        };

        let title = proposal.format_title();
        assert_eq!(title, "fix(tools)!: redesign dispatch return contract");

        let full = proposal.format_full_message();
        assert!(full.contains("BREAKING CHANGE: Changes tool result type to Result<String>"));
    }

    #[test]
    fn test_detect_primary_scope() {
        let files = vec![
            "src/context/flaky.rs".to_string(),
            "src/context/mod.rs".to_string(),
        ];
        let scope = SemanticCommitSynthesizer::detect_primary_scope(&files);
        assert_eq!(scope, Some("context/flaky".to_string()));

        let test_files = vec!["tests/integration_flaky.rs".to_string()];
        assert_eq!(
            SemanticCommitSynthesizer::detect_primary_scope(&test_files),
            Some("tests".to_string())
        );
    }

    #[test]
    fn test_analyze_diff_and_changelog_generation() {
        let diff = r#"
diff --git a/src/context/flaky.rs b/src/context/flaky.rs
--- a/src/context/flaky.rs
+++ b/src/context/flaky.rs
@@ -10,6 +10,12 @@ pub struct FlakyTestDetector {
+    pub fn execute_burn_in() -> bool {
+        true
+    }
"#;
        let files = vec![
            "src/context/flaky.rs".to_string(),
            "tests/integration_flaky.rs".to_string(),
            "CHANGELOG.md".to_string(),
        ];

        let report = SemanticCommitSynthesizer::analyze_diff(
            diff,
            &files,
            Some("Automated flaky test quarantine"),
        );

        assert_eq!(report.unified_proposal.commit_type, CommitType::Feat);
        assert_eq!(
            report.unified_proposal.summary,
            "automated flaky test quarantine"
        );
        assert!(report.insertions > 0);
        assert_eq!(report.affected_files.len(), 3);
        assert!(report.atomic_proposals.len() >= 2);

        let changelog = &report.changelog_markdown;
        assert!(changelog.contains("### Added"));
        assert!(changelog.contains("automated flaky test quarantine"));

        let md = report.format_markdown();
        assert!(md.contains("Synthesized Semantic Commits & Release Changelog"));
        assert!(md.contains("Unified Conventional Commit"));
    }
}
