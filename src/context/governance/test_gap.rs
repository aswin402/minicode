use crate::context::graph::{CodeGraph, SymbolKind};
use crate::error::Result;
use petgraph::graph::NodeIndex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;

/// Categorization of automated test coverage reachability
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TestCoverageKind {
    /// Function or type is invoked directly within a test function
    DirectlyTested { test_symbols: Vec<String> },
    /// Function or type is reached indirectly through dependencies of a tested module
    TransitivelyCovered {
        depth: usize,
        via_tests: Vec<String>,
    },
    /// Function or type has zero reachability paths from any test suite
    Untested,
}

impl TestCoverageKind {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::DirectlyTested { .. } => "✓ Direct Test",
            Self::TransitivelyCovered { .. } => "🔄 Transitive Test",
            Self::Untested => "⚠️ Untested",
        }
    }
}

/// A specific symbol analyzed for test coverage gaps and architectural risk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolTestGap {
    pub symbol_name: String,
    pub qualified_name: String,
    pub file_path: String,
    pub kind: String,
    pub is_public: bool,
    pub start_line: usize,
    pub end_line: usize,
    pub coverage_kind: TestCoverageKind,
    pub composite_risk: f64,
    pub caller_count: usize,
    pub pagerank: f64,
    pub suggested_test_file: String,
}

/// Comprehensive report detailing codebase-wide or file-scoped test gaps
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestGapReport {
    pub total_symbols: usize,
    pub directly_tested_count: usize,
    pub transitively_covered_count: usize,
    pub untested_count: usize,
    pub coverage_percentage: f64,
    pub high_risk_gaps: Vec<SymbolTestGap>,
    pub gaps: Vec<SymbolTestGap>,
}

pub struct TestGapAnalyzer;

impl TestGapAnalyzer {
    /// Analyzes the workspace code graph for test reachability gaps and risk scores
    pub fn analyze(
        workspace_root: &Path,
        graph: &CodeGraph,
        target_file: Option<&str>,
        untested_only: bool,
        min_risk: Option<f64>,
    ) -> Result<TestGapReport> {
        let petgraph_ref = graph.graph();

        // 1. Identify all test entrypoint nodes in graph
        let mut test_nodes: Vec<NodeIndex> = Vec::new();
        for node_idx in petgraph_ref.node_indices() {
            if let Some(node) = petgraph_ref.node_weight(node_idx) {
                let path_str = node.file_path.to_string_lossy().to_lowercase();
                let name_lower = node.name.to_lowercase();
                let is_test_file = path_str.contains("test") || path_str.contains("spec");
                let is_test_fn = name_lower.starts_with("test_")
                    || name_lower.ends_with("_test")
                    || name_lower == "test";

                if is_test_file || is_test_fn {
                    test_nodes.push(node_idx);
                }
            }
        }

        // 2. Multi-source BFS from all test nodes to trace reachability
        // Map: NodeIndex -> (min_depth, Vec<test_symbol_name>)
        let mut reachability: HashMap<NodeIndex, (usize, Vec<String>)> = HashMap::new();
        let mut queue: VecDeque<(NodeIndex, usize, String)> = VecDeque::new();

        for &t_idx in &test_nodes {
            if let Some(t_node) = petgraph_ref.node_weight(t_idx) {
                let test_name = t_node.qualified_name.clone();
                queue.push_back((t_idx, 0, test_name));
            }
        }

        while let Some((curr, depth, origin_test)) = queue.pop_front() {
            for neighbor in petgraph_ref.neighbors_directed(curr, petgraph::Direction::Outgoing) {
                if let Some(neighbor_node) = petgraph_ref.node_weight(neighbor) {
                    if neighbor_node.kind == SymbolKind::File {
                        continue;
                    }

                    let next_depth = depth + 1;
                    if let Some(entry) = reachability.get_mut(&neighbor) {
                        if next_depth < entry.0 {
                            entry.0 = next_depth;
                        }
                        if !entry.1.contains(&origin_test) && entry.1.len() < 4 {
                            entry.1.push(origin_test.clone());
                        }
                    } else {
                        reachability.insert(neighbor, (next_depth, vec![origin_test.clone()]));
                        if next_depth <= 4 {
                            queue.push_back((neighbor, next_depth, origin_test.clone()));
                        }
                    }
                }
            }
        }

        // 3. Compute PageRank
        let pr_list = graph.compute_symbol_pagerank(&[]);
        let mut pagerank_map: HashMap<String, f64> = HashMap::new();
        for (node, score) in pr_list {
            pagerank_map.insert(node.name.clone(), score);
            pagerank_map.insert(node.qualified_name.clone(), score);
        }

        // 4. Evaluate each non-test symbol in target scope
        let mut gaps = Vec::new();
        let mut directly_tested_count = 0;
        let mut transitively_covered_count = 0;
        let mut untested_count = 0;

        for node_idx in petgraph_ref.node_indices() {
            let node = match petgraph_ref.node_weight(node_idx) {
                Some(n) => n,
                None => continue,
            };

            if node.kind == SymbolKind::File || test_nodes.contains(&node_idx) {
                continue;
            }

            let rel_file = node
                .file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&node.file_path)
                .display()
                .to_string();

            if let Some(target) = target_file {
                if !rel_file.contains(target) && !node.file_path.to_string_lossy().contains(target)
                {
                    continue;
                }
            }

            let incoming_callers = petgraph_ref
                .neighbors_directed(node_idx, petgraph::Direction::Incoming)
                .count();

            let pr_score = pagerank_map.get(&node.name).copied().unwrap_or(0.0);

            // Determine coverage
            let coverage_kind = if let Some((depth, tests)) = reachability.get(&node_idx) {
                if *depth == 1 {
                    directly_tested_count += 1;
                    TestCoverageKind::DirectlyTested {
                        test_symbols: tests.clone(),
                    }
                } else {
                    transitively_covered_count += 1;
                    TestCoverageKind::TransitivelyCovered {
                        depth: *depth,
                        via_tests: tests.clone(),
                    }
                }
            } else {
                untested_count += 1;
                TestCoverageKind::Untested
            };

            if untested_only && !matches!(coverage_kind, TestCoverageKind::Untested) {
                continue;
            }

            // Calculate Composite Risk Formula
            let callers_risk = (incoming_callers as f64 / 8.0).min(1.0);
            let pr_risk = (pr_score * 20.0).min(1.0);
            let gap_risk = match &coverage_kind {
                TestCoverageKind::DirectlyTested { .. } => 0.0,
                TestCoverageKind::TransitivelyCovered { depth, .. } => {
                    (0.25 * (*depth as f64)).min(0.7)
                }
                TestCoverageKind::Untested => 1.0,
            };
            let pub_bonus = if node.signature.starts_with("pub ") {
                0.2
            } else {
                0.0
            };

            let composite_risk =
                (0.35 * callers_risk + 0.25 * pr_risk + 0.30 * gap_risk + 0.10 * pub_bonus)
                    .min(1.0);

            if let Some(min_r) = min_risk {
                if composite_risk < min_r {
                    continue;
                }
            }

            let suggested_test_file = format!(
                "tests/integration_{}.rs",
                Path::new(&rel_file)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
            );

            gaps.push(SymbolTestGap {
                symbol_name: node.name.clone(),
                qualified_name: node.qualified_name.clone(),
                file_path: rel_file,
                kind: node.kind.as_str().to_string(),
                is_public: node.signature.starts_with("pub "),
                start_line: node.start_line,
                end_line: node.end_line,
                coverage_kind,
                composite_risk,
                caller_count: incoming_callers,
                pagerank: pr_score,
                suggested_test_file,
            });
        }

        gaps.sort_by(|a, b| {
            b.composite_risk
                .partial_cmp(&a.composite_risk)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let high_risk_gaps: Vec<SymbolTestGap> = gaps
            .iter()
            .filter(|g| {
                g.composite_risk >= 0.50 && matches!(g.coverage_kind, TestCoverageKind::Untested)
            })
            .cloned()
            .collect();

        let total = directly_tested_count + transitively_covered_count + untested_count;
        let coverage_percentage = if total > 0 {
            ((directly_tested_count + transitively_covered_count) as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        Ok(TestGapReport {
            total_symbols: total,
            directly_tested_count,
            transitively_covered_count,
            untested_count,
            coverage_percentage,
            high_risk_gaps,
            gaps,
        })
    }

    /// Formats the test gap report into structured markdown
    pub fn format_markdown(report: &TestGapReport, target_hint: Option<&str>) -> String {
        let scope_str = target_hint
            .map(|t| format!(" for `{}`", t))
            .unwrap_or_default();

        let mut out = format!(
            "### 🧪 Test Gap & Reachability Analysis{}\n\n\
            • **Total Analyzed Symbols**: {}\n\
            • **Directly Tested**: {} symbols\n\
            • **Transitively Covered**: {} symbols\n\
            • **Untested Symbols**: {} symbols\n\
            • **Graph Reachability**: `{:.1}%`\n\n",
            scope_str,
            report.total_symbols,
            report.directly_tested_count,
            report.transitively_covered_count,
            report.untested_count,
            report.coverage_percentage
        );

        if !report.high_risk_gaps.is_empty() {
            out.push_str("#### ⚠️ High-Risk Untested Symbols (Immediate Priority)\n");
            for h in report.high_risk_gaps.iter().take(6) {
                let vis = if h.is_public { "pub " } else { "" };
                out.push_str(&format!(
                    "- `{}` **`{}{}`** in `{}` (lines {}-{}):\n  • *Risk:* `{:.3}` | *Callers:* {} | *PageRank:* `{:.4}`\n  • *Suggested Test Suite:* `{}`\n",
                    h.kind, vis, h.symbol_name, h.file_path, h.start_line, h.end_line,
                    h.composite_risk, h.caller_count, h.pagerank, h.suggested_test_file
                ));
            }
            out.push('\n');
        }

        if !report.gaps.is_empty() {
            out.push_str("#### 📋 Symbol Test Coverage Breakdown\n");
            for g in report.gaps.iter().take(12) {
                let vis = if g.is_public { "pub " } else { "" };
                out.push_str(&format!(
                    "- `{}` **`{}{}`** (`{}`) — {} (Risk: `{:.2}`)\n",
                    g.kind,
                    vis,
                    g.symbol_name,
                    g.file_path,
                    g.coverage_kind.badge(),
                    g.composite_risk
                ));
            }
            if report.gaps.len() > 12 {
                out.push_str(&format!(
                    "\n*... and {} more symbols evaluated.*\n",
                    report.gaps.len() - 12
                ));
            }
        }

        out
    }
}
