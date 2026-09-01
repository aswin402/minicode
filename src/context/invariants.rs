use crate::context::graph::{CodeGraph, SymbolKind, SymbolNode};
use crate::context::layers::{ArchitecturalLayer, LayerClassifier};
use crate::error::Result;
use petgraph::algo::tarjan_scc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// A detected architecture or dependency invariant violation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvariantViolation {
    pub rule_id: String,
    pub severity: String,
    pub source_symbol: String,
    pub target_symbol: String,
    pub source_file: String,
    pub target_file: String,
    pub message: String,
    pub suggestion: String,
}

/// Architectural integrity scorecard and list of invariant violations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvariantReport {
    pub health_score: u32,
    pub total_edges_checked: usize,
    pub violations: Vec<InvariantViolation>,
    pub cycles_count: usize,
    pub layer_violations_count: usize,
}

impl InvariantReport {
    /// Formats the invariant audit report into a rich markdown scorecard
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 🏛️ Multi-File Dependency Invariant Report (Score: {}/100)\n\n",
            self.health_score
        );

        out.push_str(&format!(
            "📊 **Audit Scope:** {} symbol relationships evaluated across architectural layers.\n\n",
            self.total_edges_checked
        ));

        if self.violations.is_empty() {
            out.push_str("✅ **100% Invariant Compliance:** Zero layer inversion violations, call cycles, or structural anomalies detected.\n");
            return out;
        }

        out.push_str(&format!(
            "⚠️ **Found {} Invariant Violation(s):** ({} layer violations, {} circular cycles)\n\n",
            self.violations.len(),
            self.layer_violations_count,
            self.cycles_count
        ));

        for v in &self.violations {
            let badge = match v.severity.as_str() {
                "CRITICAL" => "🔴 [CRITICAL]",
                "HIGH" => "🟠 [HIGH]",
                "MEDIUM" => "🟡 [MEDIUM]",
                _ => "🔵 [LOW]",
            };

            out.push_str(&format!("### {} `{}`: {}\n", badge, v.rule_id, v.message));
            out.push_str(&format!(
                "- **Caller / Source:** `{}` in `{}`\n",
                v.source_symbol, v.source_file
            ));
            out.push_str(&format!(
                "- **Callee / Target:** `{}` in `{}`\n",
                v.target_symbol, v.target_file
            ));
            out.push_str(&format!("- **💡 Remediation:** {}\n\n", v.suggestion));
        }

        out
    }
}

pub struct InvariantChecker;

impl InvariantChecker {
    /// Evaluates architectural and call-graph invariants across the workspace
    pub fn check_workspace(
        workspace_root: &Path,
        provided_graph: Option<&CodeGraph>,
        target_file: Option<&str>,
    ) -> Result<InvariantReport> {
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
        let mut violations = Vec::new();
        let mut total_edges = 0;
        let mut layer_violations_count = 0;
        let mut score = 100_u32;

        // 1. Layer Dependency Invariants
        for edge_idx in petgraph_ref.edge_indices() {
            if let Some((source_idx, target_idx)) = petgraph_ref.edge_endpoints(edge_idx) {
                if let (Some(source_node), Some(target_node)) = (
                    petgraph_ref.node_weight(source_idx),
                    petgraph_ref.node_weight(target_idx),
                ) {
                    if source_node.kind == SymbolKind::File || target_node.kind == SymbolKind::File
                    {
                        continue;
                    }

                    total_edges += 1;

                    let source_rel = source_node
                        .file_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(&source_node.file_path)
                        .display()
                        .to_string();
                    let target_rel = target_node
                        .file_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(&target_node.file_path)
                        .display()
                        .to_string();

                    if let Some(tf) = target_file {
                        if source_rel != tf && target_rel != tf {
                            continue;
                        }
                    }

                    // Check same file
                    if source_node.file_path == target_node.file_path {
                        continue;
                    }

                    let source_layer = LayerClassifier::classify_path(&source_node.file_path);
                    let target_layer = LayerClassifier::classify_path(&target_node.file_path);

                    // Rule INV-001: Service -> UI violation (Core domain calling presentation)
                    if source_layer == ArchitecturalLayer::Service
                        && target_layer == ArchitecturalLayer::Ui
                    {
                        layer_violations_count += 1;
                        score = score.saturating_sub(15);
                        violations.push(InvariantViolation {
                            rule_id: "INV-001".to_string(),
                            severity: "CRITICAL".to_string(),
                            source_symbol: source_node.name.clone(),
                            target_symbol: target_node.name.clone(),
                            source_file: source_rel.clone(),
                            target_file: target_rel.clone(),
                            message: format!(
                                "Service layer symbol `{}` directly invokes UI symbol `{}`",
                                source_node.name, target_node.name
                            ),
                            suggestion: "Invert the dependency using an event channel, callback, or state model so domain logic remains decoupled from UI widgets.".to_string(),
                        });
                    }

                    // Rule INV-002: Data -> UI / Api violation (Persistence layer calling UI/API)
                    if source_layer == ArchitecturalLayer::Data
                        && (target_layer == ArchitecturalLayer::Ui
                            || target_layer == ArchitecturalLayer::Api)
                    {
                        layer_violations_count += 1;
                        score = score.saturating_sub(15);
                        violations.push(InvariantViolation {
                            rule_id: "INV-002".to_string(),
                            severity: "CRITICAL".to_string(),
                            source_symbol: source_node.name.clone(),
                            target_symbol: target_node.name.clone(),
                            source_file: source_rel.clone(),
                            target_file: target_rel.clone(),
                            message: format!(
                                "Data persistence symbol `{}` directly invokes {} symbol `{}`",
                                source_node.name,
                                target_layer.badge(),
                                target_node.name
                            ),
                            suggestion: "Data repositories should be leaf nodes in the dependency tree. Extract UI/API communications into a coordinator service.".to_string(),
                        });
                    }

                    // Rule INV-004: Utility -> Service/Data/UI violation (Utility calling higher-level tiers)
                    if source_layer == ArchitecturalLayer::Utility
                        && matches!(
                            target_layer,
                            ArchitecturalLayer::Ui
                                | ArchitecturalLayer::Service
                                | ArchitecturalLayer::Data
                        )
                    {
                        layer_violations_count += 1;
                        score = score.saturating_sub(10);
                        violations.push(InvariantViolation {
                            rule_id: "INV-004".to_string(),
                            severity: "HIGH".to_string(),
                            source_symbol: source_node.name.clone(),
                            target_symbol: target_node.name.clone(),
                            source_file: source_rel.clone(),
                            target_file: target_rel.clone(),
                            message: format!(
                                "Utility helper `{}` depends on higher architectural tier `{}`",
                                source_node.name,
                                target_layer.badge()
                            ),
                            suggestion: "Utilities must be pure, cross-cutting helpers without upward dependencies. Move domain logic out of utility modules.".to_string(),
                        });
                    }
                }
            }
        }

        // 2. Call-Graph Mutual Cycle Invariants (Tarjan SCC on non-file nodes)
        let sccs = tarjan_scc(petgraph_ref);
        let mut cycles_count = 0;

        for scc in sccs {
            if scc.len() > 1 {
                let scc_nodes: Vec<&SymbolNode> = scc
                    .iter()
                    .filter_map(|&idx| petgraph_ref.node_weight(idx))
                    .filter(|n| n.kind != SymbolKind::File)
                    .collect();

                if scc_nodes.len() > 1 {
                    let files_in_scc: HashSet<_> = scc_nodes.iter().map(|n| &n.file_path).collect();
                    if files_in_scc.len() > 1 {
                        cycles_count += 1;
                        score = score.saturating_sub(12);

                        let names: Vec<String> = scc_nodes
                            .iter()
                            .map(|n| {
                                format!(
                                    "`{}` ({})",
                                    n.name,
                                    n.file_path
                                        .strip_prefix(workspace_root)
                                        .unwrap_or(&n.file_path)
                                        .display()
                                )
                            })
                            .collect();

                        let first = scc_nodes[0];
                        let second = scc_nodes[1];

                        violations.push(InvariantViolation {
                            rule_id: "INV-003".to_string(),
                            severity: "HIGH".to_string(),
                            source_symbol: first.name.clone(),
                            target_symbol: second.name.clone(),
                            source_file: first
                                .file_path
                                .strip_prefix(workspace_root)
                                .unwrap_or(&first.file_path)
                                .display()
                                .to_string(),
                            target_file: second
                                .file_path
                                .strip_prefix(workspace_root)
                                .unwrap_or(&second.file_path)
                                .display()
                                .to_string(),
                            message: format!(
                                "Cross-file mutual recursion / call cycle detected among {} symbols",
                                scc_nodes.len()
                            ),
                            suggestion: format!(
                                "Break the circular dependency cycle between {}. Extract common abstractions into an intermediary module.",
                                names.join(" ⇄ ")
                            ),
                        });
                    }
                }
            }
        }

        Ok(InvariantReport {
            health_score: score,
            total_edges_checked: total_edges,
            violations,
            cycles_count,
            layer_violations_count,
        })
    }
}
