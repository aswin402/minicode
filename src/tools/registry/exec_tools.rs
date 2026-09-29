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
            description: "Primary shell execution tool for project development, testing, builds (cargo, npm, pip), dev servers (e.g. python3 -m http.server, npm run dev), and git operations. Always use exec_cmd for standard development workflow commands.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command string to execute in workspace root"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Optional execution timeout in seconds (default: 30)"
                    },
                    "is_daemon": {
                        "type": "boolean",
                        "description": "Optional flag: if true (or if running a webserver like 'python3 -m http.server' or 'npm run dev'), executes as a persistent background task under MiniTask Manager instead of waiting synchronously."
                    },
                    "background": {
                        "type": "boolean",
                        "description": "Alias for is_daemon. If true, runs the command asynchronously in the background under MiniTask Manager."
                    }
                },
                "required": ["command"]
            }),
        },
        ToolSchema {
            name: "sandbox_exec".to_string(),
            description: "Execute untrusted or experimental shell scripts inside an isolated security sandbox (Bubblewrap / Landlock) with resource limits. DO NOT use sandbox_exec for running local dev servers, builds, or tests — use exec_cmd instead.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command or script to execute inside the sandbox"
                    },
                    "allow_network": {
                        "type": "boolean",
                        "description": "Whether network/socket access is permitted. If omitted, auto-permits networking for dev servers and package managers, otherwise defaults to false for security."
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
                let cmd = require_command(args, "exec_cmd")?;
                let timeout = opt_u64(args, "timeout_secs");
                let explicit_ctx = opt_u64(args, "context_window").map(|c| c as usize);
                let is_daemon = get_bool(args, "is_daemon")
                    .or_else(|| get_bool(args, "background"))
                    .unwrap_or(false);
                exec::exec_cmd_full(workspace_root, cmd, timeout, explicit_ctx, is_daemon).await
            }
            .await,
        ),
        "sandbox_exec" => Some(
            async {
                let cmd = require_command(args, "sandbox_exec")?;
                let mut policy = crate::sandbox::SandboxPolicy::default();
                if let Some(net) = get_bool(args, "allow_network") {
                    policy.allow_network = net;
                } else if crate::sandbox::is_likely_network_or_server_command(cmd) {
                    policy.allow_network = true;
                }
                if let Some(ro) = get_bool(args, "read_only") {
                    policy.read_only_workspace = ro;
                }
                if let Some(eph) = get_bool(args, "ephemeral") {
                    policy.ephemeral_overlay = eph;
                }
                policy.timeout_secs = opt_u64(args, "timeout_secs").unwrap_or_else(|| {
                    crate::tools::exec::resolve_smart_exec_timeout(cmd, None).as_secs()
                });
                if let Some(m) = opt_u64(args, "max_memory_mb") {
                    policy.max_memory_mb = Some(m);
                }
                if let Some(extra) = opt_string_map(args, "extra_env") {
                    policy.extra_env.extend(extra);
                }

                let res = crate::sandbox::run_sandboxed(workspace_root, cmd, &policy).await?;
                if res.success {
                    Ok(crate::sandbox::format_sandbox_result(&res))
                } else {
                    Err(crate::error::ToolError::CommandExec(
                        crate::sandbox::format_sandbox_result(&res),
                    )
                    .into())
                }
            }
            .await,
        ),
        _ => None,
    }
}
