use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::error::{ContextError, Result};
use ignore::WalkBuilder;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Normalized classification kind for a symbol in the graph
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Trait,
    Interface,
    Enum,
    TypeAlias,
    Impl,
    Import,
    Module,
    Variable,
    File,
    Other,
}

impl SymbolKind {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Class => "class",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::TypeAlias => "type_alias",
            Self::Impl => "impl",
            Self::Import => "import",
            Self::Module => "module",
            Self::Variable => "variable",
            Self::File => "file",
            Self::Other => "other",
        }
    }

    pub fn from_kind_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "function" => Self::Function,
            "method" => Self::Method,
            "struct" => Self::Struct,
            "class" => Self::Class,
            "trait" => Self::Trait,
            "interface" => Self::Interface,
            "enum" => Self::Enum,
            "type_alias" => Self::TypeAlias,
            "impl" => Self::Impl,
            "import" => Self::Import,
            "module" => Self::Module,
            "variable" => Self::Variable,
            "file" => Self::File,
            _ => Self::Other,
        }
    }
}

/// A node in the symbol-level code graph representing a function, struct, trait, file, etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SymbolNode {
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
    pub doc_comment: Option<String>,
}

impl SymbolNode {
    pub fn new(sym: &SymbolDef, file_path: &Path, workspace_root: Option<&Path>) -> Self {
        let rel_path = if let Some(root) = workspace_root {
            file_path.strip_prefix(root).unwrap_or(file_path)
        } else {
            file_path
        };
        let qualified_name = format!("{}::{}", rel_path.display(), sym.name);
        Self {
            name: sym.name.clone(),
            qualified_name,
            kind: SymbolKind::from_kind_str(&sym.kind),
            file_path: file_path.to_path_buf(),
            start_line: sym.line_number,
            end_line: sym.end_line,
            signature: sym.signature.clone(),
            doc_comment: sym.doc_comment.clone(),
        }
    }

    pub fn file_node(file_path: &Path, workspace_root: Option<&Path>) -> Self {
        let rel_path = if let Some(root) = workspace_root {
            file_path.strip_prefix(root).unwrap_or(file_path)
        } else {
            file_path
        };
        let name = file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| rel_path.display().to_string());
        let qualified_name = rel_path.display().to_string();
        Self {
            name,
            qualified_name,
            kind: SymbolKind::File,
            file_path: file_path.to_path_buf(),
            start_line: 1,
            end_line: 1,
            signature: format!("file {}", rel_path.display()),
            doc_comment: None,
        }
    }
}

/// Relationship edge type between symbols in the code graph
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Calls,      // function A calls function B
    Imports,    // file/module imports symbol
    Implements, // struct implements trait
    Contains,   // file contains symbol, or impl contains method
    References, // symbol references another symbol
    DependsOn,  // file-level dependency (backward compat)
}

/// Tracks file content hashes for incremental graph rebuilds
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileHashTracker {
    /// Map from canonical file path to (content_hash, mtime_millis)
    pub hashes: HashMap<PathBuf, (u64, u64)>,
}

impl FileHashTracker {
    pub fn new() -> Self {
        Self {
            hashes: HashMap::new(),
        }
    }

    pub fn compute_hash(content: &str) -> u64 {
        let mut hash = crate::constants::FNV_OFFSET_BASIS;
        for byte in content.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(crate::constants::FNV_PRIME);
        }
        hash
    }

    #[allow(dead_code)]
    pub fn is_dirty(&self, path: &Path, current_hash: u64) -> bool {
        match self.hashes.get(path) {
            Some(&(stored_hash, _)) => stored_hash != current_hash,
            None => true,
        }
    }

    pub fn update(&mut self, path: PathBuf, hash: u64, mtime: u64) {
        self.hashes.insert(path, (hash, mtime));
    }

    #[allow(dead_code)]
    pub fn removed_files(&self, current_files: &HashSet<PathBuf>) -> Vec<PathBuf> {
        self.hashes
            .keys()
            .filter(|p| !current_files.contains(*p))
            .cloned()
            .collect()
    }

    #[allow(dead_code)]
    pub fn remove(&mut self, path: &Path) {
        self.hashes.remove(path);
    }
}

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
    #[serde(default)]
    pub direct_caller_symbols: Vec<String>,
    #[serde(default)]
    pub transitive_caller_symbols: Vec<String>,
}

pub struct CodeGraph {
    graph: DiGraph<SymbolNode, EdgeKind>,
    symbol_node_indices: HashMap<String, NodeIndex>, // qualified_name -> NodeIndex
    name_to_nodes: HashMap<String, Vec<NodeIndex>>,  // symbol name -> NodeIndex
    file_to_nodes: HashMap<PathBuf, Vec<NodeIndex>>, // file -> symbol nodes in that file
    file_node_indices: HashMap<PathBuf, NodeIndex>,  // file -> file node index in graph

    // Backward-compat accessors:
    symbol_to_file: HashMap<String, Vec<(PathBuf, SymbolDef)>>,
    file_to_symbols: HashMap<PathBuf, Vec<SymbolDef>>,

    // Incremental hash tracking:
    file_tracker: FileHashTracker,

    // AST extractor:
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
            symbol_node_indices: HashMap::new(),
            name_to_nodes: HashMap::new(),
            file_to_nodes: HashMap::new(),
            file_node_indices: HashMap::new(),
            symbol_to_file: HashMap::new(),
            file_to_symbols: HashMap::new(),
            file_tracker: FileHashTracker::new(),
            extractor: RepoMapExtractor::new(),
        }
    }

    /// Indexes the entire workspace, constructing a symbol-level dependency graph.
    pub fn build_graph(&mut self, workspace_root: &Path) -> Result<()> {
        self.graph.clear();
        self.symbol_node_indices.clear();
        self.name_to_nodes.clear();
        self.file_to_nodes.clear();
        self.file_node_indices.clear();
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

        let mut file_contents = HashMap::new();

        // 1. Add file nodes and extract AST symbols
        for file in &source_files {
            match std::fs::read_to_string(file) {
                Ok(c) => {
                    let hash = FileHashTracker::compute_hash(&c);
                    let mtime = std::fs::metadata(file)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    self.file_tracker.update(file.clone(), hash, mtime);
                    file_contents.insert(file.clone(), c);
                }
                Err(_) => continue,
            };

            // Add File Node
            let file_node = SymbolNode::file_node(file, Some(workspace_root));
            let file_node_idx = self.graph.add_node(file_node.clone());
            self.file_node_indices.insert(file.clone(), file_node_idx);
            self.symbol_node_indices
                .insert(file_node.qualified_name.clone(), file_node_idx);

            match self.extractor.extract_file_symbols(file) {
                Ok(symbols) => {
                    let mut file_sym_indices = Vec::new();
                    for sym in &symbols {
                        if sym.name.len() >= crate::constants::SYMBOL_REFERENCE_MIN_LEN
                            && sym.kind != "import"
                        {
                            self.symbol_to_file
                                .entry(sym.name.clone())
                                .or_default()
                                .push((file.clone(), sym.clone()));

                            let sym_node = SymbolNode::new(sym, file, Some(workspace_root));
                            let sym_node_idx = self.graph.add_node(sym_node.clone());

                            // File Contains Symbol edge
                            self.graph
                                .add_edge(file_node_idx, sym_node_idx, EdgeKind::Contains);

                            self.symbol_node_indices
                                .insert(sym_node.qualified_name.clone(), sym_node_idx);
                            self.name_to_nodes
                                .entry(sym.name.clone())
                                .or_default()
                                .push(sym_node_idx);
                            file_sym_indices.push(sym_node_idx);
                        }
                    }
                    self.file_to_nodes.insert(file.clone(), file_sym_indices);
                    self.file_to_symbols.insert(file.clone(), symbols);
                }
                Err(e) => {
                    tracing::debug!(path = %file.display(), error = %e, "Could not extract AST symbols from file");
                }
            }
        }

        // 2. Build directed edges between symbols and files
        for file in &source_files {
            let file_node_idx = match self.file_node_indices.get(file) {
                Some(&idx) => idx,
                None => continue,
            };

            if let Some(content) = file_contents.get(file) {
                let lines: Vec<&str> = content.lines().collect();

                // Connect symbol-to-symbol calls and file dependencies
                if let Some(sym_indices) = self.file_to_nodes.get(file) {
                    for &caller_idx in sym_indices {
                        let caller_node = match self.graph.node_weight(caller_idx) {
                            Some(n) => n.clone(),
                            None => continue,
                        };

                        let start = caller_node.start_line.saturating_sub(1);
                        let end = caller_node.end_line.min(lines.len());
                        let body_text = if start < lines.len() && start <= end {
                            lines[start..end].join("\n")
                        } else {
                            String::new()
                        };

                        let identifiers: HashSet<&str> = body_text
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .filter(|w| w.len() >= crate::constants::SYMBOL_REFERENCE_MIN_LEN)
                            .collect();

                        for ident in identifiers {
                            if crate::constants::CODEGRAPH_IGNORED_IDENTIFIERS.contains(&ident) {
                                continue;
                            }
                            if let Some(target_indices) = self.name_to_nodes.get(ident) {
                                for &target_idx in target_indices {
                                    if target_idx != caller_idx {
                                        let edge_kind = if caller_node.kind == SymbolKind::Impl {
                                            EdgeKind::Implements
                                        } else {
                                            EdgeKind::Calls
                                        };
                                        if !self.graph.contains_edge(caller_idx, target_idx) {
                                            self.graph.add_edge(caller_idx, target_idx, edge_kind);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // File-level dependency edges (for fast file-level traversal & backward compat)
                let all_identifiers: HashSet<&str> = content
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .filter(|w| w.len() >= crate::constants::SYMBOL_REFERENCE_MIN_LEN)
                    .collect();

                for ident in all_identifiers {
                    if crate::constants::CODEGRAPH_IGNORED_IDENTIFIERS.contains(&ident) {
                        continue;
                    }
                    if let Some(targets) = self.symbol_to_file.get(ident) {
                        for (target_file, _) in targets {
                            if target_file != file {
                                if let Some(&target_file_idx) =
                                    self.file_node_indices.get(target_file)
                                {
                                    if !self.graph.contains_edge(file_node_idx, target_file_idx) {
                                        self.graph.add_edge(
                                            file_node_idx,
                                            target_file_idx,
                                            EdgeKind::DependsOn,
                                        );
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

    /// Computes symbol-level PageRank scores across all nodes.
    pub fn compute_symbol_pagerank(&self, active_files: &[PathBuf]) -> Vec<(SymbolNode, f64)> {
        let node_count = self.graph.node_count();
        if node_count == 0 {
            return Vec::new();
        }

        let n = node_count as f64;
        let mut scores: HashMap<NodeIndex, f64> = HashMap::with_capacity(node_count);
        let initial_score = 1.0 / n;

        for node_idx in self.graph.node_indices() {
            scores.insert(node_idx, initial_score);
        }

        let damping = crate::constants::PAGERANK_DAMPING;
        let iterations = crate::constants::PAGERANK_ITERATIONS;
        let mut next_scores: HashMap<NodeIndex, f64> = HashMap::with_capacity(node_count);

        let active_set: HashSet<&PathBuf> = active_files.iter().collect();

        for _ in 0..iterations {
            next_scores.clear();

            // Dangling node redistribution
            let dangling_sum: f64 = self
                .graph
                .node_indices()
                .filter(|&node| self.graph.neighbors(node).count() == 0)
                .map(|node| *scores.get(&node).unwrap_or(&0.0))
                .sum();

            for node in self.graph.node_indices() {
                let mut sum_in = 0.0;
                for neighbor in self
                    .graph
                    .neighbors_directed(node, petgraph::Direction::Incoming)
                {
                    let out_degree = self.graph.neighbors(neighbor).count().max(1);
                    sum_in += scores.get(&neighbor).unwrap_or(&0.0) / out_degree as f64;
                }

                // Personalization boost for active files
                let personalization_bias = if let Some(node_data) = self.graph.node_weight(node) {
                    if active_set.contains(&node_data.file_path) {
                        crate::constants::PAGERANK_PERSONALIZATION_BIAS
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                let score = (1.0 - damping) / n
                    + damping * (sum_in + (dangling_sum / n))
                    + personalization_bias;
                next_scores.insert(node, score);
            }

            // L1 score normalization
            let total: f64 = next_scores.values().sum();
            if total > 0.0 {
                for score in next_scores.values_mut() {
                    *score /= total;
                }
            }

            std::mem::swap(&mut scores, &mut next_scores);
        }

        let mut results: Vec<(SymbolNode, f64)> = self
            .graph
            .node_indices()
            .filter_map(|idx| {
                self.graph
                    .node_weight(idx)
                    .map(|node| (node.clone(), *scores.get(&idx).unwrap_or(&0.0)))
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Computes PageRank scores aggregated by file (backward-compatible API).
    pub fn compute_pagerank(&self, active_files: &[PathBuf]) -> Vec<(PathBuf, f64)> {
        let symbol_ranks = self.compute_symbol_pagerank(active_files);
        let mut file_scores: HashMap<PathBuf, f64> = HashMap::new();

        for (sym_node, score) in symbol_ranks {
            *file_scores.entry(sym_node.file_path).or_default() += score;
        }

        let mut results: Vec<(PathBuf, f64)> = file_scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Evaluates the blast radius and architectural impact of modifying a symbol or file.
    pub fn get_blast_radius(
        &self,
        target_query: &str,
        workspace_root: &Path,
    ) -> Result<BlastRadiusReport> {
        let mut direct_caller_symbols = Vec::new();
        let mut transitive_caller_symbols = Vec::new();
        let mut direct_dependents = Vec::new();
        let mut transitive_dependents = Vec::new();
        let mut target_type = "file".to_string();
        let mut target_file_path = PathBuf::new();
        let mut target_symbol_name: Option<String> = None;

        // 1. Resolve Target Node(s)
        if let Some(target_nodes) = self.name_to_nodes.get(target_query) {
            // Target is a symbol
            target_type = "symbol".to_string();
            target_symbol_name = Some(target_query.to_string());

            let mut visited_nodes = HashSet::new();
            let mut queue = VecDeque::new();

            for &sym_idx in target_nodes {
                if let Some(sym_node) = self.graph.node_weight(sym_idx) {
                    if target_file_path.as_os_str().is_empty() {
                        target_file_path = sym_node.file_path.clone();
                    }
                }
                visited_nodes.insert(sym_idx);

                // Direct callers of symbol
                for neighbor in self
                    .graph
                    .neighbors_directed(sym_idx, petgraph::Direction::Incoming)
                {
                    if let Some(caller_node) = self.graph.node_weight(neighbor) {
                        if caller_node.kind != SymbolKind::File {
                            direct_caller_symbols.push(caller_node.qualified_name.clone());
                            let rel = caller_node
                                .file_path
                                .strip_prefix(workspace_root)
                                .unwrap_or(&caller_node.file_path)
                                .display()
                                .to_string();
                            if !direct_dependents.contains(&rel) {
                                direct_dependents.push(rel);
                            }
                            if visited_nodes.insert(neighbor) {
                                queue.push_back((neighbor, 1));
                            }
                        }
                    }
                }
            }

            // Transitive BFS up to BLAST_RADIUS_MAX_HOPS
            while let Some((curr_idx, depth)) = queue.pop_front() {
                if depth >= crate::constants::BLAST_RADIUS_MAX_HOPS {
                    continue;
                }
                for pred in self
                    .graph
                    .neighbors_directed(curr_idx, petgraph::Direction::Incoming)
                {
                    if visited_nodes.insert(pred) {
                        if let Some(node) = self.graph.node_weight(pred) {
                            if node.kind != SymbolKind::File {
                                transitive_caller_symbols.push(node.qualified_name.clone());
                                let rel = node
                                    .file_path
                                    .strip_prefix(workspace_root)
                                    .unwrap_or(&node.file_path)
                                    .display()
                                    .to_string();
                                if !direct_dependents.contains(&rel)
                                    && !transitive_dependents.contains(&rel)
                                {
                                    transitive_dependents.push(rel);
                                }
                                queue.push_back((pred, depth + 1));
                            }
                        }
                    }
                }
            }
        } else {
            // Target is a file path
            let candidate_path = if Path::new(target_query).is_absolute() {
                PathBuf::from(target_query)
            } else {
                workspace_root.join(target_query)
            };

            let canonical = std::fs::canonicalize(&candidate_path).unwrap_or(candidate_path);
            let matched_file = if self.file_node_indices.contains_key(&canonical) {
                Some(canonical)
            } else {
                self.file_node_indices
                    .keys()
                    .find(|p| {
                        p.to_string_lossy().ends_with(target_query)
                            || p.file_name()
                                .map(|n| n.to_string_lossy() == target_query)
                                .unwrap_or(false)
                    })
                    .cloned()
            };

            let file_path = matched_file.ok_or_else(|| {
                ContextError::Graph(format!(
                    "Target '{}' not found in indexed codebase symbols or files",
                    target_query
                ))
            })?;

            target_file_path = file_path.clone();
            let target_node = *self.file_node_indices.get(&file_path).ok_or_else(|| {
                ContextError::Graph(format!("File node for '{}' missing", file_path.display()))
            })?;

            // Direct callers of file
            let mut direct_indices = HashSet::new();
            for neighbor in self
                .graph
                .neighbors_directed(target_node, petgraph::Direction::Incoming)
            {
                if let Some(node) = self.graph.node_weight(neighbor) {
                    let rel = node
                        .file_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(&node.file_path)
                        .display()
                        .to_string();
                    if !direct_dependents.contains(&rel) {
                        direct_dependents.push(rel);
                        direct_indices.insert(neighbor);
                    }
                }
            }

            // Transitive BFS
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
                        if let Some(node) = self.graph.node_weight(pred) {
                            let rel = node
                                .file_path
                                .strip_prefix(workspace_root)
                                .unwrap_or(&node.file_path)
                                .display()
                                .to_string();
                            if !direct_dependents.contains(&rel)
                                && !transitive_dependents.contains(&rel)
                            {
                                transitive_dependents.push(rel);
                            }
                            queue.push_back((pred, depth + 1));
                        }
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

        // 4. Tarjan SCC Cyclic Dependency Detection
        let sccs = tarjan_scc(&self.graph);
        let mut in_cyclic_dependency = false;
        let mut cycle_members = Vec::new();

        let target_node_indices: Vec<NodeIndex> = if let Some(ref sym) = target_symbol_name {
            self.name_to_nodes.get(sym).cloned().unwrap_or_default()
        } else {
            self.file_node_indices
                .get(&target_file_path)
                .copied()
                .into_iter()
                .collect()
        };

        for scc in sccs {
            if scc.len() > 1 && scc.iter().any(|n| target_node_indices.contains(n)) {
                in_cyclic_dependency = true;
                for n in scc {
                    if let Some(node) = self.graph.node_weight(n) {
                        let name = node.qualified_name.clone();
                        if !cycle_members.contains(&name) {
                            cycle_members.push(name);
                        }
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

        let target_display = if let Some(ref sym) = target_symbol_name {
            format!(
                "symbol `{}` in `{}`",
                sym,
                target_file_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            )
        } else {
            format!(
                "file `{}`",
                target_file_path
                    .strip_prefix(workspace_root)
                    .unwrap_or(&target_file_path)
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

        if !direct_caller_symbols.is_empty() {
            summary.push_str(&format!(
                "- **Caller Symbols ({})**:\n",
                direct_caller_symbols.len()
            ));
            for sym in direct_caller_symbols.iter().take(8) {
                summary.push_str(&format!("  - `← {}`\n", sym));
            }
            if direct_caller_symbols.len() > 8 {
                summary.push_str(&format!(
                    "  - ... and {} more symbols\n",
                    direct_caller_symbols.len() - 8
                ));
            }
        }

        if !transitive_dependents.is_empty() {
            summary.push_str(&format!(
                "- **Transitive Impact ({} downstream files)**:\n",
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
                "- **⚠️ Mutual Dependency Cycle Detected ({} nodes)**:\n",
                cycle_members.len()
            ));
            for member in &cycle_members {
                summary.push_str(&format!("  - `{}`\n", member));
            }
        }

        Ok(BlastRadiusReport {
            target: target_query.to_string(),
            target_type,
            file_path: target_file_path.display().to_string(),
            direct_dependents,
            transitive_dependents,
            test_coverage,
            in_cyclic_dependency,
            cycle_members,
            risk_level: risk_level.to_string(),
            summary,
            direct_caller_symbols,
            transitive_caller_symbols,
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

    /// Accessor for symbol nodes
    #[allow(dead_code)]
    pub fn symbol_nodes(&self) -> impl Iterator<Item = &SymbolNode> {
        self.graph.node_indices().filter_map(|idx| {
            let node = self.graph.node_weight(idx)?;
            if node.kind != SymbolKind::File {
                Some(node)
            } else {
                None
            }
        })
    }

    /// Accessor for symbol name to node indices mapping
    #[allow(dead_code)]
    pub fn name_to_nodes(&self) -> &HashMap<String, Vec<NodeIndex>> {
        &self.name_to_nodes
    }

    /// Accessor for qualified name to node index mapping
    #[allow(dead_code)]
    pub fn symbol_node_indices(&self) -> &HashMap<String, NodeIndex> {
        &self.symbol_node_indices
    }

    /// Accessor for the mapping from symbol names to defining files and definitions
    pub fn symbol_to_file(&self) -> &HashMap<String, Vec<(PathBuf, SymbolDef)>> {
        &self.symbol_to_file
    }

    /// Accessor for the mapping from file paths to all extracted symbols
    pub fn file_to_symbols(&self) -> &HashMap<PathBuf, Vec<SymbolDef>> {
        &self.file_to_symbols
    }

    /// Accessor for file tracker
    #[allow(dead_code)]
    pub fn file_tracker(&self) -> &FileHashTracker {
        &self.file_tracker
    }

    /// Accessor for file tracker mutable
    #[allow(dead_code)]
    pub fn file_tracker_mut(&mut self) -> &mut FileHashTracker {
        &mut self.file_tracker
    }

    /// Accessor for the underlying Petgraph directed graph
    #[allow(dead_code)]
    pub fn graph(&self) -> &DiGraph<SymbolNode, EdgeKind> {
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

    #[test]
    fn test_symbol_nodes_and_hash_tracker() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_sym_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_a = temp_dir.join("calc.rs");
        std::fs::write(
            &file_a,
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn mul(a: i32, b: i32) -> i32 { a * b }",
        )
        .unwrap();

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        // Verify symbol nodes are indexed
        let syms: Vec<_> = graph.symbol_nodes().collect();
        assert!(syms.iter().any(|s| s.name == "add"));
        assert!(syms.iter().any(|s| s.name == "mul"));

        // Verify hash tracker recorded hash
        let hash = FileHashTracker::compute_hash("test content");
        assert_ne!(hash, 0);

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
