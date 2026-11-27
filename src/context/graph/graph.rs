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

/// Statistics from an incremental graph update pass
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IncrementalStats {
    pub files_scanned: usize,
    pub files_reparsed: usize,
    pub files_removed: usize,
    pub nodes_added: usize,
    pub nodes_removed: usize,
    pub edges_rebuilt: usize,
}

/// Evaluation report on workspace drift against the cached AST code graph
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphDriftReport {
    pub is_stale: bool,
    pub modified_count: usize,
    pub added_count: usize,
    pub removed_count: usize,
    pub total_current_files: usize,
    pub cached_files_count: usize,
}

impl GraphDriftReport {
    pub fn total_drift(&self) -> usize {
        self.modified_count + self.added_count + self.removed_count
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
    #[serde(default)]
    pub composite_risk_score: f64,
}

/// Pre-parsed AST symbol entry with body identifier hashes for $O(|\Delta|)$ shard relinking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolAstRecord {
    pub def: SymbolDef,
    /// Sorted, deduplicated 64-bit FNV-1a hashes of candidate identifiers referenced inside this symbol.
    pub body_ref_hashes: Vec<u64>,
}

/// Per-file cached AST shard avoiding whole-repository Tree-sitter re-parsing on incremental edits
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FileAstShard {
    pub content_hash: u64,
    pub mtime_nanos: u64,
    pub byte_len: u64,
    pub recorded_at_nanos: u64,
    pub all_symbols: Vec<SymbolDef>,
    pub symbol_records: Vec<SymbolAstRecord>,
    pub file_ref_hashes: Vec<u64>,
}

/// Non-blocking CodeGraph telemetry exposed to the Orchestrator so the LLM decides when to invoke graph tools
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeGraphTelemetry {
    Empty,
    Unindexed {
        source_files: usize,
    },
    Synced {
        indexed_files: usize,
        symbol_count: usize,
    },
    DeltaPending {
        indexed_files: usize,
        symbol_count: usize,
        modified: usize,
        added: usize,
        removed: usize,
    },
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

    // Incremental hash & per-file AST shard tracking:
    file_tracker: FileHashTracker,
    file_shards: HashMap<PathBuf, FileAstShard>,

    // AST extractor:
    extractor: RepoMapExtractor,
}

type CachedCodeGraphEntry = Option<(PathBuf, std::time::Instant, std::sync::Arc<CodeGraph>)>;

static GLOBAL_CODE_GRAPH_CACHE: std::sync::OnceLock<std::sync::RwLock<CachedCodeGraphEntry>> =
    std::sync::OnceLock::new();

static ACTIVE_BACKGROUND_SYNCS: std::sync::OnceLock<std::sync::Mutex<HashSet<PathBuf>>> =
    std::sync::OnceLock::new();

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CodeGraph {
    fn clone(&self) -> Self {
        Self {
            graph: self.graph.clone(),
            symbol_node_indices: self.symbol_node_indices.clone(),
            name_to_nodes: self.name_to_nodes.clone(),
            file_to_nodes: self.file_to_nodes.clone(),
            file_node_indices: self.file_node_indices.clone(),
            symbol_to_file: self.symbol_to_file.clone(),
            file_to_symbols: self.file_to_symbols.clone(),
            file_tracker: self.file_tracker.clone(),
            file_shards: self.file_shards.clone(),
            extractor: RepoMapExtractor::new(),
        }
    }
}

impl CodeGraph {
    /// Maximum supported source file count automatically cold-indexed on post-turn mutation when no prior `.minicode/graph.json` exists.
    /// Larger unindexed monorepos index lazily on demand when the Orchestrator invokes CodeGraph tools.
    pub const MAX_AUTO_INDEX_SOURCE_FILES: usize = 512;

    /// Git-style racily-clean guard window (10ms in nanoseconds): when a shard was recorded strictly after `mtime + 10ms`
    /// and `(mtime_nanos, byte_len)` matches, the file is guaranteed clean without reading disk bytes.
    const RACILY_CLEAN_WINDOW_NANOS: u64 = 10_000_000;

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
            file_shards: HashMap::new(),
            extractor: RepoMapExtractor::new(),
        }
    }

    /// Publishes an updated `Arc<CodeGraph>` into the process-wide in-memory cache.
    pub fn store_in_global_cache(workspace_root: &Path, arc_graph: std::sync::Arc<Self>) {
        let cache_lock = GLOBAL_CODE_GRAPH_CACHE.get_or_init(|| std::sync::RwLock::new(None));
        let canonical = workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf());
        if let Ok(mut guard) = cache_lock.write() {
            *guard = Some((canonical, std::time::Instant::now(), arc_graph));
        }
    }

    /// Applies targeted $O(|\Delta|)$ per-file AST shard deltas from `WorkspaceIndexBus` without
    /// walking the workspace directory tree or re-parsing untouched source files.
    pub fn apply_incremental_deltas(
        &mut self,
        workspace_root: &Path,
        deltas: &[(String, crate::context::search::index_bus::FileMutationOp)],
    ) -> IncrementalStats {
        use crate::context::search::index_bus::FileMutationOp;

        let prev_nodes = self.graph.node_count();
        let mut stats = IncrementalStats {
            files_scanned: deltas.len(),
            ..Default::default()
        };
        if deltas.is_empty() {
            return stats;
        }

        let canonical_root = workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf());
        let now_nanos = Self::current_epoch_nanos();
        let mut reparsed_count = 0usize;
        let mut removed_count = 0usize;

        for (rel, op) in deltas {
            let norm = rel.trim_start_matches("./").replace('\\', "/");
            if norm.is_empty() {
                continue;
            }
            let joined = workspace_root.join(&norm);
            let canonical_joined = canonical_root.join(&norm);

            let existed = self.file_shards.remove(&joined).is_some()
                | self.file_shards.remove(&canonical_joined).is_some();
            self.file_tracker.remove(&joined);
            self.file_tracker.remove(&canonical_joined);

            if *op == FileMutationOp::Delete {
                if existed {
                    removed_count += 1;
                }
                continue;
            }

            let ext = joined.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !crate::constants::SUPPORTED_LANG_EXTENSIONS.contains(&ext) || !joined.is_file() {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&joined) else {
                continue;
            };
            let (mtime_nanos, byte_len) = Self::stat_file_nanos_and_len(&joined);
            let hash = FileHashTracker::compute_hash(&content);
            let shard = Self::extract_file_shard(
                &mut self.extractor,
                &joined,
                &content,
                hash,
                mtime_nanos,
                byte_len,
                now_nanos,
            );
            self.file_shards.insert(joined.clone(), shard);
            self.file_tracker
                .update(joined, hash, mtime_nanos / 1_000_000);
            reparsed_count += 1;
        }

        if reparsed_count == 0 && removed_count == 0 {
            return stats;
        }

        let (new_nodes, edges_rebuilt) = self.assemble_from_shards(workspace_root);
        stats.files_reparsed = reparsed_count;
        stats.files_removed = removed_count;
        stats.nodes_added = new_nodes.saturating_sub(prev_nodes);
        stats.nodes_removed = prev_nodes.saturating_sub(new_nodes);
        stats.edges_rebuilt = edges_rebuilt;
        stats
    }

    /// Returns a shared in-memory `Arc<CodeGraph>` across the Orchestrator and `MiniPub` workers,
    /// applying $O(|\Delta|)$ incremental AST shard deltas from `WorkspaceIndexBus` when files change.
    pub fn get_or_build_cached(workspace_root: &Path) -> Result<std::sync::Arc<Self>> {
        use crate::context::search::index_bus::{IndexConsumer, WorkspaceIndexBus};
        const MAX_INCREMENTAL_DELTAS: usize = 64;

        let cache_lock = GLOBAL_CODE_GRAPH_CACHE.get_or_init(|| std::sync::RwLock::new(None));
        let canonical = workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf());
        let dirty = WorkspaceIndexBus::drain_dirty(workspace_root, IndexConsumer::CodeGraph);

        if dirty.is_empty() {
            if let Ok(guard) = cache_lock.read() {
                if let Some((ref cached_root, built_at, ref arc_graph)) = *guard {
                    if cached_root == &canonical && built_at.elapsed().as_secs() < 30 {
                        return Ok(std::sync::Arc::clone(arc_graph));
                    }
                }
            }
        } else if dirty.len() <= MAX_INCREMENTAL_DELTAS {
            if let Ok(mut guard) = cache_lock.write() {
                if let Some((ref cached_root, ref mut built_at, ref mut arc_graph)) = *guard {
                    if cached_root == &canonical && !arc_graph.file_shards.is_empty() {
                        let mut patched = (**arc_graph).clone();
                        let stats = patched.apply_incremental_deltas(workspace_root, &dirty);
                        if stats.files_reparsed > 0 || stats.files_removed > 0 {
                            let _ = patched.save_to_disk(workspace_root);
                        }
                        let arc_patched = std::sync::Arc::new(patched);
                        *arc_graph = std::sync::Arc::clone(&arc_patched);
                        *built_at = std::time::Instant::now();
                        return Ok(arc_patched);
                    }
                }
            }
        }

        let mut graph = Self::new();
        graph.build_graph(workspace_root)?;
        let arc_graph = std::sync::Arc::new(graph);
        Self::store_in_global_cache(workspace_root, std::sync::Arc::clone(&arc_graph));
        Ok(arc_graph)
    }

    /// Non-blocking fast lookup: returns the in-memory cached `CodeGraph` if available, or loads the
    /// persisted snapshot from `.minicode/graph.json` without triggering an AST rebuild.
    #[must_use]
    pub fn try_get_or_load_fast(workspace_root: &Path) -> Option<std::sync::Arc<Self>> {
        let cache_lock = GLOBAL_CODE_GRAPH_CACHE.get_or_init(|| std::sync::RwLock::new(None));
        let canonical = workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf());

        if let Ok(guard) = cache_lock.read() {
            if let Some((ref cached_root, _, ref arc_graph)) = *guard {
                if cached_root == &canonical {
                    return Some(std::sync::Arc::clone(arc_graph));
                }
            }
        }

        let mut graph = Self::new();
        if !graph.load_cached(workspace_root) {
            return None;
        }
        let arc_graph = std::sync::Arc::new(graph);
        Self::store_in_global_cache(workspace_root, std::sync::Arc::clone(&arc_graph));
        Some(arc_graph)
    }

    /// Invalidates the shared in-memory `CodeGraph` cache (called on bulk workspace changes/merges).
    pub fn invalidate_cache() {
        if let Some(cache_lock) = GLOBAL_CODE_GRAPH_CACHE.get() {
            if let Ok(mut guard) = cache_lock.write() {
                *guard = None;
            }
        }
        crate::context::search::index_bus::WorkspaceIndexBus::clear_consumer_queue(
            crate::context::search::index_bus::IndexConsumer::CodeGraph,
        );
    }

    /// Builds or incrementally updates the symbol-level dependency graph, leveraging disk cache if available.
    pub fn build_graph(&mut self, workspace_root: &Path) -> Result<()> {
        if self.load_cached(workspace_root) {
            tracing::debug!("Loaded cached code graph snapshot; running incremental shard update");
            if let Ok(stats) = self.incremental_update(workspace_root) {
                if stats.files_reparsed > 0 || stats.files_removed > 0 {
                    let _ = self.save_to_disk(workspace_root);
                }
            }
            return Ok(());
        }

        self.full_rebuild(workspace_root)?;
        if !self.file_tracker.hashes.is_empty()
            || crate::context::graph_store::GraphStore::graph_file_path(workspace_root).exists()
        {
            let _ = self.save_to_disk(workspace_root);
        }
        Ok(())
    }

    /// Discovers all supported source files in `workspace_root`, respecting `.gitignore` and excluded directories.
    pub(crate) fn discover_supported_source_files(workspace_root: &Path) -> Vec<PathBuf> {
        let mut builder = WalkBuilder::new(workspace_root);
        builder
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .filter_entry(|entry| {
                if let Some(name) = entry.file_name().to_str() {
                    !crate::context::walker::is_excluded_dir_name(name)
                } else {
                    true
                }
            });
        let walker = builder.build();

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
        source_files
    }

    #[inline]
    fn current_epoch_nanos() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }

    #[inline]
    fn stat_file_nanos_and_len(file: &Path) -> (u64, u64) {
        match std::fs::metadata(file) {
            Ok(meta) => {
                let mtime_nanos = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);
                (mtime_nanos, meta.len())
            }
            Err(_) => (0, 0),
        }
    }

    /// Extracts a standalone per-file AST shard (`FileAstShard`) containing symbols and candidate identifier hashes.
    fn extract_file_shard(
        extractor: &mut RepoMapExtractor,
        file: &Path,
        content: &str,
        content_hash: u64,
        mtime_nanos: u64,
        byte_len: u64,
        recorded_at_nanos: u64,
    ) -> FileAstShard {
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        let all_symbols = match extractor.extract_symbols_from_str(ext, content) {
            Ok(syms) => syms,
            Err(e) => {
                tracing::debug!(
                    path = %file.display(),
                    error = %e,
                    "Could not extract AST symbols from file"
                );
                Vec::new()
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        let mut symbol_records = Vec::new();

        for sym in &all_symbols {
            if sym.name.len() >= crate::constants::SYMBOL_REFERENCE_MIN_LEN && sym.kind != "import"
            {
                let start = sym.line_number.saturating_sub(1);
                let end = sym.end_line.min(lines.len());
                let mut body_ref_hashes = Vec::new();
                if start < lines.len() && start <= end {
                    for line in &lines[start..end] {
                        for ident in line
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .filter(|w| w.len() >= crate::constants::SYMBOL_REFERENCE_MIN_LEN)
                        {
                            if !crate::constants::CODEGRAPH_IGNORED_IDENTIFIERS.contains(&ident) {
                                body_ref_hashes.push(FileHashTracker::compute_hash(ident));
                            }
                        }
                    }
                    body_ref_hashes.sort_unstable();
                    body_ref_hashes.dedup();
                }
                symbol_records.push(SymbolAstRecord {
                    def: sym.clone(),
                    body_ref_hashes,
                });
            }
        }

        let mut file_ref_hashes: Vec<u64> = content
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| {
                w.len() >= crate::constants::SYMBOL_REFERENCE_MIN_LEN
                    && !crate::constants::CODEGRAPH_IGNORED_IDENTIFIERS.contains(w)
            })
            .map(FileHashTracker::compute_hash)
            .collect();
        file_ref_hashes.sort_unstable();
        file_ref_hashes.dedup();

        FileAstShard {
            content_hash,
            mtime_nanos,
            byte_len,
            recorded_at_nanos,
            all_symbols,
            symbol_records,
            file_ref_hashes,
        }
    }

    /// Assembles the in-memory `DiGraph` and lookup indices purely from `self.file_shards` in $O(|V| + |E|)$ RAM time,
    /// avoiding any disk reads or Tree-sitter re-parsing of clean files.
    fn assemble_from_shards(&mut self, workspace_root: &Path) -> (usize, usize) {
        self.graph.clear();
        self.symbol_node_indices.clear();
        self.name_to_nodes.clear();
        self.file_to_nodes.clear();
        self.file_node_indices.clear();
        self.symbol_to_file.clear();
        self.file_to_symbols.clear();

        let mut sorted_files: Vec<PathBuf> = self.file_shards.keys().cloned().collect();
        sorted_files.sort();

        // Pass 1: Register file nodes and symbol nodes from cached shards
        for file in &sorted_files {
            let shard = match self.file_shards.get(file) {
                Some(s) => s,
                None => continue,
            };

            let file_node = SymbolNode::file_node(file, Some(workspace_root));
            let file_node_idx = self.graph.add_node(file_node.clone());
            self.file_node_indices.insert(file.clone(), file_node_idx);
            self.symbol_node_indices
                .insert(file_node.qualified_name.clone(), file_node_idx);

            let mut file_sym_indices = Vec::with_capacity(shard.symbol_records.len());
            for rec in &shard.symbol_records {
                let sym = &rec.def;
                self.symbol_to_file
                    .entry(sym.name.clone())
                    .or_default()
                    .push((file.clone(), sym.clone()));

                let sym_node = SymbolNode::new(sym, file, Some(workspace_root));
                let sym_node_idx = self.graph.add_node(sym_node.clone());
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
            self.file_to_nodes.insert(file.clone(), file_sym_indices);
            self.file_to_symbols
                .insert(file.clone(), shard.all_symbols.clone());
        }

        // Build hash-indexed symbol tables for fast O(1) integer reference linking
        let mut hash_to_symbol_nodes: HashMap<u64, Vec<NodeIndex>> =
            HashMap::with_capacity(self.name_to_nodes.len());
        for (name, indices) in &self.name_to_nodes {
            hash_to_symbol_nodes
                .entry(FileHashTracker::compute_hash(name))
                .or_default()
                .extend(indices.iter().copied());
        }

        let mut hash_to_symbol_files: HashMap<u64, Vec<PathBuf>> =
            HashMap::with_capacity(self.symbol_to_file.len());
        for (name, entries) in &self.symbol_to_file {
            let bucket = hash_to_symbol_files
                .entry(FileHashTracker::compute_hash(name))
                .or_default();
            for (p, _) in entries {
                bucket.push(p.clone());
            }
        }

        // Pass 2: Link directed symbol-level and file-level edges
        for file in &sorted_files {
            let file_node_idx = match self.file_node_indices.get(file) {
                Some(&idx) => idx,
                None => continue,
            };
            let shard = match self.file_shards.get(file) {
                Some(s) => s,
                None => continue,
            };

            if let Some(sym_indices) = self.file_to_nodes.get(file).cloned() {
                for (rec, caller_idx) in shard.symbol_records.iter().zip(sym_indices) {
                    let caller_kind = self
                        .graph
                        .node_weight(caller_idx)
                        .map(|n| n.kind)
                        .unwrap_or(SymbolKind::Other);
                    let edge_kind = if caller_kind == SymbolKind::Impl {
                        EdgeKind::Implements
                    } else {
                        EdgeKind::Calls
                    };

                    for &ref_hash in &rec.body_ref_hashes {
                        if let Some(target_indices) = hash_to_symbol_nodes.get(&ref_hash) {
                            let same_file_targets: Vec<NodeIndex> = target_indices
                                .iter()
                                .copied()
                                .filter(|&t_idx| {
                                    self.graph
                                        .node_weight(t_idx)
                                        .map(|n| &n.file_path == file)
                                        .unwrap_or(false)
                                })
                                .collect();
                            let resolved_targets: &[NodeIndex] = if !same_file_targets.is_empty() {
                                &same_file_targets
                            } else {
                                target_indices.as_slice()
                            };
                            for &target_idx in resolved_targets {
                                if target_idx != caller_idx
                                    && !self.graph.contains_edge(caller_idx, target_idx)
                                {
                                    self.graph.add_edge(caller_idx, target_idx, edge_kind);
                                }
                            }
                        }
                    }
                }
            }

            for &ref_hash in &shard.file_ref_hashes {
                if let Some(target_files) = hash_to_symbol_files.get(&ref_hash) {
                    let has_same_file_symbol = target_files.iter().any(|tf| tf == file);
                    if !has_same_file_symbol {
                        for target_file in target_files {
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

        (self.graph.node_count(), self.graph.edge_count())
    }

    /// Performs a full, cold-start re-indexing of the workspace.
    pub fn full_rebuild(&mut self, workspace_root: &Path) -> Result<()> {
        self.file_shards.clear();
        self.file_tracker = FileHashTracker::new();

        let source_files = Self::discover_supported_source_files(workspace_root);
        let now_nanos = Self::current_epoch_nanos();

        for file in &source_files {
            let (mtime_nanos, byte_len) = Self::stat_file_nanos_and_len(file);
            let content = match std::fs::read_to_string(file) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let hash = FileHashTracker::compute_hash(&content);
            self.file_tracker
                .update(file.clone(), hash, mtime_nanos / 1_000_000);
            let shard = Self::extract_file_shard(
                &mut self.extractor,
                file,
                &content,
                hash,
                mtime_nanos,
                byte_len,
                now_nanos,
            );
            self.file_shards.insert(file.clone(), shard);
        }

        self.assemble_from_shards(workspace_root);
        Ok(())
    }

    /// Restores graph state from a disk snapshot
    pub fn restore_from_snapshot(
        &mut self,
        snapshot: crate::context::graph_store::GraphSnapshot,
    ) -> Result<()> {
        self.graph.clear();
        self.symbol_node_indices.clear();
        self.name_to_nodes.clear();
        self.file_to_nodes.clear();
        self.file_node_indices.clear();
        self.symbol_to_file.clear();
        self.file_to_symbols.clear();

        let mut node_indices = Vec::with_capacity(snapshot.nodes.len());

        for node in snapshot.nodes {
            let node_idx = self.graph.add_node(node.clone());
            node_indices.push(node_idx);

            if node.kind == SymbolKind::File {
                self.file_node_indices
                    .insert(node.file_path.clone(), node_idx);
                self.symbol_node_indices
                    .insert(node.qualified_name.clone(), node_idx);
            } else {
                self.symbol_node_indices
                    .insert(node.qualified_name.clone(), node_idx);
                self.name_to_nodes
                    .entry(node.name.clone())
                    .or_default()
                    .push(node_idx);
                self.file_to_nodes
                    .entry(node.file_path.clone())
                    .or_default()
                    .push(node_idx);

                let sym_def = SymbolDef {
                    name: node.name.clone(),
                    kind: node.kind.as_str().to_string(),
                    signature: node.signature.clone(),
                    line_number: node.start_line,
                    end_line: node.end_line,
                    doc_comment: node.doc_comment.clone(),
                };

                self.symbol_to_file
                    .entry(node.name.clone())
                    .or_default()
                    .push((node.file_path.clone(), sym_def.clone()));
                self.file_to_symbols
                    .entry(node.file_path.clone())
                    .or_default()
                    .push(sym_def);
            }
        }

        for (source_idx, target_idx, edge_kind) in snapshot.edges {
            if source_idx < node_indices.len() && target_idx < node_indices.len() {
                self.graph.add_edge(
                    node_indices[source_idx],
                    node_indices[target_idx],
                    edge_kind,
                );
            }
        }

        self.file_tracker = FileHashTracker {
            hashes: snapshot.file_hashes,
        };

        Ok(())
    }

    /// Attempts to load cached graph from `.minicode/graph.json` (plus `.minicode/graph_shards.bin` when present).
    pub fn load_cached(&mut self, workspace_root: &Path) -> bool {
        match crate::context::graph_store::GraphStore::load_snapshot(workspace_root) {
            Ok(Some(snapshot)) => {
                if let Err(e) = self.restore_from_snapshot(snapshot) {
                    tracing::debug!(error = %e, "Failed to restore graph from snapshot");
                    false
                } else {
                    self.file_shards =
                        crate::context::graph_store::GraphStore::load_shards(workspace_root)
                            .unwrap_or_default();
                    true
                }
            }
            _ => false,
        }
    }

    /// Loads a cached graph from disk if available and runs an incremental shard update,
    /// or performs a full build if no valid cache exists.
    pub fn load_or_build(workspace_root: &Path) -> Result<Self> {
        let mut graph = Self::new();
        if graph.load_cached(workspace_root) {
            if let Ok(stats) = graph.incremental_update(workspace_root) {
                if stats.files_reparsed > 0 || stats.files_removed > 0 {
                    let _ = graph.save_to_disk(workspace_root);
                }
            }
            Ok(graph)
        } else {
            graph.build_graph(workspace_root)?;
            Ok(graph)
        }
    }

    /// Persists current graph state to `.minicode/graph.json` and `.minicode/graph_shards.bin`.
    pub fn save_to_disk(&self, workspace_root: &Path) -> Result<()> {
        let snapshot =
            crate::context::graph_store::GraphStore::create_snapshot(self, workspace_root);
        crate::context::graph_store::GraphStore::save_snapshot(&snapshot, workspace_root)?;
        if !self.file_shards.is_empty()
            || crate::context::graph_store::GraphStore::shards_file_path(workspace_root).exists()
        {
            let _ = crate::context::graph_store::GraphStore::save_shards(
                &self.file_shards,
                workspace_root,
            );
        }
        Ok(())
    }

    /// True $O(|\Delta|)$ per-file AST shard update:
    /// 1. Short-circuits untouched files via `(mtime_nanos, byte_len)` metadata with Git-style racily-clean safety (`0` bytes read).
    /// 2. Short-circuits metadata-touched files whose `FNV-1a` content hash is unchanged (`0` Tree-sitter parses).
    /// 3. Runs Tree-sitter **only** on dirty or newly added files ($\Delta$), drops deleted shards, and relinks edges in memory.
    pub fn incremental_update(&mut self, workspace_root: &Path) -> Result<IncrementalStats> {
        let current_files: HashSet<PathBuf> = Self::discover_supported_source_files(workspace_root)
            .into_iter()
            .collect();

        let prev_nodes = self.graph.node_count();
        let mut stats = IncrementalStats {
            files_scanned: current_files.len(),
            ..Default::default()
        };

        // 1. Prune removed files from tracker and shard table
        let removed_files = self.file_tracker.removed_files(&current_files);
        let removed_count = removed_files.len();
        for removed in &removed_files {
            self.file_tracker.remove(removed);
            self.file_shards.remove(removed);
        }
        self.file_shards
            .retain(|path, _| current_files.contains(path));

        // 2. Screen current files: stat-first -> hash-check -> Tree-sitter on Δ only
        let now_nanos = Self::current_epoch_nanos();
        let mut reparsed_count = 0usize;
        let mut hydrated_legacy_shards = false;

        for file in &current_files {
            let (mtime_nanos, byte_len) = Self::stat_file_nanos_and_len(file);

            if let Some(shard) = self.file_shards.get(file) {
                let racily_safe = mtime_nanos > 0
                    && shard.recorded_at_nanos
                        > mtime_nanos.saturating_add(Self::RACILY_CLEAN_WINDOW_NANOS);
                if racily_safe && shard.mtime_nanos == mtime_nanos && shard.byte_len == byte_len {
                    self.file_tracker.update(
                        file.clone(),
                        shard.content_hash,
                        mtime_nanos / 1_000_000,
                    );
                    continue;
                }
            }

            let content = match std::fs::read_to_string(file) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let hash = FileHashTracker::compute_hash(&content);
            let was_dirty = self.file_tracker.is_dirty(file, hash);

            if !was_dirty {
                if let Some(existing_shard) = self.file_shards.get_mut(file) {
                    existing_shard.mtime_nanos = mtime_nanos;
                    existing_shard.byte_len = byte_len;
                    existing_shard.recorded_at_nanos = now_nanos;
                    self.file_tracker
                        .update(file.clone(), hash, mtime_nanos / 1_000_000);
                    continue;
                }
                hydrated_legacy_shards = true;
            } else {
                reparsed_count += 1;
            }

            let shard = Self::extract_file_shard(
                &mut self.extractor,
                file,
                &content,
                hash,
                mtime_nanos,
                byte_len,
                now_nanos,
            );
            self.file_shards.insert(file.clone(), shard);
            self.file_tracker
                .update(file.clone(), hash, mtime_nanos / 1_000_000);
        }

        if reparsed_count == 0 && removed_count == 0 {
            if hydrated_legacy_shards {
                let _ = crate::context::graph_store::GraphStore::save_shards(
                    &self.file_shards,
                    workspace_root,
                );
            }
            return Ok(stats);
        }

        let (new_nodes, edges_rebuilt) = self.assemble_from_shards(workspace_root);
        stats.files_reparsed = reparsed_count;
        stats.files_removed = removed_count;
        stats.nodes_added = new_nodes.saturating_sub(prev_nodes);
        stats.nodes_removed = prev_nodes.saturating_sub(new_nodes);
        stats.edges_rebuilt = edges_rebuilt;

        Ok(stats)
    }

    /// Evaluates whether the workspace has drifted from the cached graph using metadata-first screening.
    pub fn check_drift(&self, workspace_root: &Path) -> Result<GraphDriftReport> {
        let current_files: HashSet<PathBuf> = Self::discover_supported_source_files(workspace_root)
            .into_iter()
            .collect();

        let total_current_files = current_files.len();
        let cached_files_count = self.file_tracker.hashes.len();
        let removed_count = self.file_tracker.removed_files(&current_files).len();

        let mut added_count = 0;
        let mut modified_count = 0;

        for file in &current_files {
            match self.file_tracker.hashes.get(file) {
                None => {
                    added_count += 1;
                }
                Some(&(stored_hash, stored_mtime)) => {
                    let (mtime_nanos, byte_len) = Self::stat_file_nanos_and_len(file);
                    if let Some(shard) = self.file_shards.get(file) {
                        let racily_safe = mtime_nanos > 0
                            && shard.recorded_at_nanos
                                > mtime_nanos.saturating_add(Self::RACILY_CLEAN_WINDOW_NANOS);
                        if racily_safe
                            && shard.mtime_nanos == mtime_nanos
                            && shard.byte_len == byte_len
                        {
                            continue;
                        }
                    } else if mtime_nanos > 0 && (mtime_nanos / 1_000_000) == stored_mtime {
                        continue;
                    }

                    if let Ok(content) = std::fs::read_to_string(file) {
                        let current_hash = FileHashTracker::compute_hash(&content);
                        if current_hash != stored_hash {
                            modified_count += 1;
                        }
                    }
                }
            }
        }

        let total_drift = modified_count + added_count + removed_count;
        let is_stale = total_drift >= crate::constants::DEFAULT_DRIFT_STALE_COUNT
            || (total_drift >= crate::constants::DEFAULT_DRIFT_MIN_STALE_FILES
                && cached_files_count > 0
                && ((total_drift as f64) / (cached_files_count as f64)
                    >= crate::constants::DEFAULT_DRIFT_STALE_RATIO));

        Ok(GraphDriftReport {
            is_stale,
            modified_count,
            added_count,
            removed_count,
            total_current_files,
            cached_files_count,
        })
    }

    /// Synchronously updates or initializes `.minicode/graph.json` + `.minicode/graph_shards.bin`
    /// after workspace mutations, deduplicating concurrent runs per workspace root.
    pub fn sync_after_workspace_mutation(workspace_root: &Path) -> Result<bool> {
        let canonical = workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf());
        let active_set =
            ACTIVE_BACKGROUND_SYNCS.get_or_init(|| std::sync::Mutex::new(HashSet::new()));
        {
            let mut guard = match active_set.lock() {
                Ok(g) => g,
                Err(_) => return Ok(false),
            };
            if !guard.insert(canonical.clone()) {
                return Ok(false);
            }
        }

        struct SyncGuard {
            canonical: PathBuf,
        }
        impl Drop for SyncGuard {
            fn drop(&mut self) {
                if let Some(lock) = ACTIVE_BACKGROUND_SYNCS.get() {
                    if let Ok(mut g) = lock.lock() {
                        g.remove(&self.canonical);
                    }
                }
            }
        }
        let _guard = SyncGuard { canonical };

        let graph_file = crate::context::graph_store::GraphStore::graph_file_path(workspace_root);
        let shards_file = crate::context::graph_store::GraphStore::shards_file_path(workspace_root);
        let source_files = Self::discover_supported_source_files(workspace_root);

        if source_files.is_empty() {
            if graph_file.exists() {
                let _ = std::fs::remove_file(&graph_file);
            }
            if shards_file.exists() {
                let _ = std::fs::remove_file(&shards_file);
            }
            Self::invalidate_cache();
            return Ok(false);
        }

        if graph_file.exists() {
            let mut graph = Self::new();
            if graph.load_cached(workspace_root) {
                let stats = graph.incremental_update(workspace_root)?;
                if stats.files_reparsed > 0 || stats.files_removed > 0 {
                    graph.save_to_disk(workspace_root)?;
                }
                Self::store_in_global_cache(workspace_root, std::sync::Arc::new(graph));
                return Ok(true);
            }

            // Self-heal unreadable/corrupted snapshot without blocking the user
            if source_files.len() <= Self::MAX_AUTO_INDEX_SOURCE_FILES {
                graph.full_rebuild(workspace_root)?;
                graph.save_to_disk(workspace_root)?;
                Self::store_in_global_cache(workspace_root, std::sync::Arc::new(graph));
                return Ok(true);
            }
            let _ = std::fs::remove_file(&graph_file);
            let _ = std::fs::remove_file(&shards_file);
            Self::invalidate_cache();
            return Ok(false);
        }

        if source_files.len() <= Self::MAX_AUTO_INDEX_SOURCE_FILES {
            let mut graph = Self::new();
            graph.full_rebuild(workspace_root)?;
            graph.save_to_disk(workspace_root)?;
            Self::store_in_global_cache(workspace_root, std::sync::Arc::new(graph));
            return Ok(true);
        }

        Ok(false)
    }

    /// Spawns a non-blocking background AST shard sync after turn completion or detected external delta.
    pub fn spawn_background_mutation_sync(workspace_root: PathBuf) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn_blocking(move || {
                if let Err(e) = Self::sync_after_workspace_mutation(&workspace_root) {
                    tracing::debug!(
                        path = %workspace_root.display(),
                        error = %e,
                        "Background CodeGraph shard sync skipped"
                    );
                }
            });
        } else {
            let _ = std::thread::Builder::new()
                .name("minicode-graph-sync".to_string())
                .spawn(move || {
                    let _ = Self::sync_after_workspace_mutation(&workspace_root);
                });
        }
    }

    /// Inspects live CodeGraph readiness without blocking or running Tree-sitter,
    /// triggering silent background healing if an existing `.minicode/graph.json` has drifted.
    pub fn inspect_workspace_telemetry(
        workspace_root: &Path,
        discovered_code_files: &[PathBuf],
    ) -> CodeGraphTelemetry {
        if discovered_code_files.is_empty() {
            return CodeGraphTelemetry::Empty;
        }

        let graph_file = crate::context::graph_store::GraphStore::graph_file_path(workspace_root);
        if !graph_file.exists() {
            return CodeGraphTelemetry::Unindexed {
                source_files: discovered_code_files.len(),
            };
        }

        let Some(graph) = Self::try_get_or_load_fast(workspace_root) else {
            // Corrupted or outdated schema on disk: heal silently in background
            Self::spawn_background_mutation_sync(workspace_root.to_path_buf());
            return CodeGraphTelemetry::Unindexed {
                source_files: discovered_code_files.len(),
            };
        };

        let symbol_count = graph.symbol_nodes().count();
        let indexed_files = graph.file_count();

        if let Ok(drift) = graph.check_drift(workspace_root) {
            if drift.total_drift() > 0 {
                Self::spawn_background_mutation_sync(workspace_root.to_path_buf());
                return CodeGraphTelemetry::DeltaPending {
                    indexed_files,
                    symbol_count,
                    modified: drift.modified_count,
                    added: drift.added_count,
                    removed: drift.removed_count,
                };
            }
        }

        CodeGraphTelemetry::Synced {
            indexed_files,
            symbol_count,
        }
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

        // 1. Resolve Target Node(s): prefer exact qualified_name (`file::symbol`), fallback to unqualified symbol name
        let resolved_symbol_nodes: Option<Vec<NodeIndex>> = self
            .symbol_node_indices
            .get(target_query)
            .and_then(|&idx| {
                self.graph
                    .node_weight(idx)
                    .filter(|n| n.kind != SymbolKind::File)
                    .map(|_| vec![idx])
            })
            .or_else(|| self.name_to_nodes.get(target_query).cloned());

        if let Some(target_nodes) = resolved_symbol_nodes {
            // Target is a symbol
            target_type = "symbol".to_string();
            target_symbol_name = Some(target_query.to_string());

            let mut visited_nodes = HashSet::new();
            let mut queue = VecDeque::new();

            for &sym_idx in &target_nodes {
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

        let callers_risk = ((direct_count as f64 + 0.5 * transitive_count as f64) / 10.0).min(1.0);
        let cycle_risk = if in_cyclic_dependency { 1.0 } else { 0.0 };
        let test_risk = if has_tests { 0.0 } else { 1.0 };
        let pagerank_approx = (direct_count as f64 * 0.05).min(1.0);
        let composite_risk_score =
            (0.35 * callers_risk + 0.25 * pagerank_approx + 0.25 * test_risk + 0.15 * cycle_risk)
                .min(1.0);

        summary.push_str(&format!(
            "- **Composite Risk Score**: `{:.3}` / 1.000\n",
            composite_risk_score
        ));

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
            composite_risk_score,
        })
    }

    /// Formats the top-ranked repository symbols into a compact Repo-Map skeleton outline.
    pub fn format_repomap(
        &self,
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

    /// Formats a Ponytail-inspired compact 1-line-per-directory symbol reuse map (`dir/: sym1, sym2, ...`)
    /// within `char_budget` characters (`REUSE_MAP_DEFAULT_CHAR_BUDGET`).
    ///
    /// Directories are ranked purely algorithmically by aggregate `CodeGraph` PageRank centrality +
    /// incoming cross-directory call/dependency edges (zero hardcoded folder lists, zero regexes).
    #[must_use]
    pub fn format_compact_reuse_map(
        &self,
        workspace_root: &Path,
        active_files: &[PathBuf],
        char_budget: usize,
    ) -> String {
        use petgraph::visit::EdgeRef;

        if char_budget == 0 {
            return String::new();
        }

        let ranked_files = self.compute_pagerank(active_files);
        if ranked_files.is_empty() {
            return String::new();
        }

        let rel_dir_key = |file_path: &Path| -> String {
            let rel = file_path.strip_prefix(workspace_root).unwrap_or(file_path);
            match rel.parent() {
                Some(parent) => {
                    let s = parent.to_string_lossy().replace('\\', "/");
                    if s.is_empty() {
                        ".".to_string()
                    } else {
                        s
                    }
                }
                None => ".".to_string(),
            }
        };

        // Compute cross-directory incoming dependency degree from the AST CodeGraph
        let mut cross_dir_inbound: HashMap<String, usize> = HashMap::new();
        for edge in self.graph.edge_references() {
            if matches!(edge.weight(), EdgeKind::Contains) {
                continue;
            }
            let Some(src_node) = self.graph.node_weight(edge.source()) else {
                continue;
            };
            let Some(dst_node) = self.graph.node_weight(edge.target()) else {
                continue;
            };
            let src_dir = rel_dir_key(&src_node.file_path);
            let dst_dir = rel_dir_key(&dst_node.file_path);
            if src_dir != dst_dir {
                *cross_dir_inbound.entry(dst_dir).or_insert(0) += 1;
            }
        }

        let mut dir_files: HashMap<String, Vec<(PathBuf, f64)>> = HashMap::new();
        let mut dir_pagerank: HashMap<String, f64> = HashMap::new();
        for (path, pr_score) in ranked_files {
            let dir_key = rel_dir_key(&path);
            *dir_pagerank.entry(dir_key.clone()).or_insert(0.0) += pr_score;
            dir_files.entry(dir_key).or_default().push((path, pr_score));
        }

        let mut ranked_dirs: Vec<(String, f64)> = dir_files
            .keys()
            .map(|dir_key| {
                let pr = dir_pagerank.get(dir_key).copied().unwrap_or(0.0);
                let inbound = cross_dir_inbound.get(dir_key).copied().unwrap_or(0);
                let combined_score =
                    pr + (inbound as f64) * crate::constants::REUSE_MAP_CROSS_DIR_EDGE_WEIGHT;
                (dir_key.clone(), combined_score)
            })
            .collect();

        ranked_dirs.sort_by(|(dir_a, score_a), (dir_b, score_b)| {
            score_b
                .partial_cmp(score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| dir_a.cmp(dir_b))
        });

        let mut lines = Vec::new();
        let mut used_chars = 0usize;

        for (dir_key, _) in ranked_dirs {
            let Some(files) = dir_files.get_mut(&dir_key) else {
                continue;
            };
            files.sort_by(|(pa, sa), (pb, sb)| {
                sb.partial_cmp(sa)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| pa.cmp(pb))
            });

            let per_file_limit =
                if files.len() > crate::constants::REUSE_MAP_DENSE_DIR_FILE_THRESHOLD {
                    crate::constants::REUSE_MAP_SYMBOLS_PER_FILE_DENSE
                } else {
                    crate::constants::REUSE_MAP_SYMBOLS_PER_FILE_NORMAL
                };

            let mut dir_symbols = Vec::new();
            let mut seen_in_dir = HashSet::new();

            for (file_path, _) in files.iter() {
                if dir_symbols.len() >= crate::constants::REUSE_MAP_MAX_SYMBOLS_PER_DIR {
                    break;
                }
                let Some(symbols) = self.file_to_symbols.get(file_path) else {
                    continue;
                };
                let mut taken_from_file = 0usize;
                for sym in symbols {
                    if sym.kind == "import"
                        || sym.kind == "impl"
                        || sym.name.len() < crate::constants::REUSE_MIN_IDENTIFIER_LEN
                    {
                        continue;
                    }
                    if seen_in_dir.insert(sym.name.clone()) {
                        dir_symbols.push(sym.name.clone());
                        taken_from_file += 1;
                        if taken_from_file >= per_file_limit
                            || dir_symbols.len() >= crate::constants::REUSE_MAP_MAX_SYMBOLS_PER_DIR
                        {
                            break;
                        }
                    }
                }
            }

            if dir_symbols.is_empty() {
                continue;
            }

            let prefix = if dir_key == "." {
                "./".to_string()
            } else {
                format!("{}/", dir_key)
            };
            let line = format!("{}: {}", prefix, dir_symbols.join(", "));
            let line_cost = line.len() + usize::from(!lines.is_empty());
            if used_chars + line_cost > char_budget {
                break;
            }
            used_chars += line_cost;
            lines.push(line);
        }

        lines.join("\n")
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

    /// Accessor for the number of indexed files
    pub fn file_count(&self) -> usize {
        self.file_node_indices.len()
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

    #[test]
    fn test_graph_persistence_and_incremental_update() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_persist_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_a = temp_dir.join("math.rs");
        let file_b = temp_dir.join("main.rs");

        std::fs::write(&file_a, "pub fn double(x: i32) -> i32 { x * 2 }").unwrap();
        std::fs::write(&file_b, "fn start() { let _y = double(4); }").unwrap();

        // 1. Build and save
        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        let graph_file = crate::context::graph_store::GraphStore::graph_file_path(&temp_dir);
        assert!(graph_file.exists());

        // 2. Load into new graph instance from disk
        let mut restored = CodeGraph::new();
        let loaded = restored.load_cached(&temp_dir);
        assert!(loaded);
        assert_eq!(restored.graph().node_count(), graph.graph().node_count());

        // 3. Modify 1 file and run incremental update
        std::fs::write(
            &file_a,
            "pub fn double(x: i32) -> i32 { x * 2 }\npub fn triple(x: i32) -> i32 { x * 3 }",
        )
        .unwrap();

        let stats = restored.incremental_update(&temp_dir).unwrap();
        assert_eq!(stats.files_reparsed, 1);
        assert_eq!(stats.files_removed, 0);

        let sym_names: Vec<_> = restored.symbol_nodes().map(|s| s.name.clone()).collect();
        assert!(sym_names.contains(&"triple".to_string()));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_drift_detector_clean_repo() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_drift_clean_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_a = temp_dir.join("lib.rs");
        std::fs::write(&file_a, "pub fn helper() -> bool { true }").unwrap();

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        let drift = graph.check_drift(&temp_dir).unwrap();
        assert!(!drift.is_stale);
        assert_eq!(drift.modified_count, 0);
        assert_eq!(drift.added_count, 0);
        assert_eq!(drift.removed_count, 0);
        assert_eq!(drift.total_current_files, 1);
        assert_eq!(drift.cached_files_count, 1);

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_drift_detector_stale_threshold() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_drift_stale_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create 15 initial files
        for i in 0..15 {
            let f = temp_dir.join(format!("mod_{}.rs", i));
            std::fs::write(&f, format!("pub fn f_{}() {{}}", i)).unwrap();
        }

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        let clean_drift = graph.check_drift(&temp_dir).unwrap();
        assert!(!clean_drift.is_stale);

        // Modify 11 files to cross the >= 10 drift threshold
        for i in 0..11 {
            let f = temp_dir.join(format!("mod_{}.rs", i));
            std::fs::write(&f, format!("pub fn f_{}() {{ println!(\"drift\"); }}", i)).unwrap();
        }

        let stale_drift = graph.check_drift(&temp_dir).unwrap();
        assert!(stale_drift.is_stale);
        assert_eq!(stale_drift.modified_count, 11);
        assert_eq!(stale_drift.added_count, 0);
        assert_eq!(stale_drift.removed_count, 0);

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_format_compact_reuse_map_ranks_by_graph_inbound_edges() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_reuse_map_{}", uuid::Uuid::new_v4()));
        let core_dir = temp_dir.join("src/foundation");
        let leaf_a = temp_dir.join("src/feature_alpha");
        let leaf_b = temp_dir.join("src/feature_beta");
        std::fs::create_dir_all(&core_dir).unwrap();
        std::fs::create_dir_all(&leaf_a).unwrap();
        std::fs::create_dir_all(&leaf_b).unwrap();

        std::fs::write(
            core_dir.join("primitives.rs"),
            "pub fn clamp_score(v: i32) -> i32 { v }\npub fn hash_key(s: &str) -> u64 { s.len() as u64 }",
        )
        .unwrap();
        std::fs::write(
            leaf_a.join("alpha.rs"),
            "pub fn run_alpha() -> i32 { clamp_score(10) + hash_key(\"a\") as i32 }",
        )
        .unwrap();
        std::fs::write(
            leaf_b.join("beta.rs"),
            "pub fn run_beta() -> i32 { clamp_score(20) + hash_key(\"b\") as i32 }",
        )
        .unwrap();

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        let reuse_map = graph.format_compact_reuse_map(
            &temp_dir,
            &[],
            crate::constants::REUSE_MAP_DEFAULT_CHAR_BUDGET,
        );
        let lines: Vec<&str> = reuse_map.lines().collect();
        assert_eq!(lines.len(), 3);
        // `src/foundation` has incoming calls from both `src/feature_alpha` and `src/feature_beta`,
        // so it must rank first purely by graph topology without any folder-name heuristics!
        assert!(
            lines[0].starts_with("src/foundation/:"),
            "Expected src/foundation/: first, got: {:?}",
            lines
        );
        assert!(lines[0].contains("clamp_score"));
        assert!(lines[0].contains("hash_key"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_empty_workspace_build_does_not_pollute_zero_file_graph_json() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_empty_ws_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        let graph_file = crate::context::graph_store::GraphStore::graph_file_path(&temp_dir);
        assert!(
            !graph_file.exists(),
            "Empty fresh workspace must not persist a premature 0-file graph.json"
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_per_file_ast_shards_incremental_delta_and_deletion() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_shards_inc_{}", uuid::Uuid::new_v4()));
        let src_dir = temp_dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let api_file = src_dir.join("api.ts");
        let card_file = src_dir.join("Card.tsx");
        let legacy_file = src_dir.join("legacy.ts");

        std::fs::write(
            &api_file,
            "export function fetchProducts(): string[] { return ['a']; }\n",
        )
        .unwrap();
        std::fs::write(
            &card_file,
            "import { fetchProducts } from './api';\nexport function ProductCard() { return fetchProducts(); }\n",
        )
        .unwrap();
        std::fs::write(
            &legacy_file,
            "export function oldUnusedHelper() { return 1; }\n",
        )
        .unwrap();

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();
        assert!(crate::context::graph_store::GraphStore::shards_file_path(&temp_dir).exists());

        // Reload in a brand-new session instance and verify 0 reparses when clean
        let mut session2 = CodeGraph::new();
        assert!(session2.load_cached(&temp_dir));
        let clean_stats = session2.incremental_update(&temp_dir).unwrap();
        assert_eq!(clean_stats.files_reparsed, 0);
        assert_eq!(clean_stats.files_removed, 0);

        // Modify 1 file, delete 1 file, leave `Card.tsx` untouched
        std::fs::write(
            &api_file,
            "export function fetchProducts(): string[] { return ['a']; }\nexport function fetchCartTotal(): number { return 42; }\n",
        )
        .unwrap();
        std::fs::remove_file(&legacy_file).unwrap();

        let delta_stats = session2.incremental_update(&temp_dir).unwrap();
        assert_eq!(delta_stats.files_reparsed, 1);
        assert_eq!(delta_stats.files_removed, 1);

        let sym_names: HashSet<String> = session2.symbol_nodes().map(|s| s.name.clone()).collect();
        assert!(sym_names.contains("fetchCartTotal"));
        assert!(sym_names.contains("ProductCard"));
        assert!(!sym_names.contains("oldUnusedHelper"));

        // Verify cross-file edge from untouched `ProductCard` -> updated `fetchProducts` remained intact
        let report = session2
            .get_blast_radius("fetchProducts", &temp_dir)
            .unwrap();
        assert!(
            report
                .direct_caller_symbols
                .iter()
                .any(|s| s.contains("ProductCard")),
            "Cross-file caller edge from untouched shard must be preserved: {:?}",
            report.direct_caller_symbols
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_sync_after_workspace_mutation_indexes_website_and_persists_across_sessions() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_website_sync_{}", uuid::Uuid::new_v4()));
        let src_dir = temp_dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        // Simulate Turn 1 building a 20-file website from a fresh empty directory
        for i in 0..20 {
            std::fs::write(
                src_dir.join(format!("Section{}.tsx", i)),
                format!("export function Section{}() {{ return <section/>; }}\n", i),
            )
            .unwrap();
        }

        let synced = CodeGraph::sync_after_workspace_mutation(&temp_dir).unwrap();
        assert!(synced);

        // Reopen in a fresh session: zero drift, all 20 files already indexed on disk
        let mut reopened = CodeGraph::new();
        assert!(reopened.load_cached(&temp_dir));
        assert_eq!(reopened.file_count(), 20);
        let drift = reopened.check_drift(&temp_dir).unwrap();
        assert_eq!(drift.total_drift(), 0);
        assert!(!drift.is_stale);

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
