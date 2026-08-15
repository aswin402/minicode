pub mod compactor;
pub mod exec;
pub mod fs;
pub mod search;
pub mod web;

use crate::agent::provider::ToolSchema;
use crate::agent::types::ToolResult;
use crate::error::{Result, ToolError};
use crate::session::backup::BackupManager;
use serde_json::json;
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
    /// Returns the schemas of all 14 built-in tools (primitives, core memory, working memory) for the LLM.
    pub fn get_tool_schemas() -> Vec<ToolSchema> {
        vec![
            ToolSchema {
                name: "read_file".to_string(),
                description: "Read the contents of a file in the workspace within an optional 1-indexed line range.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file within the workspace"
                        },
                        "start_line": {
                            "type": "integer",
                            "description": "Optional 1-indexed starting line number"
                        },
                        "end_line": {
                            "type": "integer",
                            "description": "Optional 1-indexed ending line number (inclusive)"
                        }
                    },
                    "required": ["path"]
                }),
            },
            ToolSchema {
                name: "patch_file".to_string(),
                description: "Apply a precise search-and-replace block edit to a file. Provide the exact text to replace and the new content.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file to modify"
                        },
                        "search_block": {
                            "type": "string",
                            "description": "The exact unique code block in the file to replace"
                        },
                        "replace_block": {
                            "type": "string",
                            "description": "The new replacement code block"
                        }
                    },
                    "required": ["path", "search_block", "replace_block"]
                }),
            },
            ToolSchema {
                name: "write_file".to_string(),
                description: "Create a new file or completely overwrite an existing file with the provided content.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file"
                        },
                        "content": {
                            "type": "string",
                            "description": "The complete text content to write"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolSchema {
                name: "exec_cmd".to_string(),
                description: "Execute a shell command inside the sandboxed workspace environment (with timeout and environment sanitization).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command string to execute"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Optional execution timeout in seconds (default: 30)"
                        }
                    },
                    "required": ["command"]
                }),
            },
            ToolSchema {
                name: "grep_search".to_string(),
                description: "Search for regex patterns or text across workspace files respecting .gitignore.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The text or regular expression to search for"
                        },
                        "is_regex": {
                            "type": "boolean",
                            "description": "Whether to treat query as a regex (default: false)"
                        },
                        "file_pattern": {
                            "type": "string",
                            "description": "Optional glob filter for file names (e.g. '*.rs')"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolSchema {
                name: "fetch_or_browse".to_string(),
                description: "Fetch web documentation or public web pages and convert HTML to readable Markdown.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The full HTTP/HTTPS URL to fetch"
                        }
                    },
                    "required": ["url"]
                }),
            },
            ToolSchema {
                name: "remember_fact".to_string(),
                description: "Save a persistent fact, convention, or developer preference to Core Memory (survives across sessions).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Unique identifier for this memory (e.g. 'code_style', 'architecture')"
                        },
                        "value": {
                            "type": "string",
                            "description": "The fact, convention, or preference to remember"
                        },
                        "is_global": {
                            "type": "boolean",
                            "description": "Whether to store globally across all projects (~/.config/minicode/memory.json) or locally (.minicode/memory.json). Default: false (local)"
                        },
                        "category": {
                            "type": "string",
                            "enum": ["preference", "project_fact", "pattern"],
                            "description": "Category of memory (default: 'project_fact')"
                        }
                    },
                    "required": ["key", "value"]
                }),
            },
            ToolSchema {
                name: "update_fact".to_string(),
                description: "Update an existing fact or preference in Core Memory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Key of the memory to update"
                        },
                        "new_value": {
                            "type": "string",
                            "description": "The updated fact or preference text"
                        }
                    },
                    "required": ["key", "new_value"]
                }),
            },
            ToolSchema {
                name: "forget_fact".to_string(),
                description: "Remove a fact or preference from Core Memory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Key of the memory to remove"
                        }
                    },
                    "required": ["key"]
                }),
            },
            ToolSchema {
                name: "create_plan".to_string(),
                description: "Initialize an active multi-step task plan in Working Memory (.minicode/plan/task_plan.md).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Short, descriptive title of the task"
                        },
                        "steps": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Ordered list of action steps to complete the task"
                        }
                    },
                    "required": ["steps"]
                }),
            },
            ToolSchema {
                name: "read_plan".to_string(),
                description: "Read the active task plan, progress tracker, and findings from Working Memory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolSchema {
                name: "log_finding".to_string(),
                description: "Record an architectural discovery, symbol location, or observation into findings.md.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "finding": {
                            "type": "string",
                            "description": "The observation, discovery, or architectural note to record"
                        }
                    },
                    "required": ["finding"]
                }),
            },
            ToolSchema {
                name: "update_progress".to_string(),
                description: "Update the status of a specific task step in progress.md (e.g. 'Completed', 'Blocked', 'In Progress').".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "step": {
                            "type": "string",
                            "description": "The description of the step matching the task plan"
                        },
                        "status": {
                            "type": "string",
                            "description": "The status (e.g. 'Completed', 'In Progress', 'Blocked')"
                        }
                    },
                    "required": ["step"]
                }),
            },
            ToolSchema {
                name: "archive_plan".to_string(),
                description: "Archive the completed task plan and clear the active Working Memory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolSchema {
                name: "impact_analysis".to_string(),
                description: "Analyze the architectural blast radius and downstream dependencies of modifying a symbol or file.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "target": {
                            "type": "string",
                            "description": "Symbol name (e.g. 'verify_token') or relative file path (e.g. 'src/auth.rs') to analyze"
                        }
                    },
                    "required": ["target"]
                }),
            },
            ToolSchema {
                name: "locate_symbol".to_string(),
                description: "Instantly locate symbol declarations, signatures, and doc comments across the workspace without full grep scans.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "The exact or partial symbol name to locate"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of matches to return (default: 10)"
                        }
                    },
                    "required": ["name"]
                }),
            },
            ToolSchema {
                name: "repo_map".to_string(),
                description: "Generate a compact AST repository skeleton map of symbols ranked by PageRank importance.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "max_tokens": {
                            "type": "integer",
                            "description": "Optional token budget for the skeleton map (default: 1024)"
                        }
                    }
                }),
            },
        ]
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
        match tool_name {
            "read_file" => {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "read_file".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                })?;
                let start_line =
                    parse_u64_param(args.get("start_line")).and_then(|v| usize::try_from(v).ok());
                let end_line =
                    parse_u64_param(args.get("end_line")).and_then(|v| usize::try_from(v).ok());
                fs::read_file(workspace_root, path, start_line, end_line)
            }
            "write_file" => {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "write_file".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                })?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "write_file".to_string(),
                        reason: "Missing required argument 'content'".to_string(),
                    })?;

                let validated_path = crate::sandbox::path::validate_path_in_workspace(
                    workspace_root,
                    Path::new(path),
                )?;

                // Safety checkpoint before modifying
                if let Some(mgr) = backup_manager {
                    if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id)
                    {
                        tracing::warn!(path = %validated_path.display(), error = %e, "Failed to create safety checkpoint before write_file");
                    }
                }

                fs::write_file(workspace_root, path, content)
            }
            "patch_file" => {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "patch_file".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                })?;
                let search = args
                    .get("search_block")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "patch_file".to_string(),
                        reason: "Missing required argument 'search_block'".to_string(),
                    })?;
                let replace = args
                    .get("replace_block")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "patch_file".to_string(),
                        reason: "Missing required argument 'replace_block'".to_string(),
                    })?;

                let validated_path = crate::sandbox::path::validate_path_in_workspace(
                    workspace_root,
                    Path::new(path),
                )?;

                // Safety checkpoint before patching
                if let Some(mgr) = backup_manager {
                    if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id)
                    {
                        tracing::warn!(path = %validated_path.display(), error = %e, "Failed to create safety checkpoint before patch_file");
                    }
                }

                fs::patch_file(workspace_root, path, search, replace)
            }
            "exec_cmd" => {
                let cmd = args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "exec_cmd".to_string(),
                        reason: "Missing required argument 'command'".to_string(),
                    })?;
                let timeout = parse_u64_param(args.get("timeout_secs"));
                exec::exec_cmd(workspace_root, cmd, timeout).await
            }
            "grep_search" => {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "grep_search".to_string(),
                        reason: "Missing required argument 'query'".to_string(),
                    }
                })?;
                let is_regex = args
                    .get("is_regex")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let pattern = args.get("file_pattern").and_then(|v| v.as_str());
                search::grep_search(workspace_root, query, is_regex, pattern)
            }
            "fetch_or_browse" => {
                let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "fetch_or_browse".to_string(),
                        reason: "Missing required argument 'url'".to_string(),
                    }
                })?;
                web::fetch_or_browse(url).await
            }
            "remember_fact" => {
                let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "remember_fact".to_string(),
                        reason: "Missing required argument 'key'".to_string(),
                    }
                })?;
                let value = args.get("value").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "remember_fact".to_string(),
                        reason: "Missing required argument 'value'".to_string(),
                    }
                })?;
                let is_global = args
                    .get("is_global")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let cat_str = args
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("project_fact");
                let category = match cat_str {
                    "preference" => crate::context::memory::MemoryCategory::Preference,
                    "pattern" => crate::context::memory::MemoryCategory::Pattern,
                    _ => crate::context::memory::MemoryCategory::ProjectFact,
                };
                let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
                mem.remember(workspace_root, key, value, is_global, category)
                    .map(|_| {
                        format!(
                            "✔ Remembered '{}' ({})",
                            key,
                            if is_global { "global" } else { "local" }
                        )
                    })
            }
            "update_fact" => {
                let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "update_fact".to_string(),
                        reason: "Missing required argument 'key'".to_string(),
                    }
                })?;
                let new_value =
                    args.get("new_value")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "update_fact".to_string(),
                            reason: "Missing required argument 'new_value'".to_string(),
                        })?;
                let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
                mem.update(workspace_root, key, new_value).map(|updated| {
                    if updated {
                        format!("✔ Updated fact '{}'", key)
                    } else {
                        format!("ℹ Fact '{}' not found to update", key)
                    }
                })
            }
            "forget_fact" => {
                let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "forget_fact".to_string(),
                        reason: "Missing required argument 'key'".to_string(),
                    }
                })?;
                let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
                mem.forget(workspace_root, key).map(|forgotten| {
                    if forgotten {
                        format!("✔ Removed fact '{}' from memory", key)
                    } else {
                        format!("ℹ Fact '{}' not found in memory", key)
                    }
                })
            }
            "create_plan" => {
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Task Plan");
                let steps: Vec<String> = args
                    .get("steps")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| item.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "create_plan".to_string(),
                        reason: "Missing required argument 'steps'".to_string(),
                    })?;
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                wm.init_plan(title, &steps).map(|_| {
                    format!(
                        "✔ Created active task plan with {} steps in .minicode/plan/task_plan.md",
                        steps.len()
                    )
                })
            }
            "read_plan" => {
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                match wm.read_plan() {
                    Ok(Some(plan)) => Ok(plan),
                    Ok(None) => Ok("ℹ No active task plan found in .minicode/plan/".to_string()),
                    Err(e) => Err(e),
                }
            }
            "log_finding" => {
                let finding = args
                    .get("finding")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "log_finding".to_string(),
                        reason: "Missing required argument 'finding'".to_string(),
                    })?;
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                wm.append_finding(finding)
                    .map(|_| "✔ Logged observation into .minicode/plan/findings.md".to_string())
            }
            "update_progress" => {
                let step = args.get("step").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "update_progress".to_string(),
                        reason: "Missing required argument 'step'".to_string(),
                    }
                })?;
                let status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Completed");
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                wm.update_progress(step, status)
                    .map(|_| format!("✔ Updated step '{}' status to '{}'", step, status))
            }
            "archive_plan" => {
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                match wm.archive_plan() {
                    Ok(Some(archive_path)) => Ok(format!(
                        "✔ Archived completed task plan to {}",
                        archive_path.display()
                    )),
                    Ok(None) => Ok("ℹ No active task plan to archive".to_string()),
                    Err(e) => Err(e),
                }
            }
            "impact_analysis" => {
                let target = args.get("target").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "impact_analysis".to_string(),
                        reason: "Missing required argument 'target'".to_string(),
                    }
                })?;
                let mut graph = crate::context::graph::CodeGraph::new();
                graph.build_graph(workspace_root)?;
                let report = graph.get_blast_radius(target, workspace_root)?;
                Ok(report.summary)
            }
            "locate_symbol" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "locate_symbol".to_string(),
                        reason: "Missing required argument 'name'".to_string(),
                    }
                })?;
                let limit = parse_u64_param(args.get("limit")).unwrap_or(10) as usize;
                let mut index = crate::context::index::SymbolIndex::new();
                index.build_index(workspace_root)?;
                let matches = if name.contains(' ') {
                    index.search_symbols(name, limit)
                } else {
                    let mut res = index.locate_symbol(name);
                    if res.is_empty() {
                        res = index.search_symbols(name, limit);
                    }
                    res.truncate(limit);
                    res
                };
                Ok(index.format_matches(&matches, workspace_root))
            }
            "repo_map" => {
                let max_tokens = parse_u64_param(args.get("max_tokens"))
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(crate::constants::DEFAULT_MAP_TOKENS);
                let mut graph = crate::context::graph::CodeGraph::new();
                graph.build_graph(workspace_root)?;
                Ok(graph.format_repomap(workspace_root, &[], max_tokens))
            }
            unknown => Err(ToolError::NotFound {
                name: unknown.to_string(),
            }
            .into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_schemas_count() {
        let schemas = ToolRegistry::get_tool_schemas();
        assert_eq!(schemas.len(), 17);
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"patch_file"));
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
}
