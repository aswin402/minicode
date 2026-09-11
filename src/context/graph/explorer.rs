use crate::context::graph::CodeGraph;
use crate::context::layers::{ArchitecturalLayer, LayerClassifier};
use crate::context::repomap::SymbolDef;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Detailed information about an incoming or outgoing symbol reference
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolCallInfo {
    pub name: String,
    pub file_path: String,
    pub line: usize,
    pub kind: String,
}

/// A rich, surgical context payload for a single symbol match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExploreMatch {
    pub symbol_name: String,
    pub kind: String,
    pub file_path: String,
    pub line_range: (usize, usize),
    pub signature: String,
    pub layer: ArchitecturalLayer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_code: Option<String>,
    pub callers: Vec<SymbolCallInfo>,
    pub callees: Vec<SymbolCallInfo>,
    pub blast_radius_files: Vec<String>,
}

/// Complete exploration result containing matched symbols and rendered surgical summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExploreResult {
    pub query: String,
    pub target_symbol: Option<String>,
    pub matches: Vec<CodeExploreMatch>,
    pub summary: String,
}

/// Dense surgical context exploration engine
pub struct CodeExploreEngine;

impl CodeExploreEngine {
    /// Explores codebase structure for a query or specific symbol in a single dense call
    pub fn explore(
        workspace_root: &Path,
        graph: &CodeGraph,
        query: &str,
        target_symbol: Option<&str>,
        _max_depth: usize,
        include_source: bool,
    ) -> Result<CodeExploreResult> {
        let query_trimmed = query.trim();
        let target_sym_clean = target_symbol.map(|s| s.trim()).filter(|s| !s.is_empty());

        let mut matched_entries: Vec<(String, PathBuf, SymbolDef)> = Vec::new();

        // 1. If explicit target_symbol is provided, look it up first
        if let Some(sym_name) = target_sym_clean {
            if let Some(entries) = graph.symbol_to_file().get(sym_name) {
                for (path, sym) in entries {
                    matched_entries.push((sym_name.to_string(), path.clone(), sym.clone()));
                }
            }
        }

        // 2. If no explicit matches yet, search symbol_to_file keys by exact or case-insensitive match
        if matched_entries.is_empty() {
            let q_lower = query_trimmed.to_lowercase();

            // Try exact key match on query
            if let Some(entries) = graph.symbol_to_file().get(query_trimmed) {
                for (path, sym) in entries {
                    matched_entries.push((query_trimmed.to_string(), path.clone(), sym.clone()));
                }
            } else {
                // Substring / case-insensitive search across symbols
                for (name, entries) in graph.symbol_to_file() {
                    let name_lower = name.to_lowercase();
                    if name_lower == q_lower
                        || name_lower.contains(&q_lower)
                        || q_lower.contains(&name_lower)
                    {
                        for (path, sym) in entries {
                            matched_entries.push((name.clone(), path.clone(), sym.clone()));
                        }
                    }
                }
            }
        }

        // Limit results to top entries to keep payload surgical
        matched_entries.truncate(crate::constants::MAX_MATCHED_ENTRIES);

        // Pre-read file contents for source extraction and callee analysis
        let mut file_contents_cache: HashMap<PathBuf, String> = HashMap::new();
        let mut explore_matches = Vec::new();

        for (sym_name, path, sym) in &matched_entries {
            let rel_path = path
                .strip_prefix(workspace_root)
                .unwrap_or(path)
                .display()
                .to_string();

            let layer = LayerClassifier::classify_symbol(path, sym_name, &sym.kind);

            // Read source file lines
            let content = match file_contents_cache.get(path) {
                Some(c) => c.clone(),
                None => {
                    let c = std::fs::read_to_string(path).unwrap_or_default();
                    file_contents_cache.insert(path.clone(), c.clone());
                    c
                }
            };

            let lines: Vec<&str> = content.lines().collect();
            let start_idx = sym.line_number.saturating_sub(1);
            let end_idx = sym.end_line.min(lines.len());

            let source_code = if include_source && start_idx < lines.len() && start_idx <= end_idx {
                let chunk: Vec<&str> = lines[start_idx..end_idx].to_vec();
                let max_lines = crate::constants::MAX_SOURCE_LINES;
                let snippet = if chunk.len() > max_lines {
                    format!(
                        "{}\n    // ... [truncated {} lines]",
                        chunk[..max_lines].join("\n"),
                        chunk.len() - max_lines
                    )
                } else {
                    chunk.join("\n")
                };
                Some(snippet)
            } else {
                None
            };

            // Calculate Callees: What symbols/functions does this symbol's body invoke?
            let mut callees = Vec::new();
            if let Some(ref body) = source_code {
                for (other_sym_name, entries) in graph.symbol_to_file() {
                    if other_sym_name != sym_name
                        && !crate::constants::CODEGRAPH_IGNORED_IDENTIFIERS
                            .contains(&other_sym_name.as_str())
                        && body.contains(other_sym_name)
                    {
                        for (callee_path, callee_sym) in entries {
                            if callee_sym.kind != "import" {
                                let callee_rel = callee_path
                                    .strip_prefix(workspace_root)
                                    .unwrap_or(callee_path)
                                    .display()
                                    .to_string();
                                callees.push(SymbolCallInfo {
                                    name: callee_sym.name.clone(),
                                    file_path: callee_rel,
                                    line: callee_sym.line_number,
                                    kind: callee_sym.kind.clone(),
                                });
                            }
                        }
                    }
                }
            }
            callees.truncate(crate::constants::MAX_CALLEES);

            // Calculate Callers: Find incoming callers from other files referencing this symbol
            let mut callers = Vec::new();
            let mut seen_callers = HashSet::new();

            for (other_path, other_symbols) in graph.file_to_symbols() {
                if other_path == path {
                    continue;
                }
                let other_content = match file_contents_cache.get(other_path) {
                    Some(c) => c.clone(),
                    None => {
                        let c = std::fs::read_to_string(other_path).unwrap_or_default();
                        file_contents_cache.insert(other_path.clone(), c.clone());
                        c
                    }
                };

                if other_content.contains(sym_name) {
                    let other_rel = other_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(other_path)
                        .display()
                        .to_string();

                    // Find which function in the other file contains this reference
                    let mut found_enclosing = false;
                    for other_sym in other_symbols {
                        if other_sym.kind != "import"
                            && seen_callers.insert(format!("{}:{}", other_rel, other_sym.name))
                        {
                            callers.push(SymbolCallInfo {
                                name: other_sym.name.clone(),
                                file_path: other_rel.clone(),
                                line: other_sym.line_number,
                                kind: other_sym.kind.clone(),
                            });
                            found_enclosing = true;
                            break;
                        }
                    }
                    if !found_enclosing && seen_callers.insert(other_rel.clone()) {
                        callers.push(SymbolCallInfo {
                            name: format!("<file: {}>", other_rel),
                            file_path: other_rel,
                            line: 1,
                            kind: "file".to_string(),
                        });
                    }
                }
            }
            callers.truncate(crate::constants::MAX_CALLERS);

            // Blast Radius Files
            let blast_radius_files = match graph.get_blast_radius(sym_name, workspace_root) {
                Ok(report) => report.direct_dependents,
                Err(_) => Vec::new(),
            };

            explore_matches.push(CodeExploreMatch {
                symbol_name: sym_name.clone(),
                kind: sym.kind.clone(),
                file_path: rel_path,
                line_range: (sym.line_number, sym.end_line),
                signature: sym.signature.clone(),
                layer,
                doc_comment: sym.doc_comment.clone(),
                source_code,
                callers,
                callees,
                blast_radius_files,
            });
        }

        // Render Surgical Markdown Summary
        let summary = Self::render_markdown_summary(query, &explore_matches);

        Ok(CodeExploreResult {
            query: query.to_string(),
            target_symbol: target_symbol.map(|s| s.to_string()),
            matches: explore_matches,
            summary,
        })
    }

    /// Renders a dense, structured Markdown summary designed for single-shot agent consumption
    fn render_markdown_summary(query: &str, matches: &[CodeExploreMatch]) -> String {
        if matches.is_empty() {
            return format!(
                "### 🔍 CodeGraph Exploration: `{}`\n\nNo direct AST symbols matching `{}` were found in the codebase index.",
                query, query
            );
        }

        let mut out = format!(
            "### 🧭 CodeGraph Surgical Exploration: `{}` ({} matched symbols)\n\n",
            query,
            matches.len()
        );

        for (i, m) in matches.iter().enumerate() {
            out.push_str(&format!(
                "#### {}. `{}` — [{}] ({})\n",
                i + 1,
                m.symbol_name,
                m.kind,
                m.layer.badge()
            ));
            out.push_str(&format!(
                "- **Location**: `{}:{}-{}`\n",
                m.file_path, m.line_range.0, m.line_range.1
            ));
            out.push_str(&format!("- **Signature**: `{}`\n", m.signature));

            if let Some(doc) = &m.doc_comment {
                out.push_str(&format!("- **Doc**: *{}*\n", doc.trim()));
            }

            // Source code
            if let Some(src) = &m.source_code {
                out.push_str("\n```\n");
                out.push_str(src);
                out.push_str("\n```\n\n");
            }

            // Callers
            out.push_str(&format!("- **Incoming Callers ({})**:\n", m.callers.len()));
            if m.callers.is_empty() {
                out.push_str("  - *(None / Root entrypoint)*\n");
            } else {
                for c in &m.callers {
                    out.push_str(&format!(
                        "  - `← {}` in `{}:{}`\n",
                        c.name, c.file_path, c.line
                    ));
                }
            }

            // Callees
            out.push_str(&format!("- **Outgoing Calls ({})**:\n", m.callees.len()));
            if m.callees.is_empty() {
                out.push_str("  - *(None / Leaf function)*\n");
            } else {
                for c in &m.callees {
                    out.push_str(&format!(
                        "  - `→ {}` in `{}:{}`\n",
                        c.name, c.file_path, c.line
                    ));
                }
            }

            // Blast Radius
            if !m.blast_radius_files.is_empty() {
                out.push_str(&format!(
                    "- **Blast Radius Impact**: `{}`\n",
                    m.blast_radius_files.join("`, `")
                ));
            }

            out.push_str("\n---\n\n");
        }

        out
    }
}
