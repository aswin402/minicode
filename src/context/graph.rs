use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::error::{ContextError, Result};
use ignore::WalkBuilder;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Architectural impact and risk analysis report for a symbol or file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadiusReport {
    pub target: String,
    pub target_type: String, // "file" | "symbol"
    pub file_path: String,
    pub direct_dependents: Vec<String>,
    pub transitive_dependents: Vec<String>,
    pub test_coverage: Vec<String>,
    pub in_cyclic_dependency: bool,
    pub cycle_members: Vec<String>,
    pub risk_level: String, // "LOW" | "MEDIUM" | "HIGH" | "CRITICAL"
    pub summary: String,
}

pub struct CodeGraph {
    graph: DiGraph<PathBuf, ()>,
    node_indices: HashMap<PathBuf, NodeIndex>,
    symbol_to_file: HashMap<String, Vec<(PathBuf, SymbolDef)>>,
    file_to_symbols: HashMap<PathBuf, Vec<SymbolDef>>,
    extractor: RepoMapExtractor,
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
            symbol_to_file: HashMap::new(),
            file_to_symbols: HashMap::new(),
            extractor: RepoMapExtractor::new(),
        }
    }

    /// Indexes the entire workspace, constructing a dependency graph of files and extracting symbols.
    pub fn build_graph(&mut self, workspace_root: &Path) -> Result<()> {
        self.graph.clear();
        self.node_indices.clear();
        self.symbol_to_file.clear();
        self.file_to_symbols.clear();

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
                    if crate::constants::SUPPORTED_LANG_EXTENSIONS.contains(&ext) {
                        source_files.push(path.to_path_buf());
                    }
                }
            }
        }

        // 1. Add nodes
        for file in &source_files {
            if !self.node_indices.contains_key(file) {
                let idx = self.graph.add_node(file.clone());
                self.node_indices.insert(file.clone(), idx);
            }
        }

        let mut file_contents = HashMap::new();

        // 2. Extract AST symbols and map symbol -> defining files
        for file in &source_files {
            if let Ok(content) = std::fs::read_to_string(file) {
                file_contents.insert(file.clone(), content);
            }
            match self.extractor.extract_file_symbols(file) {
                Ok(symbols) => {
                    for sym in &symbols {
                        if sym.name.len() > 2 && sym.kind != "import" {
                            self.symbol_to_file
                                .entry(sym.name.clone())
                                .or_default()
                                .push((file.clone(), sym.clone()));
                        }
                    }
                    self.file_to_symbols.insert(file.clone(), symbols);
                }
                Err(e) => {
                    tracing::debug!(path = %file.display(), error = %e, "Could not extract AST symbols from file");
                }
            }
        }

        // 3. Build directed dependency edges based on cross-file symbol references
        for file in &source_files {
            let from_idx = match self.node_indices.get(file) {
                Some(&idx) => idx,
                None => continue,
            };

            if let Some(content) = file_contents.get(file) {
                // Extract unique word-boundary identifiers in a single pass
                let identifiers: HashSet<&str> = content
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .filter(|w| w.len() >= 2)
                    .collect();

                for ident in identifiers {
                    if crate::constants::CODEGRAPH_IGNORED_IDENTIFIERS.contains(&ident) {
                        continue;
                    }
                    if let Some(targets) = self.symbol_to_file.get(ident) {
                        for (target_file, _) in targets {
                            if target_file != file {
                                if let Some(&to_idx) = self.node_indices.get(target_file) {
                                    if !self.graph.contains_edge(from_idx, to_idx) {
                                        self.graph.add_edge(from_idx, to_idx, ());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Computes PageRank scores for all indexed files with dangling node mass redistribution.
    pub fn compute_pagerank(&self, active_files: &[PathBuf]) -> Vec<(PathBuf, f64)> {
        let node_count = self.graph.node_count();
        if node_count == 0 {
            return Vec::new();
        }

        let n = node_count as f64;
        let mut scores: HashMap<NodeIndex, f64> = HashMap::new();
        let initial_score = 1.0 / n;

        for node_idx in self.graph.node_indices() {
            scores.insert(node_idx, initial_score);
        }

        let damping = crate::constants::PAGERANK_DAMPING;
        let iterations = crate::constants::PAGERANK_ITERATIONS;

        for _ in 0..iterations {
            // Compute dangling sum (nodes with 0 outgoing edges)
            let dangling_sum: f64 = self
                .graph
                .node_indices()
                .filter(|&node| self.graph.neighbors(node).count() == 0)
                .map(|node| *scores.get(&node).unwrap_or(&0.0))
                .sum();

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

                let score = (1.0 - damping) / n
                    + damping * (sum_in + (dangling_sum / n))
                    + personalization_bias;
                next_scores.insert(node, score);
            }

            // L1 score normalization so total score mass sums to 1.0
            let total: f64 = next_scores.values().sum();
            if total > 0.0 {
                for score in next_scores.values_mut() {
                    *score /= total;
                }
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

    /// Evaluates the blast radius and architectural impact of modifying a symbol or file.
    pub fn get_blast_radius(
        &self,
        target_query: &str,
        workspace_root: &Path,
    ) -> Result<BlastRadiusReport> {
        let (target_type, file_path, symbol_name) =
            if let Some(targets) = self.symbol_to_file.get(target_query) {
                if let Some((path, _sym)) = targets.first() {
                    (
                        "symbol".to_string(),
                        path.clone(),
                        Some(target_query.to_string()),
                    )
                } else {
                    return Err(ContextError::Graph(format!(
                        "Symbol '{}' has no associated source files",
                        target_query
                    ))
                    .into());
                }
            } else {
                let candidate_path = if Path::new(target_query).is_absolute() {
                    PathBuf::from(target_query)
                } else {
                    workspace_root.join(target_query)
                };

                let canonical = std::fs::canonicalize(&candidate_path).unwrap_or(candidate_path);
                if self.node_indices.contains_key(&canonical) {
                    ("file".to_string(), canonical, None)
                } else {
                    // Try partial file match
                    let found = self.node_indices.keys().find(|p| {
                        p.to_string_lossy().ends_with(target_query)
                            || p.file_name()
                                .map(|n| n.to_string_lossy() == target_query)
                                .unwrap_or(false)
                    });

                    if let Some(matched) = found {
                        ("file".to_string(), matched.clone(), None)
                    } else {
                        return Err(ContextError::Graph(format!(
                            "Target '{}' not found in indexed codebase symbols or files",
                            target_query
                        ))
                        .into());
                    }
                }
            };

        let target_node = *self.node_indices.get(&file_path).ok_or_else(|| {
            ContextError::Graph(format!(
                "Node for path '{}' missing in graph",
                file_path.display()
            ))
        })?;

        // 1. Direct callers (Incoming edges to file_path)
        let mut direct_dependents = Vec::new();
        let mut direct_indices = HashSet::new();

        for neighbor in self
            .graph
            .neighbors_directed(target_node, petgraph::Direction::Incoming)
        {
            if let Some(path) = self.graph.node_weight(neighbor) {
                let rel = path
                    .strip_prefix(workspace_root)
                    .unwrap_or(path)
                    .display()
                    .to_string();
                direct_dependents.push(rel);
                direct_indices.insert(neighbor);
            }
        }

        // 2. Transitive dependents (BFS up to k=3 hops)
        let mut transitive_dependents = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        visited.insert(target_node);
        for &d in &direct_indices {
            visited.insert(d);
            queue.push_back((d, 1));
        }

        while let Some((curr_node, depth)) = queue.pop_front() {
            if depth >= crate::constants::BLAST_RADIUS_MAX_HOPS {
                continue;
            }
            for pred in self
                .graph
                .neighbors_directed(curr_node, petgraph::Direction::Incoming)
            {
                if visited.insert(pred) {
                    if let Some(path) = self.graph.node_weight(pred) {
                        let rel = path
                            .strip_prefix(workspace_root)
                            .unwrap_or(path)
                            .display()
                            .to_string();
                        transitive_dependents.push(rel);
                        queue.push_back((pred, depth + 1));
                    }
                }
            }
        }

        // 3. Test coverage identification
        let mut test_coverage_set = HashSet::new();
        let mut test_coverage = Vec::new();
        for dep in direct_dependents.iter().chain(transitive_dependents.iter()) {
            let dep_lower = dep.to_lowercase();
            if (dep_lower.contains("test") || dep_lower.starts_with("tests/"))
                && test_coverage_set.insert(dep.clone())
            {
                test_coverage.push(dep.clone());
            }
        }

        // 4. Tarjan SCC Mutual Cyclic Dependency Detection
        let sccs = tarjan_scc(&self.graph);
        let mut in_cyclic_dependency = false;
        let mut cycle_members = Vec::new();

        for scc in sccs {
            if scc.len() > 1 && scc.contains(&target_node) {
                in_cyclic_dependency = true;
                for n in scc {
                    if let Some(p) = self.graph.node_weight(n) {
                        cycle_members.push(
                            p.strip_prefix(workspace_root)
                                .unwrap_or(p)
                                .display()
                                .to_string(),
                        );
                    }
                }
                break;
            }
        }

        // 5. Risk Level Rating
        let direct_count = direct_dependents.len();
        let transitive_count = transitive_dependents.len();
        let has_tests = !test_coverage.is_empty();
        let non_test_direct_count = direct_dependents
            .iter()
            .filter(|d| !test_coverage.contains(d))
            .count();

        let risk_level = if in_cyclic_dependency
            || direct_count > crate::constants::BLAST_RADIUS_CRITICAL_DIRECT
            || transitive_count > crate::constants::BLAST_RADIUS_CRITICAL_TRANSITIVE
        {
            "CRITICAL"
        } else if non_test_direct_count > crate::constants::BLAST_RADIUS_HIGH_DIRECT
            || (!has_tests && non_test_direct_count > crate::constants::BLAST_RADIUS_HIGH_NO_TESTS)
        {
            "HIGH"
        } else if non_test_direct_count > crate::constants::BLAST_RADIUS_MEDIUM_DIRECT
            || transitive_count > crate::constants::BLAST_RADIUS_MEDIUM_TRANSITIVE
        {
            "MEDIUM"
        } else {
            "LOW"
        };

        let target_display = if let Some(ref sym) = symbol_name {
            format!(
                "symbol `{}` in `{}`",
                sym,
                file_path.file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            format!(
                "file `{}`",
                file_path
                    .strip_prefix(workspace_root)
                    .unwrap_or(&file_path)
                    .display()
            )
        };

        let mut summary = format!(
            "### 🔍 Blast Radius & Architectural Impact for {}\n\n",
            target_display
        );
        summary.push_str(&format!("- **Risk Assessment**: `{}`\n", risk_level));
        summary.push_str(&format!(
            "- **Direct Callers / Dependents ({})**:\n",
            direct_dependents.len()
        ));
        if direct_dependents.is_empty() {
            summary.push_str("  - None (Isolated / Leaf module)\n");
        } else {
            for dep in &direct_dependents {
                summary.push_str(&format!("  - `{}`\n", dep));
            }
        }

        if !transitive_dependents.is_empty() {
            summary.push_str(&format!(
                "- **Transitive Impact ({} downstream modules)**:\n",
                transitive_dependents.len()
            ));
            for dep in transitive_dependents.iter().take(8) {
                summary.push_str(&format!("  - `{}`\n", dep));
            }
            if transitive_dependents.len() > 8 {
                summary.push_str(&format!(
                    "  - ... and {} more files\n",
                    transitive_dependents.len() - 8
                ));
            }
        }

        summary.push_str(&format!(
            "- **Test Suite Coverage ({})**:\n",
            test_coverage.len()
        ));
        if test_coverage.is_empty() {
            summary.push_str("  - ⚠️ No associated automated test files found\n");
        } else {
            for test in &test_coverage {
                summary.push_str(&format!("  - `✓ {}`\n", test));
            }
        }

        if in_cyclic_dependency {
            summary.push_str(&format!(
                "- **⚠️ Mutual Dependency Cycle Detected ({} files)**:\n",
                cycle_members.len()
            ));
            for member in &cycle_members {
                summary.push_str(&format!("  - `{}`\n", member));
            }
        }

        Ok(BlastRadiusReport {
            target: target_query.to_string(),
            target_type,
            file_path: file_path.display().to_string(),
            direct_dependents,
            transitive_dependents,
            test_coverage,
            in_cyclic_dependency,
            cycle_members,
            risk_level: risk_level.to_string(),
            summary,
        })
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

            if let Some(symbols) = self.file_to_symbols.get(&path) {
                if symbols.is_empty() {
                    continue;
                }

                let rel_path = path.strip_prefix(workspace_root).unwrap_or(&path).display();

                map_lines.push(format!("### {}", rel_path));
                for sym in symbols {
                    if symbol_count >= max_symbols {
                        break;
                    }
                    if sym.kind != "import" {
                        map_lines.push(format!(
                            "  L{}: {} [{}]",
                            sym.line_number, sym.signature, sym.kind
                        ));
                        symbol_count += 1;
                    }
                }
                map_lines.push(String::new());
            }
        }

        map_lines.join("\n")
    }

    /// Accessor for the mapping from symbol names to defining files and definitions
    pub fn symbol_to_file(&self) -> &HashMap<String, Vec<(PathBuf, SymbolDef)>> {
        &self.symbol_to_file
    }

    /// Accessor for the mapping from file paths to all extracted symbols
    pub fn file_to_symbols(&self) -> &HashMap<PathBuf, Vec<SymbolDef>> {
        &self.file_to_symbols
    }

    /// Accessor for the node index map
    #[allow(dead_code)]
    pub fn node_indices(&self) -> &HashMap<PathBuf, NodeIndex> {
        &self.node_indices
    }

    /// Accessor for the underlying Petgraph directed graph
    #[allow(dead_code)]
    pub fn graph(&self) -> &DiGraph<PathBuf, ()> {
        &self.graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_graph_pagerank_and_blast_radius() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_graph_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_a = temp_dir.join("helper.rs");
        let file_b = temp_dir.join("main.rs");
        let file_test = temp_dir.join("helper_test.rs");

        std::fs::write(
            &file_a,
            "pub fn compute_sum(a: i32, b: i32) -> i32 { a + b }",
        )
        .unwrap();
        std::fs::write(&file_b, "fn run() { let x = compute_sum(1, 2); }").unwrap();
        std::fs::write(
            &file_test,
            "#[test] fn test_compute() { assert_eq!(compute_sum(1, 2), 3); }",
        )
        .unwrap();

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        let ranked = graph.compute_pagerank(std::slice::from_ref(&file_b));
        assert!(!ranked.is_empty());

        let report = graph.get_blast_radius("compute_sum", &temp_dir).unwrap();
        assert_eq!(report.target, "compute_sum");
        assert_eq!(report.target_type, "symbol");
        assert!(report
            .direct_dependents
            .iter()
            .any(|d| d.contains("main.rs")));
        assert!(report
            .test_coverage
            .iter()
            .any(|t| t.contains("helper_test.rs")));
        assert_eq!(report.risk_level, "LOW");

        let formatted = graph.format_repomap(&temp_dir, &[file_a], 50);
        assert!(formatted.contains("helper.rs"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_codegraph_handles_duplicate_symbol_names() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_dup_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_a = temp_dir.join("mod_a.rs");
        let file_b = temp_dir.join("mod_b.rs");
        let file_c = temp_dir.join("caller.rs");

        std::fs::write(
            &file_a,
            "pub struct Config;\nimpl Config { pub fn new() -> Self { Self } }",
        )
        .unwrap();
        std::fs::write(
            &file_b,
            "pub struct Options;\nimpl Options { pub fn new() -> Self { Self } }",
        )
        .unwrap();
        std::fs::write(&file_c, "pub fn init() { let _a = mod_a::Config::new(); }").unwrap();

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        // Check that 'new' has multiple symbol definitions stored
        assert!(graph.symbol_to_file.get("new").unwrap().len() >= 2);

        // PageRank should compute and be normalized (sum ~ 1.0)
        let ranked = graph.compute_pagerank(&[]);
        assert_eq!(ranked.len(), 3);
        let total_score: f64 = ranked.iter().map(|(_, s)| s).sum();
        assert!((total_score - 1.0).abs() < 1e-4);

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
