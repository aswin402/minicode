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
            description: "Primary shell execution tool for project development, testing, builds (bun, cargo, npm, pytest, go, make, etc.), dev servers (e.g. python3 -m http.server, npm run dev), and git operations. Commands that do not finish within `wait_ms_before_async` automatically continue running in the background as tracked tasks in `minitask` (`MiniDevRegistry`), and the system notifies you when they complete.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command string to execute. Already executes in the workspace root by default — do NOT prepend 'cd <workspace>' or hardcode absolute paths."
                    },
                    "wait_ms_before_async": {
                        "type": "integer",
                        "description": "Milliseconds to wait for the command to finish synchronously before promoting it to an async background task in `minitask` (500 to 120000). Set to 500-2000 for commands intended to run in the background (heavy builds, test suites, installs) or 5000-10000 for commands expected to finish quickly."
                    },
                    "run_in_background": {
                        "type": "boolean",
                        "description": "If true, runs the non-daemon command (build, compile, test, bundle, install) asynchronously in `minitask` after a brief 500ms startup window and notifies you upon completion."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Optional execution timeout in seconds (default: 30)"
                    },
                    "is_daemon": {
                        "type": "boolean",
                        "description": "Set to true ONLY for long-running persistent servers/watchers meant to stay running indefinitely across turns (e.g., dev servers, file watchers, tunnels). Leave false (the default) for builds, tests, installs, and normal commands that are expected to terminate."
                    },
                    "background": {
                        "type": "boolean",
                        "description": "Alias for is_daemon."
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
                    .or_else(|| get_bool(args, "IsDaemon"))
                    .or_else(|| get_bool(args, "background"))
                    .unwrap_or(false);
                let wait_ms_before_async = opt_u64(args, "wait_ms_before_async")
                    .or_else(|| opt_u64(args, "WaitMsBeforeAsync"));
                let run_in_background = get_bool(args, "run_in_background");
                let mut res = exec::exec_cmd_with_async_options(
                    workspace_root,
                    cmd,
                    timeout,
                    explicit_ctx,
                    is_daemon,
                    wait_ms_before_async,
                    run_in_background,
                )
                .await;
                if let Ok(ref mut out) = res {
                    if out.contains(": not found") || out.contains("command not found") {
                        if let Some(hint) = detect_native_tool_in_shell_cmd(cmd) {
                            out.push_str(&hint);
                        }
                    }
                }
                res
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

/// Data-driven check: when a shell command fails with `command not found` (exit code 127),
/// scans tokens in `cmd` against the registered `ToolCategory::ALL` schemas to detect if the
/// model accidentally tried to run a built-in `minicode` tool inside `sh`.
fn detect_native_tool_in_shell_cmd(cmd: &str) -> Option<String> {
    let mut native_tools = std::collections::HashSet::new();
    for cat in crate::tools::category::ToolCategory::ALL {
        for schema in cat.get_schemas() {
            native_tools.insert(schema.name);
        }
    }
    for token in cmd.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if token.contains('_') && native_tools.contains(token) {
            return Some(format!(
                "\n\n💡 [Native Tool Hint]: `{}` is a built-in minicode tool, NOT a shell binary — invoke `{}` directly as a structured tool call instead of running it inside `exec_cmd`.",
                token, token
            ));
        }
    }
    None
}
