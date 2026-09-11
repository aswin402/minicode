use crate::context::graph::{CodeGraph, SymbolKind, SymbolNode};
use crate::error::Result;
use petgraph::graph::NodeIndex;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::path::Path;

/// Categorization of unreachable or redundant AST symbols
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum DeadCodeKind {
    DeadFunction,
    DeadStruct,
    DeadEnum,
    DeadIslandCluster,
}

impl DeadCodeKind {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::DeadFunction => "⚡ [DEAD FUNCTION]",
            Self::DeadStruct => "📦 [DEAD STRUCT]",
            Self::DeadEnum => "🏷️ [DEAD ENUM]",
            Self::DeadIslandCluster => "🏝️ [DEAD ISLAND CLUSTER]",
        }
    }
}

/// Metadata describing a specific unreachable dead code candidate
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeadSymbol {
    pub name: String,
    pub kind: DeadCodeKind,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub confidence: String, // "HIGH", "MEDIUM", "LOW"
    pub reason: String,
    pub estimated_loc: usize,
}

/// Summary report of codebase reachability and dead code cleanup candidates
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeadCodeReport {
    pub total_symbols_checked: usize,
    pub reachable_symbols: usize,
    pub dead_symbols: Vec<DeadSymbol>,
    pub potential_lines_saved: usize,
}

impl DeadCodeReport {
    /// Formats the dead code report into a structured markdown scorecard
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 🧹 AST-Guided Dead Code & Redundant Symbol Report\n\n\
            📊 **Reachability Analysis Summary:**\n\
            - **Total Symbols Analyzed:** {}\n\
            - **Reachable / Active Symbols:** {}\n\
            - **Unreachable Candidates:** {}\n\
            - **Potential Lines Saved:** ~{} LOC\n\n",
            self.total_symbols_checked,
            self.reachable_symbols,
            self.dead_symbols.len(),
            self.potential_lines_saved
        );

        if self.dead_symbols.is_empty() {
            out.push_str("✅ **100% Codebase Utilization:** No unreachable dead functions, structs, or isolated call clusters detected.\n");
            return out;
        }

        out.push_str("### 🗑️ Dead Code Pruning Candidates:\n\n");

        for sym in &self.dead_symbols {
            let conf_badge = match sym.confidence.as_str() {
                "HIGH" => "🟢 High Confidence",
                "MEDIUM" => "🟡 Medium Confidence",
                _ => "🔵 Low Confidence",
            };

            out.push_str(&format!(
                "- **{}** `{}` ({})\n  • **Location:** `{}:lines {}-{}` (~{} lines)\n  • **Reason:** {}\n\n",
                sym.kind.badge(),
                sym.name,
                conf_badge,
                sym.file_path,
                sym.start_line,
                sym.end_line,
                sym.estimated_loc,
                sym.reason
            ));
        }

        out
    }
}

pub struct DeadCodeEliminator;

impl DeadCodeEliminator {
    /// Runs root-driven reachability BFS over the code graph to identify unreachable symbols
    pub fn analyze_workspace(
        workspace_root: &Path,
        provided_graph: Option<&CodeGraph>,
        target_file: Option<&str>,
        min_confidence: Option<&str>,
    ) -> Result<DeadCodeReport> {
        let mut local_graph;
        let graph = match provided_graph {
            Some(g) => g,
            None => {
                local_graph = CodeGraph::new();
                let _ = local_graph.build_graph(workspace_root);
                &local_graph
            }
        };

        let petgraph_ref = graph.graph();
        let mut entrypoints = Vec::new();
        let mut all_symbol_indices = Vec::new();

        // 1. Identify Seed Entrypoint Roots
        for node_idx in petgraph_ref.node_indices() {
            if let Some(node) = petgraph_ref.node_weight(node_idx) {
                if node.kind == SymbolKind::File {
                    continue;
                }

                all_symbol_indices.push(node_idx);

                let file_str = node.file_path.to_string_lossy();
                let is_entrypoint = file_str.ends_with("main.rs")
                    || file_str.ends_with("lib.rs")
                    || file_str.contains("/bin/")
                    || file_str.contains("/tests/")
                    || node.name == "main"
                    || node.name.starts_with("test_");

                if is_entrypoint {
                    entrypoints.push(node_idx);
                }
            }
        }

        // 2. Multi-Source BFS Reachability Tracing (Mark Phase)
        let mut reachable: HashSet<NodeIndex> = HashSet::new();
        let mut queue: VecDeque<NodeIndex> = VecDeque::new();

        for root in entrypoints {
            if reachable.insert(root) {
                queue.push_back(root);
            }
        }

        while let Some(curr_idx) = queue.pop_front() {
            for neighbor in petgraph_ref.neighbors_directed(curr_idx, petgraph::Direction::Outgoing)
            {
                if let Some(w) = petgraph_ref.node_weight(neighbor) {
                    if w.kind != SymbolKind::File && reachable.insert(neighbor) {
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        // 3. Sweep Phase: Identify Unreachable Symbols
        let mut dead_symbols = Vec::new();
        let mut total_lines_saved = 0;

        for &node_idx in &all_symbol_indices {
            if reachable.contains(&node_idx) {
                continue;
            }

            if let Some(node) = petgraph_ref.node_weight(node_idx) {
                let rel_path = node
                    .file_path
                    .strip_prefix(workspace_root)
                    .unwrap_or(&node.file_path)
                    .display()
                    .to_string();

                if let Some(tf) = target_file {
                    if rel_path != tf {
                        continue;
                    }
                }

                let (kind, confidence, reason) =
                    Self::classify_dead_symbol(node, petgraph_ref, node_idx);

                let conf_filter = min_confidence.unwrap_or("all").to_lowercase();
                if conf_filter == "high" && confidence != "HIGH" {
                    continue;
                }
                if conf_filter == "medium" && confidence == "LOW" {
                    continue;
                }

                let loc = node.end_line.saturating_sub(node.start_line) + 1;
                total_lines_saved += loc;

                dead_symbols.push(DeadSymbol {
                    name: node.name.clone(),
                    kind,
                    file_path: rel_path,
                    start_line: node.start_line,
                    end_line: node.end_line,
                    confidence,
                    reason,
                    estimated_loc: loc,
                });
            }
        }

        let total_symbols = all_symbol_indices.len();
        let reachable_count = reachable
            .iter()
            .filter(|&&idx| {
                petgraph_ref
                    .node_weight(idx)
                    .map(|w| w.kind != SymbolKind::File)
                    .unwrap_or(false)
            })
            .count();

        Ok(DeadCodeReport {
            total_symbols_checked: total_symbols,
            reachable_symbols: reachable_count,
            dead_symbols,
            potential_lines_saved: total_lines_saved,
        })
    }

    fn classify_dead_symbol(
        node: &SymbolNode,
        graph: &petgraph::graph::DiGraph<SymbolNode, crate::context::graph::EdgeKind>,
        node_idx: NodeIndex,
    ) -> (DeadCodeKind, String, String) {
        let incoming_callers = graph
            .neighbors_directed(node_idx, petgraph::Direction::Incoming)
            .filter(|n| {
                graph
                    .node_weight(*n)
                    .map(|w| w.kind != SymbolKind::File)
                    .unwrap_or(false)
            })
            .count();
        let outgoing_callees = graph
            .neighbors_directed(node_idx, petgraph::Direction::Outgoing)
            .filter(|n| {
                graph
                    .node_weight(*n)
                    .map(|w| w.kind != SymbolKind::File)
                    .unwrap_or(false)
            })
            .count();

        // Check if part of an isolated cycle / cluster
        if incoming_callers > 0 && outgoing_callees > 0 {
            return (
                DeadCodeKind::DeadIslandCluster,
                "HIGH".to_string(),
                format!(
                    "Symbol is part of an isolated cluster ({} internal callers, {} callees) disconnected from all entrypoints",
                    incoming_callers, outgoing_callees
                ),
            );
        }

        let kind = match node.kind {
            SymbolKind::Function | SymbolKind::Method => DeadCodeKind::DeadFunction,
            SymbolKind::Struct | SymbolKind::Class | SymbolKind::Interface => {
                DeadCodeKind::DeadStruct
            }
            SymbolKind::Enum => DeadCodeKind::DeadEnum,
            _ => DeadCodeKind::DeadFunction,
        };

        if incoming_callers == 0 {
            (
                kind,
                "HIGH".to_string(),
                "Zero incoming calls or references across the entire codebase".to_string(),
            )
        } else {
            (
                kind,
                "MEDIUM".to_string(),
                format!(
                    "Only called by other unreachable dead symbols ({} incoming dead calls)",
                    incoming_callers
                ),
            )
        }
    }
}
