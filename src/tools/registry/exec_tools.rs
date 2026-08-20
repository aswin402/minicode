use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::exec;
use crate::tools::parse_u64_param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![ToolSchema {
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
    }]
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "exec_cmd" => Some({
            let cmd = match args.get("command").and_then(|v| v.as_str()) {
                Some(c) => c,
                None => {
                    return Some(Err(ToolError::InvalidArguments {
                        name: "exec_cmd".to_string(),
                        reason: "Missing required argument 'command'".to_string(),
                    }
                    .into()));
                }
            };
            let timeout = parse_u64_param(args.get("timeout_secs"));
            exec::exec_cmd(workspace_root, cmd, timeout).await
        }),
        _ => None,
    }
}
