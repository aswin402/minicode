use crate::context::graph::CodeGraph;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

/// Detailed statistics from an incremental or full CodeGraph synchronization run
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphSyncStats {
    pub files_scanned: usize,
    pub files_modified: usize,
    pub files_added: usize,
    pub files_deleted: usize,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub sync_latency_ms: u128,
    pub was_full_rebuild: bool,
}

impl GraphSyncStats {
    /// Formats the synchronization summary into a markdown scorecard
    pub fn format_markdown(&self) -> String {
        let mode_badge = if self.was_full_rebuild {
            "🔄 Full Rebuild"
        } else {
            "⚡ Incremental Delta Sync"
        };

        format!(
            "# 🔄 AST CodeGraph Synchronization Report\n\n\
            **Status:** Completed successfully in **{} ms** ({})\n\n\
            ### 📊 Graph Metrics\n\
            - **Total Graph Nodes:** {}\n\
            - **Total Dependency Edges:** {}\n\
            - **Files Scanned:** {}\n\
            - **Modified / Dirty Files Synced:** {}\n\
            - **New Files Added:** {}\n\
            - **Deleted Files Pruned:** {}\n",
            self.sync_latency_ms,
            mode_badge,
            self.total_nodes,
            self.total_edges,
            self.files_scanned,
            self.files_modified,
            self.files_added,
            self.files_deleted
        )
    }
}

pub struct GraphSynchronizer;

impl GraphSynchronizer {
    /// Synchronizes the workspace CodeGraph incrementally or via full cold-start rebuild
    pub fn sync(
        workspace_root: &Path,
        _target_file: Option<&str>,
        force_full: bool,
    ) -> Result<GraphSyncStats> {
        let start = Instant::now();
        let mut graph = CodeGraph::new();

        let files_scanned;
        let files_modified;
        let files_added;
        let files_deleted;
        let was_full_rebuild;

        if force_full {
            graph.full_rebuild(workspace_root)?;
            let _ = graph.save_to_disk(workspace_root);
            was_full_rebuild = true;
            files_scanned = graph.file_count();
            files_modified = 0;
            files_added = files_scanned;
            files_deleted = 0;
        } else {
            let had_cache = graph.load_cached(workspace_root);
            if had_cache {
                let inc = graph.incremental_update(workspace_root)?;
                let _ = graph.save_to_disk(workspace_root);
                was_full_rebuild = false;
                files_scanned = inc.files_scanned;
                files_modified = inc.files_reparsed;
                files_added = inc.nodes_added;
                files_deleted = inc.files_removed;
            } else {
                graph.full_rebuild(workspace_root)?;
                let _ = graph.save_to_disk(workspace_root);
                was_full_rebuild = true;
                files_scanned = graph.file_count();
                files_modified = 0;
                files_added = files_scanned;
                files_deleted = 0;
            }
        }

        let latency = start.elapsed().as_millis();
        let total_nodes = graph.graph().node_count();
        let total_edges = graph.graph().edge_count();

        Ok(GraphSyncStats {
            files_scanned,
            files_modified,
            files_added,
            files_deleted,
            total_nodes,
            total_edges,
            sync_latency_ms: latency,
            was_full_rebuild,
        })
    }
}
