//! Native Rust MiniReview Engine (`src/git/mini_review/`).
//!
//! Combines deterministic engineering pipelines (multi-gate file filtering, secret-path guard,
//! language & repo `.minicode/review_rules.json` / `.minireview/rule.json` resolution,
//! AST `CodeGraph` file clustering, diff-hunk line anchoring, SARIF 2.1.0 export) with
//! precision-first `MiniPub` Reviewer subagents.

pub mod cluster;
pub mod comments;
pub mod rules;

pub use cluster::{ReviewFileCluster, ReviewFileClusterer};
pub use comments::{
    AnchoredComment, AnchoredReviewSummary, CommentAnchorEngine, CommentSeverity, ReviewAudience,
    ReviewOutputFormat, ReviewPrecisionPrompts,
};
pub use rules::{
    builtin_language_rule_for_path, ExcludedFile, ExclusionReason, FileSelectionReport,
    RepoPathRule, RepoRuleConfig, ResolvedRuleGroup, ReviewRuleEngine, MINICODE_REVIEW_RULES_PATH,
    MINI_REVIEW_RULES_PATH,
};

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Zero-token deterministic preview of what a MiniReview run will inspect, exclude, cluster,
/// and enforce before invoking any LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPreviewReport {
    pub mode: String,
    pub rule_config_source: Option<String>,
    pub selection: FileSelectionReport,
    pub clusters: Vec<ReviewFileCluster>,
    pub rule_groups: Vec<ResolvedRuleGroup>,
}

/// High-level facade for the native Rust MiniReview Engine.
pub struct MiniReviewEngine;

impl MiniReviewEngine {
    /// Builds a deterministic zero-token review plan (filtering, secret-path protection, AST
    /// clustering, and language/repo rule resolution) for `candidate_files`.
    #[must_use]
    pub fn build_preview(
        workspace_root: &Path,
        mode: &str,
        candidate_files: &[String],
        explicit_rule_path: Option<&Path>,
    ) -> ReviewPreviewReport {
        let rule_engine = ReviewRuleEngine::load(workspace_root, explicit_rule_path);
        let selection = rule_engine.filter_files(candidate_files);
        let clusters = ReviewFileClusterer::cluster_files(
            workspace_root,
            &selection.reviewable_files,
            ReviewFileClusterer::DEFAULT_MAX_CLUSTER_SIZE,
            ReviewFileClusterer::DEFAULT_MAX_CLUSTERS,
        );
        let rule_groups = rule_engine.resolve_rule_groups(&selection.reviewable_files);
        ReviewPreviewReport {
            mode: mode.to_string(),
            rule_config_source: rule_engine.config_source().map(str::to_string),
            selection,
            clusters,
            rule_groups,
        }
    }

    /// Formats a [`ReviewPreviewReport`] cleanly for CLI/TUI/agent consumption.
    #[must_use]
    pub fn format_preview(preview: &ReviewPreviewReport, audience: ReviewAudience) -> String {
        if audience == ReviewAudience::Agent {
            return serde_json::to_string_pretty(preview).unwrap_or_default();
        }

        let mut out = format!("### 🔍 MiniReview Preview (`{}` mode)\n\n", preview.mode);
        out.push_str(&format!(
            "• **Reviewable Files:** {}\n",
            preview.selection.reviewable_files.len()
        ));
        out.push_str(&format!(
            "• **Excluded Files:** {} ({} secret/credential paths blocked)\n",
            preview.selection.excluded_files.len(),
            preview
                .selection
                .excluded_files
                .iter()
                .filter(|e| e.reason == ExclusionReason::SecretPath)
                .count()
        ));
        out.push_str(&format!(
            "• **AST Review Clusters:** {} (max {} files/cluster)\n",
            preview.clusters.len(),
            ReviewFileClusterer::DEFAULT_MAX_CLUSTER_SIZE
        ));
        out.push_str(&format!(
            "• **Active Rule Config:** {}\n",
            preview
                .rule_config_source
                .as_deref()
                .unwrap_or("built-in language precision rules")
        ));

        if !preview.clusters.is_empty() {
            out.push_str("\n#### Review Clusters\n");
            for c in &preview.clusters {
                out.push_str(&format!(
                    "- **Cluster #{}** (`{}` — {} file(s)): {}\n",
                    c.id,
                    c.label,
                    c.files.len(),
                    c.files
                        .iter()
                        .map(|f| format!("`{f}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }

        if !preview.rule_groups.is_empty() {
            out.push_str("\n#### Resolved Rule Groups\n");
            for g in &preview.rule_groups {
                out.push_str(&format!(
                    "- **`{}`** ({}) → {} file(s)\n",
                    g.rule_id,
                    g.source,
                    g.files.len()
                ));
            }
        }

        if !preview.selection.excluded_files.is_empty() {
            out.push_str("\n#### Excluded Paths\n");
            for ex in preview.selection.excluded_files.iter().take(15) {
                out.push_str(&format!("- `{}` — {}\n", ex.path, ex.reason.label()));
            }
            if preview.selection.excluded_files.len() > 15 {
                out.push_str(&format!(
                    "- _(+{} additional excluded path(s))_\n",
                    preview.selection.excluded_files.len() - 15
                ));
            }
        }

        out
    }
}
