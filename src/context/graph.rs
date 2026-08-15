#![allow(dead_code)]

use crate::context::repomap::RepoMapExtractor;
use crate::error::Result;
use ignore::WalkBuilder;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct CodeGraph {
    graph: DiGraph<PathBuf, ()>,
    node_indices: HashMap<PathBuf, NodeIndex>,
    extractor: RepoMapExtractor,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
            extractor: RepoMapExtractor::new(),
        }
    }

    /// Indexes the entire workspace, constructing a dependency graph of files and extracting symbols.
    pub fn build_graph(&mut self, workspace_root: &Path) -> Result<()> {
        let walker = WalkBuilder::new(workspace_root)
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .build();

        let mut source_files = Vec::new();
        for result in walker.flatten() {
            if result.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                let path = result.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if matches!(ext, "rs" | "py" | "js" | "ts" | "jsx" | "tsx") {
                        source_files.push(path.to_path_buf());
                    }
                }
            }
        }

        // Add nodes
        for file in &source_files {
            if !self.node_indices.contains_key(file) {
                let idx = self.graph.add_node(file.clone());
                self.node_indices.insert(file.clone(), idx);
            }
        }

        // Map symbol name -> defining file
        let mut symbol_to_file: HashMap<String, PathBuf> = HashMap::new();

        for file in &source_files {
            match self.extractor.extract_file_symbols(file) {
                Ok(symbols) => {
                    for sym in symbols {
                        if sym.name.len() > 2 {
                            symbol_to_file.insert(sym.name, file.clone());
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!(path = %file.display(), error = %e, "Could not extract AST symbols from file");
                }
            }
        }

        // Build edges based on cross-file symbol references
        for file in &source_files {
            if let Ok(content) = std::fs::read_to_string(file) {
                let from_idx = match self.node_indices.get(file) {
                    Some(&idx) => idx,
                    None => continue,
                };

                for (sym_name, target_file) in &symbol_to_file {
                    if target_file != file && content.contains(sym_name) {
                        if let Some(&to_idx) = self.node_indices.get(target_file) {
                            if !self.graph.contains_edge(from_idx, to_idx) {
                                self.graph.add_edge(from_idx, to_idx, ());
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Computes PageRank scores for all indexed files.
    pub fn compute_pagerank(&self, active_files: &[PathBuf]) -> Vec<(PathBuf, f64)> {
        let node_count = self.graph.node_count();
        if node_count == 0 {
            return Vec::new();
        }

        let mut scores: HashMap<NodeIndex, f64> = HashMap::new();
        let initial_score = 1.0 / node_count as f64;

        for node_idx in self.graph.node_indices() {
            scores.insert(node_idx, initial_score);
        }

        let damping = crate::constants::PAGERANK_DAMPING;
        let iterations = crate::constants::PAGERANK_ITERATIONS;

        for _ in 0..iterations {
            let mut next_scores = HashMap::new();
            for node in self.graph.node_indices() {
                let mut sum_in = 0.0;
                for neighbor in self
                    .graph
                    .neighbors_directed(node, petgraph::Direction::Incoming)
                {
                    let out_degree = self.graph.neighbors(neighbor).count().max(1);
                    sum_in += scores.get(&neighbor).unwrap_or(&0.0) / out_degree as f64;
                }

                // Personalization boost for active files in conversation
                let personalization_bias = if active_files
                    .iter()
                    .any(|af| self.node_indices.get(af) == Some(&node))
                {
                    crate::constants::PAGERANK_PERSONALIZATION_BIAS
                } else {
                    0.0
                };

                let score =
                    (1.0 - damping) / node_count as f64 + damping * sum_in + personalization_bias;
                next_scores.insert(node, score);
            }
            scores = next_scores;
        }

        let mut results: Vec<(PathBuf, f64)> = self
            .node_indices
            .iter()
            .map(|(path, idx)| (path.clone(), *scores.get(idx).unwrap_or(&0.0)))
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Formats the top-ranked repository symbols into a compact Repo-Map skeleton outline.
    pub fn format_repomap(
        &mut self,
        workspace_root: &Path,
        active_files: &[PathBuf],
        max_symbols: usize,
    ) -> String {
        let ranked = self.compute_pagerank(active_files);
        let mut map_lines = Vec::new();
        let mut symbol_count = 0;

        for (path, _) in ranked {
            if symbol_count >= max_symbols {
                break;
            }

            if let Ok(symbols) = self.extractor.extract_file_symbols(&path) {
                if symbols.is_empty() {
                    continue;
                }

                let rel_path = path.strip_prefix(workspace_root).unwrap_or(&path).display();

                map_lines.push(format!("### {}", rel_path));
                for sym in symbols {
                    if symbol_count >= max_symbols {
                        break;
                    }
                    map_lines.push(format!(
                        "  line {}: {} ({})",
                        sym.line_number, sym.name, sym.kind
                    ));
                    symbol_count += 1;
                }
                map_lines.push(String::new());
            }
        }

        map_lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_graph_pagerank() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_graph_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_a = temp_dir.join("a.rs");
        let file_b = temp_dir.join("b.rs");
        std::fs::write(&file_a, "pub fn helper() {}").unwrap();
        std::fs::write(&file_b, "pub fn main() {}").unwrap();

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        let ranked = graph.compute_pagerank(&[file_a.clone()]);
        assert!(!ranked.is_empty());

        let formatted = graph.format_repomap(&temp_dir, &[file_a], 50);
        assert!(formatted.contains("### a.rs") || formatted.contains("### b.rs"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
