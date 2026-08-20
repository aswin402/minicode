use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::session::backup::BackupManager;
use crate::tools::fs;
use crate::tools::parse_u64_param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
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
    ]
}

pub fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
    backup_manager: Option<&BackupManager>,
    turn_id: usize,
) -> Option<Result<String>> {
    match tool_name {
        "read_file" => Some((|| {
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
        })()),
        "write_file" => Some((|| {
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

            let validated_path =
                crate::sandbox::path::validate_path_in_workspace(workspace_root, Path::new(path))?;

            // Safety checkpoint before modifying
            if let Some(mgr) = backup_manager {
                if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id) {
                    tracing::warn!(path = %validated_path.display(), error = %e, "Failed to create safety checkpoint before write_file");
                }
            }

            fs::write_file(workspace_root, path, content)
        })()),
        "patch_file" => Some((|| {
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

            let validated_path =
                crate::sandbox::path::validate_path_in_workspace(workspace_root, Path::new(path))?;

            // Safety checkpoint before patching
            if let Some(mgr) = backup_manager {
                if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id) {
                    tracing::warn!(path = %validated_path.display(), error = %e, "Failed to create safety checkpoint before patch_file");
                }
            }

            fs::patch_file(workspace_root, path, search, replace)
        })()),
        _ => None,
    }
}
