pub mod exec;
pub mod fs;
pub mod search;
pub mod web;

use crate::agent::provider::ToolSchema;
use crate::agent::types::ToolResult;
use crate::error::ToolError;
use crate::session::backup::BackupManager;
use serde_json::json;
use std::path::Path;
use std::time::Instant;

pub struct ToolRegistry;

impl ToolRegistry {
    /// Returns the schemas of all 6 high-precision coding primitives for the LLM.
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

        let result = match tool_name {
            "read_file" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let start_line = args
                    .get("start_line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let end_line = args
                    .get("end_line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                fs::read_file(workspace_root, path, start_line, end_line)
            }
            "write_file" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                // Safety checkpoint before modifying
                if let Some(mgr) = backup_manager {
                    let target_path = workspace_root.join(path);
                    mgr.create_checkpoint(workspace_root, &target_path, turn_id)
                        .ok();
                }

                fs::write_file(workspace_root, path, content)
            }
            "patch_file" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let search = args
                    .get("search_block")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let replace = args
                    .get("replace_block")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                // Safety checkpoint before patching
                if let Some(mgr) = backup_manager {
                    let target_path = workspace_root.join(path);
                    mgr.create_checkpoint(workspace_root, &target_path, turn_id)
                        .ok();
                }

                fs::patch_file(workspace_root, path, search, replace)
            }
            "exec_cmd" => {
                let cmd = args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let timeout = args.get("timeout_secs").and_then(|v| v.as_u64());
                exec::exec_cmd(workspace_root, cmd, timeout).await
            }
            "grep_search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let is_regex = args
                    .get("is_regex")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let pattern = args.get("file_pattern").and_then(|v| v.as_str());
                search::grep_search(workspace_root, query, is_regex, pattern)
            }
            "fetch_or_browse" => {
                let url = args.get("url").and_then(|v| v.as_str()).unwrap_or_default();
                web::fetch_or_browse(url).await
            }
            unknown => Err(ToolError::NotFound {
                name: unknown.to_string(),
            }
            .into()),
        };

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_schemas_count() {
        let schemas = ToolRegistry::get_tool_schemas();
        assert_eq!(schemas.len(), 6);
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"patch_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"exec_cmd"));
        assert!(names.contains(&"grep_search"));
        assert!(names.contains(&"fetch_or_browse"));
    }
}
