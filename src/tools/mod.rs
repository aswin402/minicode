pub mod browser;
pub mod category;
pub mod compactor;
pub mod compiler;
pub mod concurrency;
pub mod crawler;
pub mod diff;
pub mod exec;
pub mod fs;
pub mod github;
pub mod middleware;
pub mod minikit;
#[allow(unused_imports)]
pub use minikit as onpkg;
pub mod param;
pub mod registry;
pub mod repair;
pub mod rtk_filter;
pub mod schema_compactor;
pub mod search;
pub mod web;
pub mod web_search;

#[allow(unused_imports)]
pub use concurrency::{classify_tool, is_barrier, is_read_only, ToolSafetyLevel};

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
        let mut schemas = Vec::with_capacity(crate::constants::TOTAL_TOOL_COUNT);
        schemas.extend(registry::fs_tools::get_schemas());
        schemas.extend(registry::exec_tools::get_schemas());
        schemas.extend(registry::search_tools::get_schemas());
        schemas.extend(registry::git_tools::get_schemas());
        schemas.extend(registry::agent_tools::get_schemas());
        schemas.extend(registry::context_tools::get_schemas());
        schemas.extend(registry::web_tools::get_schemas());
        schemas.extend(registry::minikit_tools::get_schemas());
        schemas.extend(registry::minipower_tools::get_schemas());
        schemas.extend(registry::explore_tools::get_schemas());
        schemas.extend(registry::block_tools::get_schemas());
        schemas.extend(registry::dev_tools::get_schemas());
        schemas.extend(registry::vault_tools::get_schemas());
        schemas
    }

    /// Filters a list of tool schemas according to the active ToolFilterMode.
    #[allow(dead_code)]
    pub fn filter_tools(
        mode: crate::config::ToolFilterMode,
        tools: Vec<ToolSchema>,
    ) -> Vec<ToolSchema> {
        match mode {
            crate::config::ToolFilterMode::ReadOnly => tools
                .into_iter()
                .filter(|s| crate::tools::is_read_only(&s.name))
                .collect(),
            crate::config::ToolFilterMode::Standard => tools
                .into_iter()
                .filter(|s| s.name != "spawn_subagent" && s.name != "activate_tools")
                .collect(),
            crate::config::ToolFilterMode::CoreOnly => {
                let core = crate::tools::category::get_core_schemas();
                let core_names: std::collections::HashSet<String> =
                    core.into_iter().map(|s| s.name).collect();
                tools
                    .into_iter()
                    .filter(|s| core_names.contains(&s.name))
                    .collect()
            }
            crate::config::ToolFilterMode::Full | crate::config::ToolFilterMode::Dynamic => tools,
        }
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
                display_output: String::new(),
                duration_ms,
            },
            Err(err) => {
                let display_output = Self::format_minimal_display_error(tool_name, &err);
                ToolResult {
                    tool_id: tool_id.to_string(),
                    tool_name: tool_name.to_string(),
                    success: false,
                    output: format!("Error executing {}: {}", tool_name, err),
                    display_output,
                    duration_ms,
                }
            }
        }
    }

    fn format_minimal_display_error(tool_name: &str, err: &crate::error::MinicodeError) -> String {
        let err_str = err.to_string();
        if err_str.contains("EOF while parsing") || err_str.contains("Invalid JSON syntax") {
            "Arguments truncated (LLM output token ceiling reached). Write in chunks (<180 lines) using write_file(overwrite=true) then write_file(append=true); never use heredocs or generator scripts.".to_string()
        } else if let Some(idx) = err_str.find("Raw arguments preview: '") {
            let prefix = err_str[..idx].trim().trim_end_matches('.');
            format!("{}: {}", tool_name, prefix)
        } else if let Some(idx) = err_str.find("Raw arguments: '") {
            let prefix = err_str[..idx].trim().trim_end_matches('.');
            format!("{}: {}", tool_name, prefix)
        } else {
            let clean_lines: Vec<&str> = err_str
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .take(4)
                .collect();
            clean_lines.join("\n")
        }
    }

    /// Decodes a JSON string field value for `key` from a raw (potentially truncated) JSON payload.
    /// Returns `Some((decoded_value, is_complete_string))`.
    fn extract_json_string_field(raw: &str, key: &str) -> Option<(String, bool)> {
        let needle = format!("\"{}\"", key);
        let mut search_from = 0;
        while let Some(rel_pos) = raw[search_from..].find(&needle) {
            let after_key = search_from + rel_pos + needle.len();
            let rest = raw[after_key..].trim_start();
            if let Some(after_colon) = rest.strip_prefix(':') {
                let after_colon_trimmed = after_colon.trim_start();
                if let Some(str_body) = after_colon_trimmed.strip_prefix('"') {
                    let mut decoded = String::with_capacity(str_body.len().min(65536));
                    let mut chars = str_body.chars();
                    while let Some(ch) = chars.next() {
                        if ch == '"' {
                            return Some((decoded, true));
                        } else if ch == '\\' {
                            let Some(esc) = chars.next() else {
                                return Some((decoded, false));
                            };
                            match esc {
                                'n' => decoded.push('\n'),
                                'r' => decoded.push('\r'),
                                't' => decoded.push('\t'),
                                '"' => decoded.push('"'),
                                '\\' => decoded.push('\\'),
                                '/' => decoded.push('/'),
                                'b' => decoded.push('\u{0008}'),
                                'f' => decoded.push('\u{000C}'),
                                'u' => {
                                    let mut hex = String::with_capacity(4);
                                    for _ in 0..4 {
                                        if let Some(h) = chars.next() {
                                            hex.push(h);
                                        } else {
                                            return Some((decoded, false));
                                        }
                                    }
                                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                        if let Some(u_ch) = char::from_u32(code) {
                                            decoded.push(u_ch);
                                        }
                                    }
                                }
                                other => decoded.push(other),
                            }
                        } else {
                            decoded.push(ch);
                        }
                    }
                    return Some((decoded, false));
                }
            }
            search_from = after_key;
        }
        None
    }

    /// Transactionally commits all complete top-level blocks (`</section>`, `</footer>`, or `\n}\n`)
    /// when a large `write_file` call is cut off mid-stream by the LLM single-turn output token limit.
    fn try_recover_truncated_write_file(
        workspace_root: &Path,
        raw: &str,
        backup_manager: Option<&BackupManager>,
        turn_id: usize,
    ) -> Option<String> {
        if raw.len() < 3000 {
            return None;
        }
        let (decoded_content, _) = Self::extract_json_string_field(raw, "content")?;
        if decoded_content.len() < 1500 {
            return None;
        }

        let explicit_path = Self::extract_json_string_field(raw, "path")
            .or_else(|| Self::extract_json_string_field(raw, "file_path"))
            .and_then(|(p, complete)| {
                if complete && !p.trim().is_empty() {
                    Some(p.trim().to_string())
                } else {
                    None
                }
            });

        let trimmed_start = decoded_content.trim_start();
        let lower_start = trimmed_start
            .chars()
            .take(256)
            .collect::<String>()
            .to_ascii_lowercase();

        let target_path = if let Some(p) = explicit_path {
            p
        } else if lower_start.starts_with("<!doctype html")
            || lower_start.starts_with("<html")
            || lower_start.starts_with("<section")
            || (lower_start.starts_with("<!--") && decoded_content.contains("<section"))
        {
            "index.html".to_string()
        } else if lower_start.starts_with("/*")
            || lower_start.starts_with(":root")
            || lower_start.starts_with("@import")
        {
            if workspace_root.join("style.css").exists() {
                "style.css".to_string()
            } else if workspace_root.join("styles.css").exists() {
                "styles.css".to_string()
            } else if workspace_root.join("src/index.css").exists() {
                "src/index.css".to_string()
            } else {
                return None;
            }
        } else {
            return None;
        };

        let is_html = target_path.ends_with(".html") || target_path.ends_with(".htm");
        let is_css = target_path.ends_with(".css");

        let boundary_end = if is_html {
            let candidates = ["</section>", "</footer>", "</header>", "</nav>"];
            candidates
                .iter()
                .filter_map(|tag| decoded_content.rfind(tag).map(|idx| idx + tag.len()))
                .max()?
        } else if is_css {
            decoded_content.rfind("\n}\n").map(|idx| idx + 3)?
        } else {
            return None;
        };

        if boundary_end < 1000 {
            return None;
        }

        let committed = format!("{}\n", decoded_content[..boundary_end].trim_end());
        let explicit_append = raw.contains("\"append\":true") || raw.contains("\"append\": true");
        let is_html_fragment =
            is_html && !lower_start.starts_with("<!doctype") && !lower_start.starts_with("<html");
        let use_append = explicit_append || is_html_fragment;
        let use_overwrite = !use_append;

        if let Some(bm) = backup_manager {
            let _ =
                bm.create_checkpoint(workspace_root, &workspace_root.join(&target_path), turn_id);
        }

        fs::write_file_with_options(
            workspace_root,
            &target_path,
            &committed,
            use_append,
            use_overwrite,
        )
        .ok()?;

        // Immediately reconcile any completed tasks/sections in WorkingMemory
        let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
        let _ = wm.reconcile_workspace_tasks(std::slice::from_ref(&target_path));

        let committed_lines: Vec<&str> = committed.lines().collect();
        let total_lines = committed_lines.len();
        let tail_preview = committed_lines
            .iter()
            .rev()
            .take(6)
            .rev()
            .copied()
            .collect::<Vec<_>>()
            .join("\n");

        Some(format!(
            "⚡ Partial Write Committed to `{}` ({} lines, {} bytes):\n\
             Your tool call hit the single-turn output token ceiling (~{} bytes), so minicode transactionally saved all complete blocks up through the last complete block boundary.\n\
             • Last committed lines:\n{}\n\
             • NEXT ACTION: Call `write_file(path=\"{}\", append=true, content=\"...\")` with ONLY the remaining sections/rules after the lines shown above (plus closing `</body></html>` tags if HTML). Do NOT rewrite the sections already committed, and do NOT use `exec_cmd` heredocs or generator scripts!",
            target_path,
            total_lines,
            committed.len(),
            raw.len(),
            tail_preview,
            target_path
        ))
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
            if tool_name == "write_file" {
                if let Some(recovered_msg) = Self::try_recover_truncated_write_file(
                    workspace_root,
                    raw,
                    backup_manager,
                    turn_id,
                ) {
                    return Ok(recovered_msg);
                }
            }
            let raw_preview = if raw.chars().count() > 200 {
                format!(
                    "{}... [clamped {} bytes]",
                    crate::utils::strings::truncate_chars(raw, 200),
                    raw.len()
                )
            } else {
                raw.to_string()
            };
            let mut reason = format!("{}. Raw arguments preview: '{}'", err_msg, raw_preview);
            if err_msg.contains("EOF while parsing") || raw.len() > 8000 {
                reason.push_str(&format!(
                    "\n[Recovery]: Nothing was written because the tool-call payload (~{} bytes) exceeded the LLM single-turn output token ceiling (~12KB / 200 lines max per tool call) and was cut off mid-JSON.\n\
                     CRITICAL RECOVERY RULES:\n\
                     1. NEVER try to bypass this with `exec_cmd` heredocs (`cat << 'EOF'`) or Python/JS generator scripts — every tool call shares the exact same LLM output token ceiling and WILL fail with the same truncation error!\n\
                     2. Write the file incrementally in chunks of <=180 lines (~8-12KB) per call:\n\
                        • First chunk: `write_file(path=\"...\", overwrite=true, content=\"...\")` with the initial structure + first 3-4 sections.\n\
                        • Next chunks: `write_file(path=\"...\", append=true, content=\"...\")` to append 3-4 sections at a time until finished.",
                    raw.len()
                ));
            }
            return Err(ToolError::InvalidArguments {
                name: tool_name.to_string(),
                reason,
            }
            .into());
        }

        // 0. Dynamic Meta-Tool
        if tool_name == "activate_tools" {
            let cat_str = args
                .get("category")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "activate_tools".to_string(),
                    reason: "Missing required argument 'category'".to_string(),
                })?;
            let reason = args
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("Dynamic on-demand activation");

            if cat_str == "all" {
                return Ok(
                    "All tool categories and MCP servers successfully activated.".to_string(),
                );
            }

            if cat_str == "mcp" {
                return Ok("All connected MCP servers successfully activated.".to_string());
            }

            if let Ok(cat) = cat_str.parse::<crate::tools::category::ToolCategory>() {
                let tool_count = cat.get_schemas().len();
                return Ok(format!(
                    "Successfully activated '{}' category ({} tools: {}). Reason: {}. These tools are now active in your context.",
                    cat.name(),
                    tool_count,
                    cat.description(),
                    reason
                ));
            }

            // Check if it's an explicit MCP server request (e.g. "mcp:figma")
            if let Some(server_name) = cat_str.strip_prefix("mcp:") {
                return Ok(format!(
                    "Successfully activated MCP server '{}'. Reason: {}. Its tools are now active in your context.",
                    server_name, reason
                ));
            }

            // Check if it matches a configured MCP server in the workspace
            if let Ok(config) = crate::config::Config::load(Some(workspace_root), None) {
                if config.mcp.servers.contains_key(cat_str) {
                    return Ok(format!(
                        "Successfully activated MCP server '{}'. Reason: {}. Its tools are now active in your context.",
                        cat_str, reason
                    ));
                }
            }

            return Err(ToolError::InvalidArguments {
                name: "activate_tools".to_string(),
                reason: format!(
                    "Unknown tool category or MCP server '{}'. Available: files, exec, search, git, web, onpkg, codegraph, agent, memory, all, mcp, or mcp:<server>",
                    cat_str
                ),
            }
            .into());
        }

        // 1. Filesystem Tools
        if let Some(res) =
            registry::fs_tools::dispatch(tool_name, args, workspace_root, backup_manager, turn_id)
                .await
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

        // 8. MiniKit Stack Scaffolding & Sync Tools
        if let Some(res) = registry::minikit_tools::dispatch(tool_name, args, workspace_root).await
        {
            return res;
        }

        // 9. CodeGraph Surgical Exploration & Diff Impact Tools
        if let Some(res) = registry::explore_tools::dispatch(tool_name, args, workspace_root).await
        {
            return res;
        }

        // 10. MiniPower Autonomous Methodology & Verification Tools
        if let Some(res) =
            registry::minipower_tools::dispatch(tool_name, args, workspace_root).await
        {
            return res;
        }

        // 11. MiniBlocks UI Component & Design Token Warehouse Tools
        if let Some(res) = registry::block_tools::dispatch(tool_name, args, workspace_root).await {
            return res;
        }

        // 12. MiniDev Runtime Orchestrator Tools
        if let Some(res) = registry::dev_tools::dispatch(tool_name, args, workspace_root).await {
            return res;
        }

        // 13. MiniVault Multi-tier Agent Skills & Storage Tools
        if let Some(res) = registry::vault_tools::dispatch(tool_name, args, workspace_root).await {
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
    fn test_total_tool_count() {
        let schemas = ToolRegistry::get_tool_schemas();
        assert_eq!(schemas.len(), crate::constants::TOTAL_TOOL_COUNT);
    }

    #[test]
    fn test_tool_schemas_count() {
        let schemas = ToolRegistry::get_tool_schemas();
        assert_eq!(schemas.len(), crate::constants::TOTAL_TOOL_COUNT);
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
        assert!(names.contains(&"block_search"));
        assert!(names.contains(&"block_get"));
        assert!(names.contains(&"block_insert"));
        assert!(names.contains(&"block_save"));
        assert!(names.contains(&"block_scaffold"));
        assert!(names.contains(&"block_stats"));
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
