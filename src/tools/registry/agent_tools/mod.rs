pub mod cognitive;
pub mod dag;
pub mod reproducer;
pub mod subagents;
pub mod swarms;

use crate::agent::provider::ToolSchema;
use crate::error::Result;
use std::path::Path;

/// Aggregate schemas for all agent, reasoning, dag, and orchestration tools.
pub fn get_schemas() -> Vec<ToolSchema> {
    let mut schemas = Vec::with_capacity(34);
    schemas.extend(subagents::get_schemas());
    schemas.extend(dag::get_schemas());
    schemas.extend(cognitive::get_schemas());
    schemas.extend(reproducer::get_schemas());
    schemas.extend(swarms::get_schemas());
    schemas
}

/// Unified dispatcher routing calls across agent submodules.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    if let Some(res) = dag::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    if let Some(res) = subagents::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    if let Some(res) = swarms::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    if let Some(res) = cognitive::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    if let Some(res) = reproducer::dispatch(tool_name, args, workspace_root).await {
        return Some(res);
    }
    None
}
