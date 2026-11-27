//! Unified workspace incremental index bus built on a `watchexec`-style algebraic
//! `DirtyPathCoalescer` event lattice.
//!
//! Every file mutation (`write_file`, `patch_file`, `replace_in_files`, `remove_file`,
//! `move_file`, `ast_grep_rewrite`, `ast_refactor`) emits a single `(workspace_root, rel_path, op)`
//! event here. Each index consumer (`Trigram`, `Symbol`, `Semantic`, `Hybrid`, `CodeGraph`)
//! maintains its own independent coalesced delta queue so draining one index never discards
//! pending updates for another, enabling $O(|\Delta|)$ incremental updates across all 5 indexes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// Workspace file mutation event fed into [`DirtyPathCoalescer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMutationOp {
    Create,
    Modify,
    Delete,
}

/// Independent incremental index subscribers attached to [`WorkspaceIndexBus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexConsumer {
    Trigram,
    Symbol,
    Semantic,
    Hybrid,
    CodeGraph,
}

impl IndexConsumer {
    pub const ALL: [Self; 5] = [
        Self::Trigram,
        Self::Symbol,
        Self::Semantic,
        Self::Hybrid,
        Self::CodeGraph,
    ];
}

/// Pure algebraic event-lattice coalescer (`watchexec` pattern) collapsing rapid bursts of
/// `Create`/`Modify`/`Delete` mutations on the same workspace path into minimal net deltas.
#[derive(Debug, Default, Clone)]
pub struct DirtyPathCoalescer {
    pending: HashMap<String, FileMutationOp>,
}

impl DirtyPathCoalescer {
    /// Algebraic state-lattice merge:
    /// - `Create + Modify -> Create`, `Create + Delete -> None` (net no-op)
    /// - `Modify + Delete -> Delete`, `Delete + Create/Modify -> Modify`
    #[inline]
    pub fn coalesce_op(
        prev: Option<FileMutationOp>,
        next: FileMutationOp,
    ) -> Option<FileMutationOp> {
        match (prev, next) {
            (None, op) => Some(op),
            (Some(FileMutationOp::Create), FileMutationOp::Create | FileMutationOp::Modify) => {
                Some(FileMutationOp::Create)
            }
            (Some(FileMutationOp::Create), FileMutationOp::Delete) => None,
            (Some(FileMutationOp::Modify), FileMutationOp::Delete) => Some(FileMutationOp::Delete),
            (Some(FileMutationOp::Modify), FileMutationOp::Create | FileMutationOp::Modify) => {
                Some(FileMutationOp::Modify)
            }
            (Some(FileMutationOp::Delete), FileMutationOp::Create | FileMutationOp::Modify) => {
                Some(FileMutationOp::Modify)
            }
            (Some(FileMutationOp::Delete), FileMutationOp::Delete) => Some(FileMutationOp::Delete),
        }
    }

    pub fn record(&mut self, rel_path: &str, op: FileMutationOp) {
        let norm = rel_path.trim_start_matches("./").replace('\\', "/");
        if norm.is_empty() {
            return;
        }
        let prev = self.pending.get(&norm).copied();
        match Self::coalesce_op(prev, op) {
            Some(merged) => {
                self.pending.insert(norm, merged);
            }
            None => {
                self.pending.remove(&norm);
            }
        }
    }

    pub fn drain(&mut self) -> Vec<(String, FileMutationOp)> {
        self.pending.drain().collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }
}

type BusKey = (PathBuf, IndexConsumer);
static WORKSPACE_INDEX_BUS: OnceLock<RwLock<HashMap<BusKey, DirtyPathCoalescer>>> = OnceLock::new();

/// Process-wide incremental delta fan-out bus coordinating all workspace search & graph indexes.
pub struct WorkspaceIndexBus;

impl WorkspaceIndexBus {
    fn canonical_root(workspace_root: &Path) -> PathBuf {
        std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf())
    }

    fn bus_lock() -> &'static RwLock<HashMap<BusKey, DirtyPathCoalescer>> {
        WORKSPACE_INDEX_BUS.get_or_init(|| RwLock::new(HashMap::new()))
    }

    /// Fans out a single workspace file mutation into every [`IndexConsumer`]'s coalesced lattice.
    pub fn record_file_mutation(workspace_root: &Path, rel_path: &str, op: FileMutationOp) {
        let key_root = Self::canonical_root(workspace_root);
        if let Ok(mut guard) = Self::bus_lock().write() {
            for consumer in IndexConsumer::ALL {
                guard
                    .entry((key_root.clone(), consumer))
                    .or_default()
                    .record(rel_path, op);
            }
        }
    }

    /// Records a mutation strictly for a single consumer (used by legacy/direct per-index callers).
    pub fn record_for_consumer(
        workspace_root: &Path,
        consumer: IndexConsumer,
        rel_path: &str,
        op: FileMutationOp,
    ) {
        let key_root = Self::canonical_root(workspace_root);
        if let Ok(mut guard) = Self::bus_lock().write() {
            guard
                .entry((key_root, consumer))
                .or_default()
                .record(rel_path, op);
        }
    }

    /// Drains all coalesced dirty paths accumulated for `consumer` under `workspace_root`.
    pub fn drain_dirty(
        workspace_root: &Path,
        consumer: IndexConsumer,
    ) -> Vec<(String, FileMutationOp)> {
        let key_root = Self::canonical_root(workspace_root);
        if let Ok(mut guard) = Self::bus_lock().write() {
            guard
                .get_mut(&(key_root, consumer))
                .map(DirtyPathCoalescer::drain)
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Clears pending coalesced dirty paths for a single consumer across all workspaces.
    pub fn clear_consumer_queue(consumer: IndexConsumer) {
        if let Some(lock) = WORKSPACE_INDEX_BUS.get() {
            if let Ok(mut guard) = lock.write() {
                guard.retain(|(_, c), _| *c != consumer);
            }
        }
    }

    /// Clears all pending dirty queues AND invalidates all 5 in-memory index caches atomically
    /// (called on bulk workspace mutations such as git checkout or worktree merges).
    pub fn invalidate_all() {
        if let Some(lock) = WORKSPACE_INDEX_BUS.get() {
            if let Ok(mut guard) = lock.write() {
                guard.clear();
            }
        }
        crate::context::search::trigram::TrigramIndex::invalidate_cache();
        crate::context::search::index::SymbolIndex::invalidate_cache();
        crate::context::search::semantic::SemanticIndex::invalidate_cache();
        crate::context::search::hybrid::HybridIndex::invalidate_cache();
        crate::context::graph::CodeGraph::invalidate_cache();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_index_bus_independent_consumer_drains_and_lattice_coalescing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        // Create + Modify -> Create; Create + Delete -> net no-op
        WorkspaceIndexBus::record_file_mutation(root, "src/ephemeral.rs", FileMutationOp::Create);
        WorkspaceIndexBus::record_file_mutation(root, "src/ephemeral.rs", FileMutationOp::Modify);
        WorkspaceIndexBus::record_file_mutation(root, "src/ephemeral.rs", FileMutationOp::Delete);

        // Delete + Create -> Modify
        WorkspaceIndexBus::record_file_mutation(root, "src/reborn.rs", FileMutationOp::Delete);
        WorkspaceIndexBus::record_file_mutation(root, "src/reborn.rs", FileMutationOp::Create);

        // Draining Trigram leaves Symbol, Semantic, Hybrid, and CodeGraph untouched
        let trigram_deltas = WorkspaceIndexBus::drain_dirty(root, IndexConsumer::Trigram);
        assert_eq!(
            trigram_deltas,
            vec![("src/reborn.rs".to_string(), FileMutationOp::Modify)]
        );
        assert!(WorkspaceIndexBus::drain_dirty(root, IndexConsumer::Trigram).is_empty());

        for consumer in [
            IndexConsumer::Symbol,
            IndexConsumer::Semantic,
            IndexConsumer::Hybrid,
            IndexConsumer::CodeGraph,
        ] {
            let deltas = WorkspaceIndexBus::drain_dirty(root, consumer);
            assert_eq!(
                deltas,
                vec![("src/reborn.rs".to_string(), FileMutationOp::Modify)],
                "consumer {:?} must independently receive coalesced deltas",
                consumer
            );
        }
    }

    #[test]
    fn test_workspace_index_bus_incremental_fanout_updates_all_five_indexes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("mkdir src");
        std::fs::write(
            root.join("src/alpha.rs"),
            "pub fn alpha_initial_fn() -> i32 { 1 }\n",
        )
        .expect("write alpha");

        // Warm all 5 cached indexes
        let tri1 = crate::context::search::trigram::TrigramIndex::get_or_build_cached(root)
            .expect("trigram 1");
        let sym1 = crate::context::search::index::SymbolIndex::get_or_build_cached(root)
            .expect("symbol 1");
        let sem1 = crate::context::search::semantic::SemanticIndex::get_or_build_cached(root)
            .expect("semantic 1");
        let hyb1 = crate::context::search::hybrid::HybridIndex::get_or_build_cached(root)
            .expect("hybrid 1");
        let cg1 = crate::context::graph::CodeGraph::get_or_build_cached(root).expect("graph 1");

        assert!(tri1.all_files().iter().any(|f| f.ends_with("alpha.rs")));
        assert!(!sym1.locate_symbol("alpha_initial_fn").is_empty());
        assert!(!sem1.search("alpha_initial_fn", 3).is_empty());
        assert!(!hyb1.search("alpha_initial_fn", 3, true).is_empty());
        assert!(cg1.name_to_nodes().contains_key("alpha_initial_fn"));

        // Mutate file on disk and emit ONE coalesced bus event
        std::fs::write(
            root.join("src/alpha.rs"),
            "pub fn alpha_updated_fn() -> i32 { 2 }\n",
        )
        .expect("rewrite alpha");
        WorkspaceIndexBus::record_file_mutation(root, "src/alpha.rs", FileMutationOp::Modify);

        // Every index must see the updated symbol incrementally without invalidating others
        let sym2 = crate::context::search::index::SymbolIndex::get_or_build_cached(root)
            .expect("symbol 2");
        assert!(sym2.locate_symbol("alpha_initial_fn").is_empty());
        assert!(!sym2.locate_symbol("alpha_updated_fn").is_empty());

        let sem2 = crate::context::search::semantic::SemanticIndex::get_or_build_cached(root)
            .expect("semantic 2");
        assert!(sem2
            .search("alpha_updated_fn", 5)
            .iter()
            .any(|h| h.snippet.contains("alpha_updated_fn")));

        let cg2 = crate::context::graph::CodeGraph::get_or_build_cached(root).expect("graph 2");
        assert!(!cg2.name_to_nodes().contains_key("alpha_initial_fn"));
        assert!(cg2.name_to_nodes().contains_key("alpha_updated_fn"));

        let hyb2 = crate::context::search::hybrid::HybridIndex::get_or_build_cached(root)
            .expect("hybrid 2");
        assert!(hyb2
            .search("alpha_updated_fn", 5, true)
            .iter()
            .any(|h| h.symbol_name.as_deref() == Some("alpha_updated_fn")));

        let tri2 = crate::context::search::trigram::TrigramIndex::get_or_build_cached(root)
            .expect("trigram 2");
        let plan = crate::context::search::trigram::build_query_plan("alpha_updated_fn", false);
        assert!(tri2
            .query_candidate_files(&plan, None)
            .iter()
            .any(|f| f.ends_with("alpha.rs")));
    }
}
