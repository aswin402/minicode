use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::exec;
use crate::tools::parse_u64_param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
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
            name: "sandbox_exec".to_string(),
            description: "Execute a command inside an isolated code sandbox with configurable network access, read-only filesystem protection, ephemeral scratchpad overlay, and resource limits.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command or script to execute inside the sandbox"
                    },
                    "allow_network": {
                        "type": "boolean",
                        "description": "Whether TCP/socket network access is permitted (default: false for security)"
                    },
                    "read_only": {
                        "type": "boolean",
                        "description": "If true, enforces strict read-only access to the workspace (default: false)"
                    },
                    "ephemeral": {
                        "type": "boolean",
                        "description": "If true, executes in an ephemeral overlay where any created or modified files are discarded upon completion (default: false)"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Execution timeout in seconds (default: 30)"
                    },
                    "max_memory_mb": {
                        "type": "integer",
                        "description": "Optional virtual memory ceiling in megabytes"
                    },
                    "extra_env": {
                        "type": "object",
                        "description": "Optional custom key-value environment variables to inject into sandbox"
                    }
                },
                "required": ["command"]
            }),
        },
    ]
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
        "sandbox_exec" => Some({
            let cmd = match args.get("command").and_then(|v| v.as_str()) {
                Some(c) => c,
                None => {
                    return Some(Err(ToolError::InvalidArguments {
                        name: "sandbox_exec".to_string(),
                        reason: "Missing required argument 'command'".to_string(),
                    }
                    .into()));
                }
            };
            let mut policy = crate::sandbox::SandboxPolicy::default();
            if let Some(net) = args.get("allow_network").and_then(|v| v.as_bool()) {
                policy.allow_network = net;
            }
            if let Some(ro) = args.get("read_only").and_then(|v| v.as_bool()) {
                policy.read_only_workspace = ro;
            }
            if let Some(eph) = args.get("ephemeral").and_then(|v| v.as_bool()) {
                policy.ephemeral_overlay = eph;
            }
            if let Some(t) = parse_u64_param(args.get("timeout_secs")) {
                policy.timeout_secs = t;
            }
            if let Some(m) = parse_u64_param(args.get("max_memory_mb")) {
                policy.max_memory_mb = Some(m);
            }
            if let Some(extra) = args.get("extra_env").and_then(|v| v.as_object()) {
                for (k, val) in extra {
                    if let Some(s) = val.as_str() {
                        policy.extra_env.insert(k.clone(), s.to_string());
                    }
                }
            }

            match crate::sandbox::run_sandboxed(workspace_root, cmd, &policy).await {
                Ok(res) => Ok(crate::sandbox::format_sandbox_result(&res)),
                Err(e) => Err(e),
            }
        }),
        _ => None,
    }
}
