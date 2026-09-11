pub mod architecture;
pub mod budget;
pub mod graph;
pub mod lsp;
pub mod memory;
pub mod wiki_skills;

use crate::agent::provider::ToolSchema;
use crate::error::Result;
use std::path::Path;

/// Aggregate schemas for all memory, planning, lsp, code graph, architecture, and context tools.
pub fn get_schemas() -> Vec<ToolSchema> {
    let mut schemas = Vec::with_capacity(37);
    schemas.extend(memory::get_schemas());
    schemas.extend(graph::get_schemas());
    schemas.extend(lsp::get_schemas());
    schemas.extend(wiki_skills::get_schemas());
    schemas.extend(architecture::get_schemas());
    schemas.extend(budget::get_schemas());
    schemas
}

/// Unified dispatcher routing calls across context submodules.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    if let Some(res) = memory::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    if let Some(res) = lsp::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    if let Some(res) = wiki_skills::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    if let Some(res) = architecture::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    if let Some(res) = graph::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    if let Some(res) = budget::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    None
}
