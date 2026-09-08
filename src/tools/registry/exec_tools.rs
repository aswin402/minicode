use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::tools::exec;
use crate::tools::param::*;
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
        "exec_cmd" => Some(
            async {
                let cmd = require_str(args, "command", "exec_cmd")?;
                let timeout = opt_u64(args, "timeout_secs");
                exec::exec_cmd(workspace_root, cmd, timeout).await
            }
            .await,
        ),
        "sandbox_exec" => Some(
            async {
                let cmd = require_str(args, "command", "sandbox_exec")?;
                let mut policy = crate::sandbox::SandboxPolicy::default();
                if let Some(net) = get_bool(args, "allow_network") {
                    policy.allow_network = net;
                }
                if let Some(ro) = get_bool(args, "read_only") {
                    policy.read_only_workspace = ro;
                }
                if let Some(eph) = get_bool(args, "ephemeral") {
                    policy.ephemeral_overlay = eph;
                }
                if let Some(t) = opt_u64(args, "timeout_secs") {
                    policy.timeout_secs = t;
                }
                if let Some(m) = opt_u64(args, "max_memory_mb") {
                    policy.max_memory_mb = Some(m);
                }
                if let Some(extra) = args.get("extra_env").and_then(|v| v.as_object()) {
                    for (k, val) in extra {
                        if let Some(s) = val.as_str() {
                            policy.extra_env.insert(k.clone(), s.to_string());
                        }
                    }
                }

                let res = crate::sandbox::run_sandboxed(workspace_root, cmd, &policy).await?;
                Ok(crate::sandbox::format_sandbox_result(&res))
            }
            .await,
        ),
        _ => None,
    }
}
