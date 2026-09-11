use crate::context::graph::{CodeGraph, EdgeKind, SymbolKind, SymbolNode};
use crate::error::{ContextError, Result};
use petgraph::graph::NodeIndex;
use std::collections::HashSet;
use std::path::Path;

/// Display mode for call-graph visualizer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizeMode {
    Box,
    Upstream,
    Downstream,
    Both,
}

impl VisualizeMode {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "box" | "card" | "summary" => Self::Box,
            "upstream" | "callers" | "in" => Self::Upstream,
            "downstream" | "callees" | "out" => Self::Downstream,
            _ => Self::Both,
        }
    }
}

pub struct GraphVisualizer;

impl GraphVisualizer {
    /// Visualizes call-graph relationships, blast radius, and architectural layers for a symbol or file
    pub fn render(
        workspace_root: &Path,
        graph: &CodeGraph,
        target_query: &str,
        mode: VisualizeMode,
        max_depth: usize,
    ) -> Result<String> {
        let petgraph_ref = graph.graph();

        // 1. Locate target node
        let target_node_idx = Self::resolve_node(graph, target_query, workspace_root)?;
        let target_node = petgraph_ref.node_weight(target_node_idx).ok_or_else(|| {
            ContextError::Graph(format!("Node weight missing for target '{}'", target_query))
        })?;

        // 2. Compute Blast Radius and Architectural Layer
        let blast_report = graph.get_blast_radius(target_query, workspace_root)?;
        let layer = crate::context::layers::LayerClassifier::classify_path(&target_node.file_path);

        let pr_list = graph.compute_symbol_pagerank(&[]);
        let pagerank_score = pr_list
            .iter()
            .find(|(n, _)| n.name == target_node.name)
            .map(|(_, score)| *score)
            .unwrap_or(0.0);

        let in_degree = petgraph_ref
            .neighbors_directed(target_node_idx, petgraph::Direction::Incoming)
            .count();
        let out_degree = petgraph_ref
            .neighbors_directed(target_node_idx, petgraph::Direction::Outgoing)
            .count();

        let rel_file = target_node
            .file_path
            .strip_prefix(workspace_root)
            .unwrap_or(&target_node.file_path)
            .display()
            .to_string();

        let mut out = String::new();

        // 3. Render Architectural Box Card
        let kind_str = match target_node.kind {
            SymbolKind::File => "File".to_string(),
            _ => format!("Symbol: {}", target_node.kind.as_str()),
        };

        let title_line = format!("[{}] `{}` in `{}`", kind_str, target_node.name, rel_file);
        let border_len = title_line.len().max(68) + 4;
        let border_top = format!("┌{}┐", "─".repeat(border_len.saturating_sub(2)));
        let border_mid = format!("├{}┤", "─".repeat(border_len.saturating_sub(2)));
        let border_bot = format!("└{}┘", "─".repeat(border_len.saturating_sub(2)));

        out.push_str("```text\n");
        out.push_str(&format!("{}\n", border_top));
        out.push_str(&format!(
            "│ {:<width$} │\n",
            title_line,
            width = border_len.saturating_sub(4)
        ));
        out.push_str(&format!("{}\n", border_mid));

        let layer_pr_line = format!(
            "Layer: {:<20} │ PageRank Centrality: {:.4}",
            layer.badge(),
            pagerank_score
        );
        out.push_str(&format!(
            "│ {:<width$} │\n",
            layer_pr_line,
            width = border_len.saturating_sub(4)
        ));

        let deg_line = format!(
            "Incoming Callers (In): {:<5} │ Outgoing Callees (Out): {:<5}",
            in_degree, out_degree
        );
        out.push_str(&format!(
            "│ {:<width$} │\n",
            deg_line,
            width = border_len.saturating_sub(4)
        ));

        let test_status = if blast_report.test_coverage.is_empty() {
            "⚠️ Untested"
        } else {
            "✓ Direct Test"
        };
        let risk_line = format!(
            "Risk Assessment: {:<11} │ Test Reachability: {}",
            format!(
                "{} ({:.2})",
                blast_report.risk_level, blast_report.composite_risk_score
            ),
            test_status
        );
        out.push_str(&format!(
            "│ {:<width$} │\n",
            risk_line,
            width = border_len.saturating_sub(4)
        ));
        out.push_str(&format!("{}\n", border_bot));

        // 4. Render Upstream Callers Tree
        if matches!(mode, VisualizeMode::Upstream | VisualizeMode::Both) {
            out.push_str("\n⟵ Upstream Callers (Who depends on this?):\n");
            let mut visited = HashSet::new();
            visited.insert(target_node_idx);
            let mut rendered_any = false;

            Self::render_tree(
                petgraph_ref,
                target_node_idx,
                petgraph::Direction::Incoming,
                workspace_root,
                "",
                1,
                max_depth,
                &mut visited,
                &mut out,
                &mut rendered_any,
            );

            if !rendered_any {
                out.push_str("└── (Isolated leaf module — zero incoming callers)\n");
            }
        }

        // 5. Render Downstream Dependencies Tree
        if matches!(mode, VisualizeMode::Downstream | VisualizeMode::Both) {
            out.push_str("\n⟶ Downstream Dependencies (What does this invoke?):\n");
            let mut visited = HashSet::new();
            visited.insert(target_node_idx);
            let mut rendered_any = false;

            Self::render_tree(
                petgraph_ref,
                target_node_idx,
                petgraph::Direction::Outgoing,
                workspace_root,
                "",
                1,
                max_depth,
                &mut visited,
                &mut out,
                &mut rendered_any,
            );

            if !rendered_any {
                out.push_str("└── (Self-contained unit — zero external outgoing calls)\n");
            }
        }

        out.push_str("```\n");
        Ok(out)
    }

    fn resolve_node(graph: &CodeGraph, query: &str, workspace_root: &Path) -> Result<NodeIndex> {
        let petgraph_ref = graph.graph();

        // Check if query matches symbol name exactly
        for node_idx in petgraph_ref.node_indices() {
            if let Some(node) = petgraph_ref.node_weight(node_idx) {
                if node.name == query || node.qualified_name == query {
                    return Ok(node_idx);
                }
            }
        }

        // Check if query matches file path
        let candidate = if Path::new(query).is_absolute() {
            query.to_string()
        } else {
            workspace_root.join(query).to_string_lossy().to_string()
        };

        for node_idx in petgraph_ref.node_indices() {
            if let Some(node) = petgraph_ref.node_weight(node_idx) {
                let file_str = node.file_path.to_string_lossy();
                if file_str == candidate
                    || file_str.ends_with(query)
                    || node.name.eq_ignore_ascii_case(query)
                {
                    return Ok(node_idx);
                }
            }
        }

        Err(ContextError::Graph(format!(
            "Symbol or file '{}' not found in indexed code graph",
            query
        ))
        .into())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_tree(
        graph: &petgraph::graph::DiGraph<SymbolNode, EdgeKind>,
        curr_idx: NodeIndex,
        dir: petgraph::Direction,
        workspace_root: &Path,
        prefix: &str,
        depth: usize,
        max_depth: usize,
        visited: &mut HashSet<NodeIndex>,
        out: &mut String,
        rendered_any: &mut bool,
    ) {
        if depth > max_depth {
            return;
        }

        let neighbors: Vec<NodeIndex> = graph
            .neighbors_directed(curr_idx, dir)
            .filter(|n| {
                graph
                    .node_weight(*n)
                    .map(|w| w.kind != SymbolKind::File)
                    .unwrap_or(false)
            })
            .collect();

        let count = neighbors.len();
        for (i, neighbor_idx) in neighbors.into_iter().enumerate() {
            let is_last = i == count - 1;
            let branch = if is_last { "└── " } else { "├── " };

            if let Some(node) = graph.node_weight(neighbor_idx) {
                *rendered_any = true;
                let rel = node
                    .file_path
                    .strip_prefix(workspace_root)
                    .unwrap_or(&node.file_path)
                    .display()
                    .to_string();

                let symbol_desc = format!("[{}] `{}` in `{}`", node.kind.as_str(), node.name, rel);
                let is_cyclic = !visited.insert(neighbor_idx);

                if is_cyclic {
                    out.push_str(&format!(
                        "{}{}{} (⟲ mutual cycle)\n",
                        prefix, branch, symbol_desc
                    ));
                } else {
                    out.push_str(&format!("{}{}{}\n", prefix, branch, symbol_desc));
                    let next_prefix =
                        format!("{}{}", prefix, if is_last { "    " } else { "│   " });
                    Self::render_tree(
                        graph,
                        neighbor_idx,
                        dir,
                        workspace_root,
                        &next_prefix,
                        depth + 1,
                        max_depth,
                        visited,
                        out,
                        rendered_any,
                    );
                }
            }
        }
    }
}
