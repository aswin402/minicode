use crate::context::graph::{CodeGraph, EdgeKind, SymbolNode};
use crate::error::{ContextError, Result};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Disk snapshot representation of the symbol-level CodeGraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub version: u32,
    pub created_at: String,
    pub workspace_root: String,
    pub file_hashes: HashMap<PathBuf, (u64, u64)>,
    pub nodes: Vec<SymbolNode>,
    /// Serialized edges: (source_index, target_index, EdgeKind)
    pub edges: Vec<(usize, usize, EdgeKind)>,
}

pub struct GraphStore;

impl GraphStore {
    pub const SCHEMA_VERSION: u32 = 1;
    pub const GRAPH_FILE_NAME: &'static str = "graph.json";
    pub const GRAPH_DIR_NAME: &'static str = ".minicode";
    pub const MAX_GRAPH_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024; // 10 MB limit to prevent bloat

    /// Resolves canonical path to `.minicode/graph.json` in workspace root
    pub fn graph_file_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(Self::GRAPH_DIR_NAME)
            .join(Self::GRAPH_FILE_NAME)
    }

    /// Serializes a CodeGraph to a GraphSnapshot
    pub fn create_snapshot(graph: &CodeGraph, workspace_root: &Path) -> GraphSnapshot {
        let mut nodes = Vec::new();
        let mut node_to_idx: HashMap<NodeIndex, usize> = HashMap::new();

        for (idx, node_idx) in graph.graph().node_indices().enumerate() {
            if let Some(node) = graph.graph().node_weight(node_idx) {
                nodes.push(node.clone());
                node_to_idx.insert(node_idx, idx);
            }
        }

        let mut edges = Vec::new();
        for edge in graph.graph().edge_references() {
            if let (Some(&source), Some(&target)) = (
                node_to_idx.get(&edge.source()),
                node_to_idx.get(&edge.target()),
            ) {
                edges.push((source, target, *edge.weight()));
            }
        }

        GraphSnapshot {
            version: Self::SCHEMA_VERSION,
            created_at: chrono::Utc::now().to_rfc3339(),
            workspace_root: workspace_root.display().to_string(),
            file_hashes: graph.file_tracker().hashes.clone(),
            nodes,
            edges,
        }
    }

    /// Loads a GraphSnapshot from disk if it exists and matches schema version
    pub fn load_snapshot(workspace_root: &Path) -> Result<Option<GraphSnapshot>> {
        let path = Self::graph_file_path(workspace_root);
        if !path.exists() {
            return Ok(None);
        }

        // Check file size to avoid loading corrupted huge files
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > Self::MAX_GRAPH_FILE_SIZE_BYTES {
                tracing::warn!(
                    path = %path.display(),
                    size = meta.len(),
                    "Cached graph file exceeds maximum size threshold; skipping cache"
                );
                return Ok(None);
            }
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to read cached graph file");
                return Ok(None);
            }
        };

        let snapshot: GraphSnapshot = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to deserialize graph snapshot; invalidating cache");
                return Ok(None);
            }
        };

        if snapshot.version != Self::SCHEMA_VERSION {
            tracing::debug!(
                cached_version = snapshot.version,
                current_version = Self::SCHEMA_VERSION,
                "Graph cache schema version mismatch; invalidating"
            );
            return Ok(None);
        }

        Ok(Some(snapshot))
    }

    /// Saves a GraphSnapshot to disk using an atomic temp-file-and-rename pattern
    pub fn save_snapshot(snapshot: &GraphSnapshot, workspace_root: &Path) -> Result<()> {
        let dir = workspace_root.join(Self::GRAPH_DIR_NAME);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), error = %e, "Failed to create .minicode directory for graph storage");
            return Err(ContextError::Graph(format!(
                "Failed to create .minicode directory: {}",
                e
            ))
            .into());
        }

        let path = Self::graph_file_path(workspace_root);
        let tmp_path = workspace_root.join(Self::GRAPH_DIR_NAME).join(format!(
            "{}.tmp.{}",
            Self::GRAPH_FILE_NAME,
            uuid::Uuid::new_v4()
        ));

        let json = match serde_json::to_string_pretty(snapshot) {
            Ok(j) => j,
            Err(e) => {
                return Err(ContextError::Graph(format!(
                    "Failed to serialize graph snapshot: {}",
                    e
                ))
                .into());
            }
        };

        if let Err(e) = std::fs::write(&tmp_path, json) {
            return Err(ContextError::Graph(format!(
                "Failed to write temporary graph file: {}",
                e
            ))
            .into());
        }

        if let Err(e) = std::fs::rename(&tmp_path, &path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(ContextError::Graph(format!(
                "Failed to atomically replace graph file: {}",
                e
            ))
            .into());
        }

        tracing::debug!(
            path = %path.display(),
            nodes = snapshot.nodes.len(),
            edges = snapshot.edges.len(),
            "Persisted code graph snapshot to disk"
        );
        Ok(())
    }

    /// Converts a snapshot back into a full in-memory CodeGraph
    #[allow(dead_code)]
    pub fn snapshot_to_graph(snapshot: GraphSnapshot) -> Result<CodeGraph> {
        let mut graph = CodeGraph::new();
        graph.restore_from_snapshot(snapshot)?;
        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_store_save_and_load_roundtrip() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_store_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_a = temp_dir.join("lib.rs");
        std::fs::write(
            &file_a,
            "pub fn greet(name: &str) -> String { format!(\"Hello {}\", name) }",
        )
        .unwrap();

        let mut graph = CodeGraph::new();
        graph.build_graph(&temp_dir).unwrap();

        let snapshot = GraphStore::create_snapshot(&graph, &temp_dir);
        assert!(!snapshot.nodes.is_empty());

        GraphStore::save_snapshot(&snapshot, &temp_dir).unwrap();

        let loaded_opt = GraphStore::load_snapshot(&temp_dir).unwrap();
        assert!(loaded_opt.is_some());
        let loaded = loaded_opt.unwrap();
        assert_eq!(loaded.version, GraphStore::SCHEMA_VERSION);
        assert_eq!(loaded.nodes.len(), snapshot.nodes.len());
        assert_eq!(loaded.edges.len(), snapshot.edges.len());

        let restored_graph = GraphStore::snapshot_to_graph(loaded).unwrap();
        assert_eq!(
            restored_graph.graph().node_count(),
            graph.graph().node_count()
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
