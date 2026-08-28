pub mod browser;
pub mod compactor;
pub mod crawler;
pub mod diff;
pub mod exec;
pub mod fs;
pub mod github;
pub mod middleware;
pub mod onpkg;
pub mod registry;
pub mod rtk_filter;
pub mod search;
pub mod web;
pub mod web_search;

use crate::agent::provider::ToolSchema;
use crate::agent::types::ToolResult;
use crate::error::{Result, ToolError};
use crate::session::backup::BackupManager;
use std::path::Path;
use std::time::Instant;

/// Robust numeric parser that handles both JSON numbers and stringified integers
pub fn parse_u64_param(value: Option<&serde_json::Value>) -> Option<u64> {
    value.and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
    })
}

pub struct ToolRegistry;

impl ToolRegistry {
    /// Returns the schemas of all built-in tools partitioned across domain submodules.
    pub fn get_tool_schemas() -> Vec<ToolSchema> {
        let mut schemas = Vec::with_capacity(65);
        schemas.extend(registry::fs_tools::get_schemas());
        schemas.extend(registry::exec_tools::get_schemas());
        schemas.extend(registry::search_tools::get_schemas());
        schemas.extend(registry::git_tools::get_schemas());
        schemas.extend(registry::agent_tools::get_schemas());
        schemas.extend(registry::context_tools::get_schemas());
        schemas.extend(registry::web_tools::get_schemas());
        schemas.extend(registry::onpkg_tools::get_schemas());
        schemas
    }

    /// Dispatches and executes a tool call by name with safety checkpointing.
    pub async fn dispatch(
        workspace_root: &Path,
        tool_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        backup_manager: Option<&BackupManager>,
        turn_id: usize,
    ) -> ToolResult {
        let start = Instant::now();

        let result =
            Self::dispatch_tool(workspace_root, tool_name, args, backup_manager, turn_id).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => ToolResult {
                tool_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                success: true,
                output,
                duration_ms,
            },
            Err(err) => ToolResult {
                tool_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                success: false,
                output: format!("Error executing {}: {}", tool_name, err),
                duration_ms,
            },
        }
    }

    async fn dispatch_tool(
        workspace_root: &Path,
        tool_name: &str,
        args: &serde_json::Value,
        backup_manager: Option<&BackupManager>,
        turn_id: usize,
    ) -> Result<String> {
        if let Some(err_msg) = args.get("__json_parse_error").and_then(|v| v.as_str()) {
            let raw = args
                .get("__raw")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return Err(ToolError::InvalidArguments {
                name: tool_name.to_string(),
                reason: format!("{}. Raw arguments: '{}'", err_msg, raw),
            }
            .into());
        }

        // 1. Filesystem Tools
        if let Some(res) =
            registry::fs_tools::dispatch(tool_name, args, workspace_root, backup_manager, turn_id)
        {
            return res;
        }

        // 2. Command Execution Tools
        if let Some(res) = registry::exec_tools::dispatch(tool_name, args, workspace_root).await {
            return res;
        }

        // 3. Search & AST Tools
        if let Some(res) = registry::search_tools::dispatch(tool_name, args, workspace_root) {
            return res;
        }

        // 4. Git Tools
        if let Some(res) = registry::git_tools::dispatch(tool_name, args, workspace_root).await {
            return res;
        }

        // 5. Multi-Agent & Reasoning Tools
        if let Some(res) = registry::agent_tools::dispatch(tool_name, args, workspace_root).await {
            return res;
        }

        // 6. Context, Memory & Skill Tools
        if let Some(res) = registry::context_tools::dispatch(tool_name, args, workspace_root).await
        {
            return res;
        }

        // 7. Web & Browser Tools
        if let Some(res) = registry::web_tools::dispatch(tool_name, args, workspace_root).await {
            return res;
        }

        // 8. onpkg Stack Scaffolding & Sync Tools
        if let Some(res) = registry::onpkg_tools::dispatch(tool_name, args, workspace_root).await {
            return res;
        }

        Err(ToolError::NotFound {
            name: tool_name.to_string(),
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_schemas_count() {
        let schemas = ToolRegistry::get_tool_schemas();
        assert_eq!(schemas.len(), 84);
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"score_task_complexity"));
        assert!(names.contains(&"check_architecture"));
        assert!(names.contains(&"ast_diff"));
        assert!(names.contains(&"explore_hypotheses"));
        assert!(names.contains(&"evaluate_branch"));
        assert!(names.contains(&"select_best_branch"));
        assert!(names.contains(&"prune_context"));
        assert!(names.contains(&"ast_query"));
        assert!(names.contains(&"ast_extract_symbol"));
        assert!(names.contains(&"semantic_search"));
        assert!(names.contains(&"create_skill"));
        assert!(names.contains(&"list_skills"));
        assert!(names.contains(&"inspect_skill"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"patch_file"));
        assert!(names.contains(&"lsp_diagnostics"));
        assert!(names.contains(&"create_task_dag"));
        assert!(names.contains(&"get_next_task"));
        assert!(names.contains(&"complete_task"));
        assert!(names.contains(&"critic_review"));
        assert!(names.contains(&"browser_navigate"));
        assert!(names.contains(&"browser_snapshot"));
        assert!(names.contains(&"sequential_thinking"));
        assert!(names.contains(&"wiki_write"));
        assert!(names.contains(&"wiki_read"));
        assert!(names.contains(&"wiki_search"));
        assert!(names.contains(&"lsp_goto_definition"));
        assert!(names.contains(&"lsp_find_references"));
        assert!(names.contains(&"search_web"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"exec_cmd"));
        assert!(names.contains(&"grep_search"));
        assert!(names.contains(&"fetch_or_browse"));
        assert!(names.contains(&"remember_fact"));
        assert!(names.contains(&"update_fact"));
        assert!(names.contains(&"forget_fact"));
        assert!(names.contains(&"create_plan"));
        assert!(names.contains(&"read_plan"));
        assert!(names.contains(&"log_finding"));
        assert!(names.contains(&"update_progress"));
        assert!(names.contains(&"archive_plan"));
        assert!(names.contains(&"impact_analysis"));
        assert!(names.contains(&"locate_symbol"));
        assert!(names.contains(&"repo_map"));
        assert!(names.contains(&"git_status"));
        assert!(names.contains(&"git_diff"));
        assert!(names.contains(&"git_commit"));
        assert!(names.contains(&"git_log"));
        assert!(names.contains(&"git_conflicts"));
        assert!(names.contains(&"create_pr"));
        assert!(names.contains(&"delegate_task"));
    }

    #[test]
    fn test_parse_u64_param() {
        let num_val = json!(42);
        assert_eq!(parse_u64_param(Some(&num_val)), Some(42));

        let str_val = json!("100");
        assert_eq!(parse_u64_param(Some(&str_val)), Some(100));

        let invalid_str = json!("abc");
        assert_eq!(parse_u64_param(Some(&invalid_str)), None);

        assert_eq!(parse_u64_param(None), None);
    }

    #[tokio::test]
    async fn test_dispatch_invalid_json_arguments() {
        let root = Path::new(".");
        let args = json!({
            "__json_parse_error": "Invalid syntax at line 1 column 4",
            "__raw": "{\"a\":"
        });
        let res = ToolRegistry::dispatch(root, "call_1", "read_file", &args, None, 1).await;
        assert!(!res.success);
        assert!(res.output.contains("Invalid syntax"));
        assert!(res.output.contains("Raw arguments"));
    }
}
