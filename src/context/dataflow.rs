use crate::context::graph::{CodeGraph, EdgeKind, SymbolKind};
use crate::error::Result;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::path::Path;

/// Individual step along a function-to-function or symbol-to-symbol dataflow trace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataflowStep {
    pub symbol_name: String,
    pub file_path: String,
    pub line: usize,
    pub step_type: String, // "Source" | "Transform" | "Sink"
    pub signature: String,
}

/// A complete path of dataflow propagation from origin to terminal sink
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataflowTrace {
    pub source_symbol: String,
    pub sink_symbol: String,
    pub steps: Vec<DataflowStep>,
    pub is_tainted: bool,
    pub taint_warning: Option<String>,
}

/// Comprehensive analysis report of inter-procedural type-flow and reachability
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataflowReport {
    pub target_symbol: String,
    pub direction: String,
    pub traces: Vec<DataflowTrace>,
}

impl DataflowReport {
    /// Formats the dataflow traces into a rich markdown report with visual Mermaid diagrams
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 🌊 Type-Flow & Dataflow Reachability Report\n\n\
            - **Target Symbol:** `{}`\n\
            - **Analysis Direction:** {}\n\
            - **Identified Paths:** {}\n\n",
            self.target_symbol,
            if self.direction == "backward" {
                "⬅️ Backward (Program Slicing / Origin Tracing)"
            } else {
                "➡️ Forward (Propagation / Sink Reachability)"
            },
            self.traces.len()
        );

        if self.traces.is_empty() {
            out.push_str("*(No cross-symbol dataflow or call propagation paths found)*\n");
            return out;
        }

        // Taint Warning Alerts
        let tainted_traces: Vec<&DataflowTrace> =
            self.traces.iter().filter(|t| t.is_tainted).collect();
        if !tainted_traces.is_empty() {
            out.push_str("> ⚠️ **Potential Tainted Dataflow / Sensitive Sink Detected**\n");
            for trace in &tainted_traces {
                if let Some(warning) = &trace.taint_warning {
                    out.push_str(&format!(
                        "> - Path `{}` ➔ `{}`: {}\n",
                        trace.source_symbol, trace.sink_symbol, warning
                    ));
                }
            }
            out.push('\n');
        }

        // Traces Breakdown Table
        out.push_str("### 📊 Dataflow Paths\n\n");
        for (i, trace) in self.traces.iter().enumerate() {
            out.push_str(&format!(
                "#### Path #{}: `{}` ➔ `{}` {}\n\n",
                i + 1,
                trace.source_symbol,
                trace.sink_symbol,
                if trace.is_tainted {
                    "🚨 [TAINTED]"
                } else {
                    "✔ [SAFE]"
                }
            ));

            out.push_str("| Step | Symbol | File | Line | Type | Signature |\n");
            out.push_str("| :---: | :--- | :--- | :---: | :---: | :--- |\n");
            for (step_idx, step) in trace.steps.iter().enumerate() {
                out.push_str(&format!(
                    "| {} | **`{}`** | `{}` | {} | {} | `{}` |\n",
                    step_idx + 1,
                    step.symbol_name,
                    step.file_path,
                    step.line,
                    step.step_type,
                    if step.signature.is_empty() {
                        "—"
                    } else {
                        &step.signature
                    }
                ));
            }
            out.push('\n');
        }

        // Mermaid Sequence Flowchart
        out.push_str("### 🗺️ Dataflow Sequence Diagram\n\n");
        out.push_str("```mermaid\nflowchart LR\n");
        for trace in &self.traces {
            for win in trace.steps.windows(2) {
                let from_id = win[0].symbol_name.replace([':', '.', '-', ' '], "_");
                let to_id = win[1].symbol_name.replace([':', '.', '-', ' '], "_");
                out.push_str(&format!(
                    "    {}[\"{}\"] --> {}[\"{}\"]\n",
                    from_id, win[0].symbol_name, to_id, win[1].symbol_name
                ));
            }
        }
        out.push_str("```\n\n");

        out
    }
}

pub struct DataflowAnalyzer;

impl DataflowAnalyzer {
    /// Traces static forward or backward dataflow reachability from a target symbol
    pub fn trace(
        workspace_root: &Path,
        target_symbol: &str,
        direction: &str,
        max_depth: usize,
        taint_check: bool,
    ) -> Result<DataflowReport> {
        let mut graph = CodeGraph::new();
        graph.build_graph(workspace_root)?;

        let is_backward = direction.eq_ignore_ascii_case("backward");

        // Find matching start nodes
        let mut start_nodes = Vec::new();
        for node_idx in graph.graph().node_indices() {
            if let Some(n) = graph.graph().node_weight(node_idx) {
                if n.kind != SymbolKind::File
                    && (n.name == target_symbol || n.qualified_name.contains(target_symbol))
                {
                    start_nodes.push(node_idx);
                }
            }
        }

        let mut traces = Vec::new();

        for &start_idx in &start_nodes {
            let mut visited = HashSet::new();
            let mut queue = VecDeque::new();
            queue.push_back((start_idx, vec![start_idx]));

            while let Some((curr_idx, path)) = queue.pop_front() {
                if path.len() > max_depth {
                    continue;
                }
                visited.insert(curr_idx);

                let pet_dir = if is_backward {
                    Direction::Incoming
                } else {
                    Direction::Outgoing
                };

                let mut expanded = false;
                for edge in graph.graph().edges_directed(curr_idx, pet_dir) {
                    let next_idx = if is_backward {
                        edge.source()
                    } else {
                        edge.target()
                    };

                    let edge_kind = edge.weight();
                    if *edge_kind == EdgeKind::Calls || *edge_kind == EdgeKind::References {
                        if let Some(next_node) = graph.graph().node_weight(next_idx) {
                            if next_node.kind != SymbolKind::File && !path.contains(&next_idx) {
                                let mut next_path = path.clone();
                                next_path.push(next_idx);
                                queue.push_back((next_idx, next_path));
                                expanded = true;
                            }
                        }
                    }
                }

                if !expanded && path.len() > 1 {
                    // Reconstruct trace
                    let mut steps = Vec::new();
                    for (i, &p_idx) in path.iter().enumerate() {
                        if let Some(node) = graph.graph().node_weight(p_idx) {
                            let step_type = if i == 0 {
                                if is_backward {
                                    "Sink"
                                } else {
                                    "Source"
                                }
                            } else if i == path.len() - 1 {
                                if is_backward {
                                    "Source"
                                } else {
                                    "Sink"
                                }
                            } else {
                                "Transform"
                            };

                            steps.push(DataflowStep {
                                symbol_name: node.name.clone(),
                                file_path: node.file_path.display().to_string(),
                                line: node.start_line,
                                step_type: step_type.to_string(),
                                signature: node.signature.clone(),
                            });
                        }
                    }

                    if is_backward {
                        steps.reverse();
                    }

                    let source_symbol = steps
                        .first()
                        .map(|s| s.symbol_name.clone())
                        .unwrap_or_default();
                    let sink_symbol = steps
                        .last()
                        .map(|s| s.symbol_name.clone())
                        .unwrap_or_default();

                    let (is_tainted, taint_warning) = if taint_check {
                        Self::check_taint(&steps)
                    } else {
                        (false, None)
                    };

                    traces.push(DataflowTrace {
                        source_symbol,
                        sink_symbol,
                        steps,
                        is_tainted,
                        taint_warning,
                    });
                }
            }
        }

        // Fallback: If no multi-node paths found, record single target node
        if traces.is_empty() {
            for &idx in &start_nodes {
                if let Some(node) = graph.graph().node_weight(idx) {
                    if node.kind != SymbolKind::File {
                        let step = DataflowStep {
                            symbol_name: node.name.clone(),
                            file_path: node.file_path.display().to_string(),
                            line: node.start_line,
                            step_type: "Target Symbol".to_string(),
                            signature: node.signature.clone(),
                        };
                        traces.push(DataflowTrace {
                            source_symbol: node.name.clone(),
                            sink_symbol: node.name.clone(),
                            steps: vec![step],
                            is_tainted: false,
                            taint_warning: None,
                        });
                    }
                }
            }
        }

        Ok(DataflowReport {
            target_symbol: target_symbol.to_string(),
            direction: if is_backward {
                "backward".to_string()
            } else {
                "forward".to_string()
            },
            traces,
        })
    }

    fn check_taint(steps: &[DataflowStep]) -> (bool, Option<String>) {
        let sensitive_sink_patterns = [
            "exec", "command", "spawn", "system", "eval", "write", "remove", "delete", "unlink",
            "query", "raw_sql",
        ];

        if let Some(sink) = steps.last() {
            let lower_name = sink.symbol_name.to_lowercase();
            for pattern in &sensitive_sink_patterns {
                if lower_name.contains(pattern) {
                    return (
                        true,
                        Some(format!(
                            "Untrusted data from `{}` reaches sensitive sink `{}` matching pattern '{}'",
                            steps.first().map(|s| s.symbol_name.as_str()).unwrap_or("source"),
                            sink.symbol_name,
                            pattern
                        )),
                    );
                }
            }
        }

        (false, None)
    }
}
