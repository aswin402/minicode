use crate::context::ast_diff::AstDiffEngine;
use crate::context::graph::CodeGraph;
use crate::error::Result;
use crate::git::diff_viewer::{GitDiffFile, GitDiffViewer};
use crate::git::service::GitService;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Nature of modification applied to an AST symbol
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SymbolMutationType {
    Added,
    BodyModified,
    SignatureChanged,
    Deleted,
}

impl SymbolMutationType {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Added => "➕ Added",
            Self::BodyModified => "🔄 Body Modified",
            Self::SignatureChanged => "⚠️ Signature Modified",
            Self::Deleted => "➖ Deleted",
        }
    }
}

/// A specific AST symbol node intersected with git diff hunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedSymbolChange {
    pub symbol_name: String,
    pub qualified_name: String,
    pub kind: String,
    pub is_public: bool,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub mutation_type: SymbolMutationType,
    pub diff_hunk_lines: Vec<usize>,
    pub direct_callers: Vec<String>,
    pub transitive_callers: Vec<String>,
    pub pagerank_score: f64,
    pub is_breaking: bool,
    pub breaking_reason: Option<String>,
}

/// Comprehensive report of all projected symbol mutations across a git diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffProjectionReport {
    pub total_files: usize,
    pub total_symbols_modified: usize,
    pub changes: Vec<ProjectedSymbolChange>,
    pub affected_caller_files: Vec<String>,
    pub high_risk_symbols: Vec<ProjectedSymbolChange>,
    pub breaking_changes: Vec<ProjectedSymbolChange>,
}

impl DiffProjectionReport {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Engine that projects raw Git diff hunks onto Tree-sitter AST symbol nodes
/// and traces symbol-level call-graph blast radius.
pub struct DiffProjector;

impl DiffProjector {
    /// Projects uncommitted or staged git changes in a workspace onto AST symbols
    /// and computes call-graph blast radius for each changed symbol.
    pub async fn project_workspace_diff(
        workspace_root: &Path,
        staged_only: bool,
        code_graph_opt: Option<&CodeGraph>,
    ) -> Result<DiffProjectionReport> {
        let git = GitService::new(workspace_root.to_path_buf());
        if !git.is_git_repo().await {
            return Ok(DiffProjectionReport {
                total_files: 0,
                total_symbols_modified: 0,
                changes: vec![],
                affected_caller_files: vec![],
                high_risk_symbols: vec![],
                breaking_changes: vec![],
            });
        }

        let diff_files = GitDiffViewer::load_diffs(workspace_root, staged_only).await?;
        if diff_files.is_empty() {
            return Ok(DiffProjectionReport {
                total_files: 0,
                total_symbols_modified: 0,
                changes: vec![],
                affected_caller_files: vec![],
                high_risk_symbols: vec![],
                breaking_changes: vec![],
            });
        }

        let mut owned_graph = CodeGraph::new();
        let graph = if let Some(g) = code_graph_opt {
            g
        } else {
            let _ = owned_graph.build_graph(workspace_root);
            &owned_graph
        };

        Self::project_diff_files(workspace_root, &diff_files, graph, &git).await
    }

    /// Projects parsed `GitDiffFile` entries onto AST symbols with call graph analysis.
    pub async fn project_diff_files(
        workspace_root: &Path,
        diff_files: &[GitDiffFile],
        graph: &CodeGraph,
        git: &GitService,
    ) -> Result<DiffProjectionReport> {
        let mut changes = Vec::new();
        let mut affected_caller_files_set = HashSet::new();

        // Compute symbol pagerank mapping
        let pagerank_list = graph.compute_symbol_pagerank(&[]);
        let mut pagerank_map: HashMap<String, f64> = HashMap::new();
        for (node, score) in pagerank_list {
            pagerank_map.insert(node.name.clone(), score);
            pagerank_map.insert(node.qualified_name.clone(), score);
        }

        for diff_file in diff_files {
            let ext = Path::new(&diff_file.path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            if !matches!(ext, "rs" | "ts" | "tsx" | "js" | "jsx" | "py") {
                continue;
            }

            // Extract diff lines for this file
            let modified_linenos: Vec<usize> = diff_file
                .lines
                .iter()
                .filter(|l| l.tag == '+' || l.tag == '-')
                .filter_map(|l| l.new_lineno.or(l.old_lineno))
                .collect();

            let old_source = git
                .show_file_at_head(&diff_file.path)
                .await
                .unwrap_or_default();

            let full_new_path = workspace_root.join(&diff_file.path);
            let new_source = tokio::fs::read_to_string(&full_new_path)
                .await
                .unwrap_or_default();

            // Run AST diff engine
            let delta_report =
                match AstDiffEngine::diff_sources(&diff_file.path, ext, &old_source, &new_source) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

            // 1. Process Added Symbols
            for added in &delta_report.added {
                let sym_hunk_lines: Vec<usize> = modified_linenos
                    .iter()
                    .copied()
                    .filter(|&line| line >= added.start_line && line <= added.end_line)
                    .collect();

                let qualified_name = format!("{}::{}", diff_file.path, added.name);
                let pagerank = pagerank_map.get(&added.name).copied().unwrap_or(0.0);

                let blast = graph.get_blast_radius(&added.name, workspace_root).ok();
                let direct_callers = blast
                    .as_ref()
                    .map(|b| b.direct_caller_symbols.clone())
                    .unwrap_or_default();
                let transitive_callers = blast
                    .as_ref()
                    .map(|b| b.transitive_caller_symbols.clone())
                    .unwrap_or_default();

                changes.push(ProjectedSymbolChange {
                    symbol_name: added.name.clone(),
                    qualified_name,
                    kind: added.kind.clone(),
                    is_public: added.is_public,
                    file_path: diff_file.path.clone(),
                    start_line: added.start_line,
                    end_line: added.end_line,
                    mutation_type: SymbolMutationType::Added,
                    diff_hunk_lines: sym_hunk_lines,
                    direct_callers,
                    transitive_callers,
                    pagerank_score: pagerank,
                    is_breaking: false,
                    breaking_reason: None,
                });
            }

            // 2. Process Modified Symbols
            for modified in &delta_report.modified {
                let sym_hunk_lines: Vec<usize> = modified_linenos
                    .iter()
                    .copied()
                    .filter(|&line| {
                        line >= modified.new_start_line && line <= modified.new_end_line
                    })
                    .collect();

                let qualified_name = format!("{}::{}", diff_file.path, modified.name);
                let pagerank = pagerank_map.get(&modified.name).copied().unwrap_or(0.0);

                let blast = graph.get_blast_radius(&modified.name, workspace_root).ok();
                let direct_callers = blast
                    .as_ref()
                    .map(|b| b.direct_caller_symbols.clone())
                    .unwrap_or_default();
                let transitive_callers = blast
                    .as_ref()
                    .map(|b| b.transitive_caller_symbols.clone())
                    .unwrap_or_default();

                if let Some(ref b) = blast {
                    for dep_file in &b.direct_dependents {
                        if dep_file != &diff_file.path {
                            affected_caller_files_set.insert(dep_file.clone());
                        }
                    }
                }

                let mutation_type = if modified.signature_changed {
                    SymbolMutationType::SignatureChanged
                } else {
                    SymbolMutationType::BodyModified
                };

                let mut is_breaking = false;
                let mut breaking_reason = None;

                if modified.is_public && modified.signature_changed {
                    is_breaking = true;
                    breaking_reason = Some(format!(
                        "Public signature modified: was `{}` -> now `{}` (affects {} callers)",
                        modified.old_signature.trim(),
                        modified.new_signature.trim(),
                        direct_callers.len()
                    ));
                }

                changes.push(ProjectedSymbolChange {
                    symbol_name: modified.name.clone(),
                    qualified_name,
                    kind: modified.kind.clone(),
                    is_public: modified.is_public,
                    file_path: diff_file.path.clone(),
                    start_line: modified.new_start_line,
                    end_line: modified.new_end_line,
                    mutation_type,
                    diff_hunk_lines: sym_hunk_lines,
                    direct_callers,
                    transitive_callers,
                    pagerank_score: pagerank,
                    is_breaking,
                    breaking_reason,
                });
            }

            // 3. Process Removed Symbols
            for removed in &delta_report.removed {
                let qualified_name = format!("{}::{}", diff_file.path, removed.name);
                let pagerank = pagerank_map.get(&removed.name).copied().unwrap_or(0.0);

                let blast = graph.get_blast_radius(&removed.name, workspace_root).ok();
                let direct_callers = blast
                    .as_ref()
                    .map(|b| b.direct_caller_symbols.clone())
                    .unwrap_or_default();
                let transitive_callers = blast
                    .as_ref()
                    .map(|b| b.transitive_caller_symbols.clone())
                    .unwrap_or_default();

                if let Some(ref b) = blast {
                    for dep_file in &b.direct_dependents {
                        if dep_file != &diff_file.path {
                            affected_caller_files_set.insert(dep_file.clone());
                        }
                    }
                }

                let is_breaking = !direct_callers.is_empty() || removed.is_public;
                let breaking_reason = if is_breaking {
                    Some(format!(
                        "Deleted symbol `{}` has {} active caller references in codebase",
                        removed.name,
                        direct_callers.len()
                    ))
                } else {
                    None
                };

                changes.push(ProjectedSymbolChange {
                    symbol_name: removed.name.clone(),
                    qualified_name,
                    kind: removed.kind.clone(),
                    is_public: removed.is_public,
                    file_path: diff_file.path.clone(),
                    start_line: removed.start_line,
                    end_line: removed.end_line,
                    mutation_type: SymbolMutationType::Deleted,
                    diff_hunk_lines: vec![],
                    direct_callers,
                    transitive_callers,
                    pagerank_score: pagerank,
                    is_breaking,
                    breaking_reason,
                });
            }
        }

        let mut affected_caller_files: Vec<String> =
            affected_caller_files_set.into_iter().collect();
        affected_caller_files.sort();

        let mut high_risk_symbols: Vec<ProjectedSymbolChange> = changes
            .iter()
            .filter(|c| c.pagerank_score > 0.03 || c.direct_callers.len() > 3 || c.is_breaking)
            .cloned()
            .collect();
        high_risk_symbols.sort_by(|a, b| {
            b.pagerank_score
                .partial_cmp(&a.pagerank_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let breaking_changes: Vec<ProjectedSymbolChange> =
            changes.iter().filter(|c| c.is_breaking).cloned().collect();

        Ok(DiffProjectionReport {
            total_files: diff_files.len(),
            total_symbols_modified: changes.len(),
            changes,
            affected_caller_files,
            high_risk_symbols,
            breaking_changes,
        })
    }

    /// Formats the diff projection report into structured markdown with caller trees
    pub fn format_markdown(report: &DiffProjectionReport) -> String {
        if report.is_empty() {
            return "### 🔍 Symbol-Level Diff Impact\n\nNo AST symbols affected by current git diff.\n"
                .to_string();
        }

        let mut out = format!(
            "### 🔍 Symbol-Level Diff Projection ({} symbols affected in {} files)\n\n",
            report.total_symbols_modified, report.total_files
        );

        if !report.breaking_changes.is_empty() {
            out.push_str("#### ⚠️ Potential Breaking API Mutations\n");
            for b in &report.breaking_changes {
                let reason = b
                    .breaking_reason
                    .as_deref()
                    .unwrap_or("Public signature modified");
                out.push_str(&format!(
                    "- **`{}`** (`{}`) in `{}` (lines {}-{}):\n  • *Warning:* {}\n",
                    b.symbol_name, b.kind, b.file_path, b.start_line, b.end_line, reason
                ));
            }
            out.push('\n');
        }

        if !report.high_risk_symbols.is_empty() {
            out.push_str("#### 🎯 High-Risk / High-Centrality Symbols\n");
            for h in &report.high_risk_symbols {
                out.push_str(&format!(
                    "- **`{}`** (`{}`) — {} (PageRank: {:.4}, Callers: {})\n",
                    h.symbol_name,
                    h.kind,
                    h.mutation_type.badge(),
                    h.pagerank_score,
                    h.direct_callers.len()
                ));
                if !h.direct_callers.is_empty() {
                    let caller_preview = h
                        .direct_callers
                        .iter()
                        .take(4)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("  • *Callers:* `{}`\n", caller_preview));
                }
            }
            out.push('\n');
        }

        out.push_str("#### 📋 All Changed Symbols & Call Impact\n");
        for c in &report.changes {
            let vis = if c.is_public { "pub " } else { "" };
            out.push_str(&format!(
                "- `{}` **`{}{}`** in `{}` (lines {}-{}) — {}\n",
                c.kind,
                vis,
                c.symbol_name,
                c.file_path,
                c.start_line,
                c.end_line,
                c.mutation_type.badge()
            ));

            if !c.direct_callers.is_empty() {
                out.push_str(&format!(
                    "  • **Direct Callers ({})**: `{}`\n",
                    c.direct_callers.len(),
                    c.direct_callers.join("`, `")
                ));
            }
        }

        if !report.affected_caller_files.is_empty() {
            out.push_str(&format!(
                "\n**Downstream Files Requiring Verification ({})**: `{}`\n",
                report.affected_caller_files.len(),
                report.affected_caller_files.join("`, `")
            ));
        }

        out
    }
}
