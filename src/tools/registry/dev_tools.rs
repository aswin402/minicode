//! MiniTask Process Vault Tool (Tool 168: `minitask`).
//! Provides unified lifecycle management (CRUD), port auto-discovery,
//! process group isolation, and resource telemetry for development servers,
//! backends, Docker containers, background scripts, workers, and browser sessions.

use crate::agent::provider::ToolSchema;
use crate::dev::models::{DevProcessId, DevProcessType, ScheduleRequest, SpawnDevRequest};
use crate::dev::registry::get_global_dev_registry;
use crate::error::{DevError, Result, ToolError};
use crate::tools::param::*;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Returns the schema for `minitask` (Tool 168).
pub fn get_schemas() -> Vec<ToolSchema> {
    vec![ToolSchema {
        name: "minitask".to_string(),
        description: "Unified Task, Process & MiniPub Subagent Vault (`minitask`). Supervises long-running development servers, web apps, scheduled tasks, and gives the Orchestrator full CRUD control over `MiniPub` (`minipub`) subagents and dynamic timeouts. When to use: (1) Call `minitask(action='start', script='dev')` to launch and supervise the workspace dev server before running `browser_navigate` or `browser_screenshot` (dev servers MUST remain running across turns). (2) Call `minitask(action='spawn_minipub', role='coder', prompt='...', todos=[...], file_boundaries=[...], timeout_secs=600)` to spawn a background MiniPub worker, `minitask(action='workers')` or `minitask(action='status', id='coder-1')` to inspect live workers, `minitask(action='update_minipub', id='coder-1', timeout_secs=900, steer_prompt='...', append_todos=[...])` to dynamically modify a running MiniPub worker or tune Orchestrator `stream_idle_timeout_secs` / `max_parallel` in-flight, and `minitask(action='kill', id='coder-1')` to terminate a worker.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "start", "scripts", "targets", "schedule", "list", "ps", "status", "logs",
                        "stop", "kill", "restart", "resources", "kill_all", "stop_all",
                        "probe_port", "check_port", "workers", "minipubs", "swarms",
                        "spawn_minipub", "create_minipub", "update_minipub", "modify_minipub", "configure",
                        "await", "wait", "transcript", "drilldown",
                        "screenshot", "audit"
                    ],
                    "description": "Lifecycle / CRUD action: 'start' (launch dev server or script target), 'spawn_minipub'/'create_minipub' (Create a background MiniPub subagent worker), 'workers'/'minipubs'/'list'/'status'/'logs'/'transcript' (Read active processes & MiniPub workers), 'await'/'wait' (Wait for one or all MiniPub workers with wait_mode='all'|'any'), 'update_minipub'/'modify_minipub'/'configure' (Update a running MiniPub worker's timeout, todos, file_boundaries, token_budget, max_turns, or inject a live steer_prompt, or dynamically adjust Orchestrator stream_idle_timeout_secs / max_parallel), 'stop'/'kill'/'kill_all' (Delete/terminate process or MiniPub worker), 'restart' (cycle process), 'resources' (CPU/RAM telemetry), 'probe_port' (check port availability), 'schedule' (register recurring task)."
                },
                "script": {
                    "type": "string",
                    "description": "Target script name to run from package.json, justfile, Makefile, or Cargo.toml (e.g. 'dev', 'start', 'serve'). Optional for action='start' (defaults to workspace dev server)."
                },
                "command": {
                    "type": "string",
                    "description": "Explicit shell command to run (e.g. 'bun run dev', 'cargo run'). Optional if 'script' is used."
                },
                "name": {
                    "type": "string",
                    "description": "Human-readable label for the process (e.g. 'frontend-dev')"
                },
                "process_type": {
                    "type": "string",
                    "enum": ["frontend", "backend", "docker", "script", "worker", "cron", "timer"],
                    "description": "Category of process (default: 'frontend')"
                },
                "id": {
                    "type": "string",
                    "description": "Process or MiniPub worker ID (e.g. 'coder-1', 'scout-2', 'dev-1') for 'status', 'logs', 'await', 'transcript', 'update_minipub', 'stop', 'kill', 'restart'"
                },
                "ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of MiniPub worker IDs to watch when action='await' (if omitted along with 'id', watches all active workers)"
                },
                "wait_mode": {
                    "type": "string",
                    "enum": ["all", "any", "first", "race"],
                    "description": "Join policy when action='await': 'all' (default) waits for all watched workers; 'any'/'first'/'race' wakes immediately as soon as the first watched worker finishes"
                },
                "role": {
                    "type": "string",
                    "enum": [
                        "scout", "architect", "planner", "designer", "frontend", "backend",
                        "coder", "vision", "content_writer", "writer", "debugger", "reviewer",
                        "security", "tester", "simplifier",
                        "ultrabrain", "visual-engineering", "quick", "explore", "librarian",
                        "researcher", "code_reviewer", "test_engineer", "security_auditor"
                    ],
                    "description": "Specialized MiniPub subagent role or semantic category for 'spawn_minipub' (default: 'coder')"
                },
                "category": {
                    "type": "string",
                    "description": "Optional semantic intent category ('ultrabrain', 'visual-engineering', 'quick', 'explore', 'librarian', 'security-audit', 'test-verification') for 'spawn_minipub'"
                },
                "load_skills": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of specialized MiniVault skill names to inject into the MiniPub worker (for 'spawn_minipub')"
                },
                "prompt": {
                    "type": "string",
                    "description": "Task instructions for 'spawn_minipub' or live in-flight steering prompt for 'update_minipub'"
                },
                "steer_prompt": {
                    "type": "string",
                    "description": "Live in-flight steering instruction to inject into a running MiniPub worker's next turn without restarting it (for 'update_minipub')"
                },
                "todos": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Boomerang todo checklist items assigned to the MiniPub worker (for 'spawn_minipub' or 'update_minipub')"
                },
                "append_todos": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Additional todo items to append to a running MiniPub worker's checklist (for 'update_minipub')"
                },
                "complete_todo_ids": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "1-indexed todo item IDs to mark completed [x] on a MiniPub worker (for 'update_minipub')"
                },
                "file_boundaries": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Allowed relative file/directory paths for cooperative write isolation (for 'spawn_minipub' or 'update_minipub')"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Dynamic activity-heartbeat timeout in seconds for a MiniPub worker (e.g. 600, 900, 1800) or default MiniPub timeout when called without 'id'"
                },
                "stream_idle_timeout_secs": {
                    "type": "integer",
                    "description": "Dynamic model stream idle timeout in seconds for the Orchestrator (e.g. 600, 900, 1800) when called with 'update_minipub' or 'configure'"
                },
                "max_parallel": {
                    "type": "integer",
                    "description": "Dynamic maximum parallel MiniPub workers (1..=16, default: 4) for the SubagentPool"
                },
                "token_budget": {
                    "type": "integer",
                    "description": "Token budget ceiling for a MiniPub worker (for 'spawn_minipub' or 'update_minipub')"
                },
                "max_turns": {
                    "type": "integer",
                    "description": "Maximum tool-calling turns for a MiniPub worker (for 'spawn_minipub' or 'update_minipub')"
                },
                "isolate_worktree": {
                    "type": "boolean",
                    "description": "Whether to isolate the MiniPub worker in a dedicated .minicode/worktrees/<id> branch"
                },
                "port": {
                    "type": "integer",
                    "description": "Target port number (for 'probe_port' or expected dev server port)"
                },
                "url": {
                    "type": "string",
                    "description": "Target URL for 'screenshot' or 'audit' (optional, defaults to running server URL)"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Subdirectory relative to workspace root"
                },
                "tail": {
                    "type": "integer",
                    "description": "Number of recent log lines to return for 'logs' (default: 50)"
                },
                "filter": {
                    "type": "string",
                    "description": "Optional substring filter for 'logs'"
                },
                "interval_seconds": {
                    "type": "integer",
                    "description": "Interval in seconds for 'schedule' (default: 60)"
                },
                "duration_seconds": {
                    "type": "integer",
                    "description": "One-shot timer delay in seconds for 'schedule'"
                },
                "cron": {
                    "type": "string",
                    "description": "Cron expression for 'schedule'"
                }
            },
            "required": ["action"]
        }),
    }]
}

fn parse_browser_mode(args: &serde_json::Value) -> crate::tools::browser::BrowserMode {
    let mode_str = opt_str(args, "mode").unwrap_or("headless");
    match mode_str {
        "gui" => crate::tools::browser::BrowserMode::Gui,
        _ => crate::tools::browser::BrowserMode::Headless,
    }
}

/// Dispatches execution of the `minitask` tool.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    if tool_name != "minitask" {
        return None;
    }

    Some(async move {
        let raw_action = require_str(args, "action", "minitask")?;
        let action = raw_action
            .split("</parameter>")
            .next()
            .unwrap_or(raw_action)
            .trim();
        let registry = get_global_dev_registry();

        match action {
            "probe_port" | "check_port" => {
                let port = require_u64(args, "port", "minitask")? as u16;
                let is_listening = crate::dev::ports::is_port_listening(port);
                let (conflicting_pid, process_name, command_line) = if is_listening {
                    let pid = crate::dev::ports::find_pid_by_port(port);
                    let (comm, cmd) = if let Some(p) = pid {
                        crate::dev::ports::get_process_info(p)
                    } else {
                        (None, None)
                    };
                    (pid, comm, cmd)
                } else {
                    (None, None, None)
                };
                let suggested = if is_listening {
                    crate::dev::ports::find_next_available_port(
                        port + 1,
                        crate::constants::DEFAULT_PORT_SCAN_RANGE,
                    )
                } else {
                    Some(port)
                };

                let status_str = if is_listening { "OCCUPIED" } else { "AVAILABLE" };
                let pid_str = conflicting_pid
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let proc_str = process_name.as_deref().unwrap_or("unknown");
                let cmd_str = command_line.as_deref().unwrap_or("-");
                let sugg_str = suggested
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "none".to_string());

                let result_json = serde_json::json!({
                    "port": port,
                    "status": status_str,
                    "is_listening": is_listening,
                    "conflicting_pid": conflicting_pid,
                    "process_name": process_name,
                    "command_line": command_line,
                    "suggested_available_port": suggested,
                });

                if is_listening {
                    Ok(format!(
                        "ℹ️ Port {} is ACTIVE / OCCUPIED:\n• Process: {} (PID {})\n• Command: {}\n• Readiness: If this process is your intended dev server or web app, it is actively listening on port {}. You can navigate or curl this port directly.\n• Next Available Port: {} (if you wish to launch a new, separate service)\n\nJSON:\n{}",
                        port, proc_str, pid_str, cmd_str, port, sugg_str, result_json
                    ))
                } else {
                    Ok(format!(
                        "✔ Port {} is AVAILABLE (ready for binding)\n\nJSON:\n{}",
                        port, result_json
                    ))
                }
            }
            "scripts" | "targets" => {
                let working_dir = opt_str(args, "working_dir")
                    .or_else(|| opt_str(args, "dir"))
                    .or_else(|| opt_str(args, "path"))
                    .map(std::path::PathBuf::from);
                let target_dir = if let Some(ref wd) = working_dir {
                    workspace_root.join(wd)
                } else {
                    workspace_root.to_path_buf()
                };
                let catalog = crate::dev::WorkspaceScriptDiscovery::discover(&target_dir);
                Ok(crate::dev::WorkspaceScriptDiscovery::format_catalog(&catalog))
            }
            "start" => {
                let working_dir = opt_str(args, "working_dir")
                    .or_else(|| opt_str(args, "dir"))
                    .or_else(|| opt_str(args, "path"))
                    .map(std::path::PathBuf::from);
                let target_dir = if let Some(ref wd) = working_dir {
                    workspace_root.join(wd)
                } else {
                    workspace_root.to_path_buf()
                };

                let script_opt = opt_str(args, "script");
                let command = if let Some(cmd) = opt_str(args, "command") {
                    cmd.to_string()
                } else if let Some(sname) = script_opt {
                    crate::dev::WorkspaceScriptDiscovery::find_script(&target_dir, sname)
                        .map(|s| s.command)
                        .ok_or_else(|| {
                            ToolError::invalid_args(
                                "minitask",
                                format!(
                                    "Script target '{}' not found in workspace (checked justfile, package.json, Makefile, Cargo.toml). Run minitask(action=\"scripts\") to discover all targets.",
                                    sname
                                ),
                            )
                        })?
                } else if let Some(dev_script) = crate::dev::WorkspaceScriptDiscovery::find_dev_server(&target_dir) {
                    dev_script.command
                } else {
                    return Err(ToolError::invalid_args(
                        "minitask",
                        "Action 'start' requires either 'command' or 'script' (or a discoverable dev server in justfile, package.json, Makefile, Cargo.toml). Run minitask(action=\"scripts\") to view all targets.",
                    ).into());
                };

                let name = opt_str(args, "name")
                    .map(|s| s.to_string())
                    .or_else(|| script_opt.map(|s| s.to_string()));
                let p_type_str = opt_str(args, "process_type").unwrap_or("frontend");
                let process_type: DevProcessType = p_type_str.parse().unwrap_or(DevProcessType::Frontend);
                let port_hint = opt_u64(args, "port_hint").map(|p| p as u16);

                let port_policy_str = opt_str(args, "port_policy").unwrap_or("fallback");
                let port_policy = crate::dev::models::PortConflictPolicy::from_str_loose(port_policy_str);

                let mut extra_env = HashMap::new();
                if let Some(env_obj) = args.get("env").and_then(|v| v.as_object()) {
                    for (k, v) in env_obj {
                        if let Some(s) = v.as_str() {
                            extra_env.insert(k.clone(), s.to_string());
                        } else {
                            extra_env.insert(k.clone(), v.to_string());
                        }
                    }
                }

                let auto_restart = opt_bool(args, "auto_restart", false);
                let restart_policy = if auto_restart {
                    Some(crate::dev::models::RestartPolicy::on_failure_default())
                } else if let Some(rp_str) = opt_str(args, "restart_policy") {
                    let max_retries = opt_u64(args, "max_retries").unwrap_or(3) as u32;
                    let backoff_ms = opt_u64(args, "backoff_ms").unwrap_or(1000);
                    match rp_str.trim().to_lowercase().as_str() {
                        "on_failure" | "on-failure" | "failure" => {
                            Some(crate::dev::models::RestartPolicy::OnFailure { max_retries, backoff_ms })
                        }
                        "always" => Some(crate::dev::models::RestartPolicy::Always { max_retries, backoff_ms }),
                        _ => Some(crate::dev::models::RestartPolicy::Never),
                    }
                } else {
                    Some(crate::dev::models::RestartPolicy::Never)
                };

                let req = SpawnDevRequest {
                    command,
                    name,
                    process_type,
                    working_dir,
                    extra_env,
                    port_hint,
                    max_memory_mb: None,
                    port_policy: Some(port_policy),
                    restart_policy,
                };

                let summary = registry.spawn(workspace_root, req).await?;

                // TCP + HTTP Readiness Probe Loop:
                // Development servers (e.g. Vite, Next.js, Astro) typically take 500ms-6500ms to bind to their port,
                // especially with modern bundlers like @tailwindcss/vite on cold start.
                // Rather than returning prematurely while still compiling/starting (which causes browser_navigate to hit ECONNREFUSED
                // or spin on an unready socket), probe localhost:port for up to 10.0s until the server accepts and responds.
                let probe_deadline = tokio::time::Instant::now() + Duration::from_millis(10000);
                let mut live_summary = summary.clone();
                let mut tcp_ready = false;
                let mut target_port = None;
                let mut resolved_host = "127.0.0.1";

                while tokio::time::Instant::now() < probe_deadline {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    if let Some(curr) = registry.get(&summary.id).await {
                        live_summary = curr;
                    }
                    // Continue probing while process is Running OR Healthy (port discovered in stdout/stderr)
                    if !live_summary.status.is_alive() {
                        break;
                    }
                    if let Some(pid) = live_summary.pid {
                        let _ = crate::dev::ports::wake_if_stopped(pid);
                    }
                    target_port = live_summary
                        .ports
                        .first()
                        .copied()
                        .or(port_hint)
                        .or_else(|| {
                            live_summary.url.as_ref().and_then(|u| {
                                u.split(':').next_back()?.trim_end_matches('/').parse::<u16>().ok()
                            })
                        });

                    if let Some(port) = target_port {
                        let is_web_process = matches!(
                            live_summary.process_type,
                            DevProcessType::Frontend | DevProcessType::Backend
                        );
                        if is_web_process {
                            if let Some(host) =
                                crate::dev::ports::resolve_http_responsive_loopback_host(port)
                            {
                                resolved_host = host;
                                tcp_ready = true;
                                break;
                            }
                        } else if let Some(host) =
                            crate::dev::ports::resolve_listening_loopback_host(port)
                        {
                            resolved_host = host;
                            tcp_ready = true;
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            break;
                        }
                    }
                }

                // Fallback: if HTTP probe timed out (e.g. non-HTTP TCP service on Frontend/Backend), check raw TCP socket
                if !tcp_ready {
                    if let Some(port) = target_port {
                        if let Some(host) = crate::dev::ports::resolve_listening_loopback_host(port)
                        {
                            resolved_host = host;
                            tcp_ready = true;
                        }
                    }
                }

                let derived_url = target_port
                    .map(|p| format!("http://{}:{}", resolved_host, p))
                    .or_else(|| live_summary.url.clone());
                let url_str = derived_url
                    .as_deref()
                    .unwrap_or("(port scanning in progress...)");

                let mut msg = format!(
                    "🚀 Development process launched successfully:\n• ID: {}\n• Name: {}\n• Type: {:?}\n• PID: {}\n• Status: {:?}\n• Primary URL: {}\n• Active Ports: {:?}",
                    live_summary.id,
                    live_summary.name,
                    live_summary.process_type,
                    live_summary.pid.unwrap_or(0),
                    live_summary.status,
                    url_str,
                    live_summary.ports,
                );

                if tcp_ready {
                    if let Some(p) = target_port {
                        msg.push_str(&format!(
                            "\n• ⚡ TCP Socket Ready: http://{}:{} is actively listening and verified responsive! (Safe for browser_navigate)",
                            resolved_host, p
                        ));
                    }
                } else if let Some(p) = target_port {
                    msg.push_str(&format!(
                        "\n• ⏳ TCP Warming Up: Port {} detected; dev server is still compiling/starting. If navigating immediately, brief delay may occur.",
                        p
                    ));
                }

                if let Some(crate::dev::models::PortResolution::Shifted { requested, resolved, conflict }) = &live_summary.port_resolution {
                    let p_str = conflict.conflicting_pid.map(|p| format!("PID {}", p)).unwrap_or_else(|| "PID unknown".to_string());
                    let c_str = conflict.process_name.as_deref().unwrap_or("process");
                    msg.push_str(&format!(
                        "\n• ⚡ Port Conflict Resolved: {} occupied by {} ({}) ➔ auto-shifted to {} (injected PORT={})",
                        requested, p_str, c_str, resolved, resolved
                    ));
                }

                if live_summary.restart_policy != crate::dev::models::RestartPolicy::Never {
                    msg.push_str(&format!(
                        "\n• 🛡 Watchdog Supervision: {:?}",
                        live_summary.restart_policy
                    ));
                }

                Ok(msg)
            }
            "schedule" => {
                let cmd = opt_str(args, "command")
                    .or_else(|| opt_str(args, "prompt"))
                    .ok_or_else(|| {
                        ToolError::invalid_args(
                            "minitask",
                            "Action 'schedule' requires either 'command' or 'prompt'",
                        )
                    })?;
                let name = opt_str(args, "name").map(|s| s.to_string());
                let interval_seconds = opt_u64(args, "interval_seconds")
                    .or_else(|| opt_u64(args, "interval"));
                let duration_seconds = opt_u64(args, "duration_seconds")
                    .or_else(|| opt_u64(args, "duration"));
                let cron_expression = opt_str(args, "cron_expression")
                    .or_else(|| opt_str(args, "cron"))
                    .map(|s| s.to_string());
                let max_iterations = opt_u64(args, "max_iterations").map(|n| n as usize);

                let req = ScheduleRequest {
                    command: cmd.to_string(),
                    name,
                    interval_seconds,
                    duration_seconds,
                    cron_expression,
                    max_iterations,
                };

                let summary = registry.schedule(workspace_root, req).await?;
                let is_one_shot = summary
                    .schedule_info
                    .as_ref()
                    .map(|s| s.is_one_shot)
                    .unwrap_or(false);
                let interval = summary
                    .schedule_info
                    .as_ref()
                    .map(|s| s.interval_secs)
                    .unwrap_or(60);

                if is_one_shot {
                    Ok(format!(
                        "⏱ One-shot timer registered successfully:\n• ID: {}\n• Name: {}\n• Type: {:?}\n• Command: {}\n• Duration: {}s\n• Status: {:?}",
                        summary.id, summary.name, summary.process_type, cmd, interval, summary.status
                    ))
                } else {
                    let max_str = summary
                        .schedule_info
                        .as_ref()
                        .and_then(|s| s.max_iterations)
                        .map(|m| format!("{} max", m))
                        .unwrap_or_else(|| "unlimited".to_string());
                    Ok(format!(
                        "⏱ Scheduled task registered successfully:\n• ID: {}\n• Name: {}\n• Type: {:?}\n• Command: {}\n• Cadence: Every {}s\n• Iterations: {}\n• Status: {:?}",
                        summary.id, summary.name, summary.process_type, cmd, interval, max_str, summary.status
                    ))
                }
            }
            "spawn_minipub" | "create_minipub" => {
                let prompt = opt_str(args, "prompt")
                    .or_else(|| opt_str(args, "command"))
                    .ok_or_else(|| {
                        ToolError::invalid_args(
                            "minitask",
                            "Action 'spawn_minipub' requires 'prompt' describing the task for the MiniPub worker",
                        )
                    })?;
                let role_str = opt_str(args, "role")
                    .or_else(|| opt_str(args, "category"))
                    .or_else(|| opt_str(args, "name"))
                    .unwrap_or("coder");
                let role = crate::agent::subagent::SubagentRole::from_str_loose(role_str);
                let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
                let (config, todos, file_boundaries) =
                    crate::tools::registry::agent_tools::subagents::build_subagent_config_from_args(
                        args, role, &app_cfg,
                    );

                if let Some(stream_secs) = opt_u64(args, "stream_idle_timeout_secs") {
                    let clamped = stream_secs.clamp(30, 7200);
                    let conn_secs = crate::config::get_active_connect_timeout_secs();
                    crate::config::set_active_provider_timeouts_override(conn_secs, clamped);
                }

                let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
                if let Some(max_p) = opt_u64(args, "max_parallel")
                    .or_else(|| opt_u64(args, "max_parallel_minipubs"))
                {
                    pool.set_max_parallel(max_p as usize);
                }

                let isolate_worktree = args
                    .get("isolate_worktree")
                    .and_then(|v| v.as_bool())
                    .unwrap_or_else(|| {
                        role.default_workspace_mode() == crate::agent::subagent::WorkspaceMode::Worktree
                    });

                let provider = pool.get_or_create_provider().await;
                let timeout_secs_disp = config.inactivity_timeout_secs;
                let max_turns_disp = config.max_turns;
                let budget_disp = config.token_budget;
                let id = pool
                    .spawn_background_worker(role, prompt, Some(config), provider, isolate_worktree)
                    .await?;

                let isolation_label = if isolate_worktree {
                    format!("Dedicated Git Worktree (`.minicode/worktrees/{}`)", id)
                } else if !file_boundaries.is_empty() {
                    format!("Cooperative File Lease (`{}`)", file_boundaries.join(", "))
                } else {
                    "Shared Read-Only Context".to_string()
                };

                Ok(format!(
                    "🤖 MiniPub worker spawned via minitask:\n\
                     • Worker ID: `{}` (also registered in `/tasks` as `worker-{}`)\n\
                     • Role: {}\n\
                     • Isolation: {}\n\
                     • Inactivity Timeout: {}s | Max Turns: {} | Token Budget: {}\n\
                     • Todos Assigned: {}\n\
                     • Pool Concurrency Cap: {} max parallel\n\
                     • Live Control: Use `minitask(action=\"update_minipub\", id=\"{}\", ...)` to adjust model/provider/timeout/todos/steer in-flight, `minitask(action=\"status\", id=\"{}\")` to inspect, or `minitask(action=\"stop\", id=\"{}\")` to terminate.",
                    id,
                    id,
                    role.badge(),
                    isolation_label,
                    timeout_secs_disp,
                    max_turns_disp,
                    budget_disp,
                    todos.len(),
                    pool.max_parallel(),
                    id,
                    id,
                    id
                ))
            }
            "update_minipub" | "modify_minipub" | "modify" | "configure" => {
                let mut changes = Vec::new();

                if let Some(stream_secs) = opt_u64(args, "stream_idle_timeout_secs") {
                    let clamped = stream_secs.clamp(30, 7200);
                    let conn_secs = crate::config::get_active_connect_timeout_secs();
                    crate::config::set_active_provider_timeouts_override(conn_secs, clamped);
                    changes.push(format!("• Orchestrator & MiniPub Model Stream Idle Timeout ➔ `{}s`", clamped));
                }

                let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
                if let Some(max_p) = opt_u64(args, "max_parallel")
                    .or_else(|| opt_u64(args, "max_parallel_minipubs"))
                {
                    let applied = pool.set_max_parallel(max_p as usize);
                    changes.push(format!("• MiniPub Pool Max Parallel Concurrency ➔ `{}`", applied));
                }

                let id_opt = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .or_else(|| opt_str(args, "target"));

                if let Some(id_str) = id_opt {
                    let minipub_id = id_str.strip_prefix("worker-").unwrap_or(id_str);
                    let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
                    let def_prov = opt_str(args, "provider").unwrap_or(&app_cfg.provider.default);
                    let update =
                        crate::tools::registry::agent_tools::subagents::build_minipub_update_from_args(
                            args, def_prov,
                        );

                    let info = pool.update_subagent(minipub_id, update).await?;
                    let done_todos = info.todos.iter().filter(|t| t.completed).count();
                    let model_badge = info
                        .active_model_label
                        .as_deref()
                        .unwrap_or("default-chain");
                    changes.push(format!(
                        "• Updated MiniPub `{}` ({}):\n  - State: {:?}\n  - Active Model Chain: `{}`\n  - Inactivity Timeout: `{}s`\n  - Max Turns: `{}` (Executed: `{}`)\n  - Token Budget: `{}` (Used: `{}`)\n  - Todos: `{}/{}` completed\n  - File Boundaries: `{}`\n  - Queued Steer Directives: `{}`",
                        info.id,
                        info.role.badge(),
                        info.state,
                        model_badge,
                        info.inactivity_timeout_secs,
                        info.max_turns,
                        info.turns_executed,
                        info.token_budget,
                        info.tokens_used,
                        done_todos,
                        info.todos.len(),
                        if info.file_boundaries.is_empty() {
                            "none".to_string()
                        } else {
                            info.file_boundaries.join(", ")
                        },
                        info.pending_steer_messages.len()
                    ));
                }

                if changes.is_empty() {
                    return Err(ToolError::invalid_args(
                        "minitask",
                        "Action 'update_minipub'/'configure' requires at least one of: 'id' (with timeout_secs, token_budget, max_turns, todos, append_todos, complete_todo_ids, file_boundaries, or steer_prompt), 'stream_idle_timeout_secs', or 'max_parallel'.",
                    )
                    .into());
                }

                Ok(format!(
                    "⚡ MiniPub & Orchestrator Runtime Configuration Updated:\n{}",
                    changes.join("\n")
                ))
            }
            "await" | "wait" => {
                crate::tools::registry::agent_tools::subagents::execute_await_workers(
                    args,
                    workspace_root,
                )
                .await
            }
            "transcript" | "drilldown" => {
                let id_raw = opt_str(args, "id")
                    .or_else(|| opt_str(args, "subagent_id"))
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "target"))
                    .ok_or_else(|| {
                        ToolError::invalid_args(
                            "minitask",
                            "Action 'transcript' requires 'id' or 'subagent_id'",
                        )
                    })?;
                let step_index = opt_u64(args, "step_index").map(|n| n as usize);
                let max_lines = opt_u64(args, "tail").map(|n| n as usize);
                let filter = opt_str(args, "filter").unwrap_or("summary");

                crate::tools::registry::agent_tools::subagents::execute_transcript_drilldown(
                    id_raw,
                    step_index,
                    max_lines,
                    filter,
                    workspace_root,
                    "MiniPub",
                )
            }
            "workers" | "minipubs" => {
                let list = registry.list_workers().await;
                let pool_workers = if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                    pool.list_subagents().await
                } else {
                    Vec::new()
                };
                let max_p = crate::agent::subagent::try_get_global_subagent_pool()
                    .map(|p| p.max_parallel())
                    .unwrap_or(crate::constants::DEFAULT_MAX_PARALLEL_MINIPUBS);
                let stream_idle = crate::config::get_active_stream_idle_timeout_secs();

                let extra_registry_workers: Vec<_> = list
                    .into_iter()
                    .filter(|p| {
                        let stripped = p.id.as_str().strip_prefix("worker-").unwrap_or(p.id.as_str());
                        !pool_workers.iter().any(|pw| pw.id == stripped)
                    })
                    .collect();

                if extra_registry_workers.is_empty() && pool_workers.is_empty() {
                    return Ok(format!(
                        "ℹ No autonomous MiniPub (`minipub`) subagents are currently active. (Pool Max Parallel: {}, Stream Idle Timeout: {}s)",
                        max_p, stream_idle
                    ));
                }

                let mut out = format!(
                    "🤖 Active MiniPub Subagents & Workers ({} MiniPub pool, {} external | Max Parallel: {} | Stream Idle Timeout: {}s):\n\n",
                    pool_workers.len(),
                    extra_registry_workers.len(),
                    max_p,
                    stream_idle
                );
                for info in &pool_workers {
                    let todo_str = if info.todos.is_empty() {
                        String::new()
                    } else {
                        let done = info.todos.iter().filter(|t| t.completed).count();
                        format!(" | Todos: {}/{}", done, info.todos.len())
                    };
                    out.push_str(&format!(
                        "• [minipub:{}] Role: {} | State: {:?} | Turns: {}/{} | Tokens: {}/{} | Timeout: {}s (Idle: {}s){}\n",
                        info.id,
                        info.role.badge(),
                        info.state,
                        info.turns_executed,
                        info.max_turns,
                        info.tokens_used,
                        info.token_budget,
                        info.inactivity_timeout_secs,
                        info.last_activity_secs_ago(),
                        todo_str
                    ));
                }
                for p in extra_registry_workers {
                    out.push_str(&format!(
                        "• [{}] {} | Status: {:?} | PID: {} | CPU: {:.1}% | RSS: {:.1}MB | Uptime: {}s\n",
                        p.id,
                        p.name,
                        p.status,
                        p.pid.unwrap_or(0),
                        p.cpu_percent,
                        p.memory_rss_mb,
                        p.uptime_secs,
                    ));
                }
                Ok(out)
            }
            "swarms" => {
                let list = registry.list_swarms().await;
                if list.is_empty() {
                    return Ok("ℹ No autonomous swarms or swarm workers are currently active.".to_string());
                }

                let mut out = format!("🐝 Active Multi-Agent Swarms & Workers ({} active):\n\n", list.len());
                for p in list {
                    out.push_str(&format!(
                        "• [{}] {} | Status: {:?} | PID: {} | CPU: {:.1}% | RSS: {:.1}MB | Uptime: {}s\n",
                        p.id,
                        p.name,
                        p.status,
                        p.pid.unwrap_or(0),
                        p.cpu_percent,
                        p.memory_rss_mb,
                        p.uptime_secs,
                    ));
                }
                Ok(out)
            }
            "list" | "ps" => {
                let filter_type = opt_str(args, "process_type")
                    .map(DevProcessType::from_str_loose);
                let list = registry.list_filtered(filter_type).await;
                let has_registered_browser = list.iter().any(|p| {
                    p.process_type == DevProcessType::Chrome
                        || p.id.as_str().eq_ignore_ascii_case("browser")
                        || p.id.as_str().starts_with("browser-")
                });
                let browser_details = if !has_registered_browser
                    && (filter_type.is_none() || filter_type == Some(DevProcessType::Chrome))
                {
                    crate::tools::browser::BrowserManager::get_live_engine_details().await
                } else {
                    None
                };

                if list.is_empty() && browser_details.is_none() {
                    if let Some(ft) = filter_type {
                        return Ok(format!("ℹ No processes of type '{:?}' are currently running.", ft));
                    } else {
                        return Ok("ℹ No development processes or browser sessions are currently running.".to_string());
                    }
                }

                let total_count = list.len() + if browser_details.is_some() { 1 } else { 0 };
                let header = if let Some(ft) = filter_type {
                    format!("📋 Managed Processes [{:?}] ({} active):\n\n", ft, total_count)
                } else {
                    format!("📋 Managed Development Processes ({} active):\n\n", total_count)
                };
                let mut out = header;
                for p in list {
                    let url_disp = p.url.as_deref().unwrap_or("-");
                    let sched_disp = if let Some(ref sched) = p.schedule_info {
                        if sched.is_one_shot {
                            format!(" | Timer: {}s (Runs: {})", sched.interval_secs, sched.iteration_count)
                        } else {
                            let max_str = sched.max_iterations.map(|m| format!("/{}", m)).unwrap_or_default();
                            format!(" | Schedule: Every {}s (Runs: {}{})", sched.interval_secs, sched.iteration_count, max_str)
                        }
                    } else {
                        String::new()
                    };
                    out.push_str(&format!(
                        "• [{}] {} ({:?}) | Status: {:?} | URL: {} | PID: {} | CPU: {:.1}% | RSS: {:.1}MB | Uptime: {}s{}\n",
                        p.id,
                        p.name,
                        p.process_type,
                        p.status,
                        url_disp,
                        p.pid.unwrap_or(0),
                        p.cpu_percent,
                        p.memory_rss_mb,
                        p.uptime_secs,
                        sched_disp,
                    ));
                }
                if let Some((engine, pid, port, uptime)) = browser_details {
                    out.push_str(&format!(
                        "• [browser] {} (Browser) | Status: Running | URL: http://127.0.0.1:{} | PID: {} | Uptime: {}s\n",
                        engine, port, pid, uptime
                    ));
                }
                Ok(out)
            }
            "status" => {
                let id_opt = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .or_else(|| opt_str(args, "target"));

                if let Some(id_str) = id_opt {
                    if id_str == "browser" || id_str == "chrome" || id_str == "obscura" {
                        if let Some((engine, pid, port, uptime)) =
                            crate::tools::browser::BrowserManager::get_live_engine_details().await
                        {
                            let bundle = crate::tools::browser::BrowserController::get_debug_bundle(
                                crate::tools::browser::BrowserMode::Headless,
                                workspace_root,
                            )
                            .await
                            .unwrap_or_default();
                            return Ok(format!(
                                "📊 Browser Engine Status:\n• Engine: {}\n• Status: Running\n• PID: {}\n• CDP Port: {}\n• Uptime: {}s\n\n{}",
                                engine, pid, port, uptime, bundle
                            ));
                        } else {
                            return Ok("ℹ No active browser session is currently running.".to_string());
                        }
                    }

                    let minipub_id = id_str.strip_prefix("worker-").unwrap_or(id_str);
                    if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                        if let Some(info) = pool.get_subagent(minipub_id).await {
                            let todos_section = if info.todos.is_empty() {
                                String::new()
                            } else {
                                let items = info
                                    .todos
                                    .iter()
                                    .map(|t| {
                                        format!(
                                            "  - [{}] (#{}) {}",
                                            if t.completed { "x" } else { " " },
                                            t.id,
                                            t.task
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                format!("\n• Assigned Todos:\n{}", items)
                            };
                            let boundaries = if info.file_boundaries.is_empty() {
                                "none".to_string()
                            } else {
                                info.file_boundaries.join(", ")
                            };
                            return Ok(format!(
                                "📊 MiniPub Subagent Status for '{}':\n\
                                 • Role: {}\n\
                                 • State: {:?}\n\
                                 • Turns: {}/{}\n\
                                 • Tokens: {}/{}\n\
                                 • Inactivity Timeout: {}s (Last Activity: {}s ago | Elapsed: {}s)\n\
                                 • Isolation: {}\n\
                                 • File Boundaries: {}{}",
                                info.id,
                                info.role.badge(),
                                info.state,
                                info.turns_executed,
                                info.max_turns,
                                info.tokens_used,
                                info.token_budget,
                                info.inactivity_timeout_secs,
                                info.last_activity_secs_ago(),
                                info.elapsed_secs(),
                                if info.isolate_worktree { "Git Worktree" } else { "Shared / Cooperative" },
                                boundaries,
                                todos_section
                            ));
                        }
                    }

                    let id = DevProcessId::from(id_str);
                    let summary = registry
                        .get(&id)
                        .await
                        .ok_or_else(|| DevError::NotFound(id_str.to_string()))?;

                    let url_disp = summary.url.as_deref().unwrap_or("none");
                    let sched_disp = if let Some(ref sched) = summary.schedule_info {
                        if sched.is_one_shot {
                            format!("\n• Timer Duration: {}s\n• Runs: {}", sched.interval_secs, sched.iteration_count)
                        } else {
                            let max_str = sched.max_iterations.map(|m| format!(" (max: {})", m)).unwrap_or_default();
                            format!("\n• Schedule: Every {}s\n• Runs: {}{}", sched.interval_secs, sched.iteration_count, max_str)
                        }
                    } else {
                        String::new()
                    };
                    Ok(format!(
                        "📊 Process Status for '{}':\n• Name: {}\n• Type: {:?}\n• Status: {:?}\n• PID: {}\n• URL: {}\n• Ports: {:?}\n• CPU: {:.1}%\n• Memory RSS: {:.1} MB\n• Uptime: {}s{}",
                        summary.id,
                        summary.name,
                        summary.process_type,
                        summary.status,
                        summary.pid.unwrap_or(0),
                        url_disp,
                        summary.ports,
                        summary.cpu_percent,
                        summary.memory_rss_mb,
                        summary.uptime_secs,
                        sched_disp
                    ))
                } else {
                    // Fall back to listing all processes
                    let list = registry.list().await;
                    let browser_details =
                        crate::tools::browser::BrowserManager::get_live_engine_details().await;
                    if list.is_empty() && browser_details.is_none() {
                        return Ok("ℹ No development processes or browser sessions are currently running.".to_string());
                    }
                    let total_count = list.len() + if browser_details.is_some() { 1 } else { 0 };
                    let mut out = format!("📋 Managed Development Processes ({} active):\n\n", total_count);
                    for p in list {
                        let url_disp = p.url.as_deref().unwrap_or("-");
                        let sched_disp = if let Some(ref sched) = p.schedule_info {
                            if sched.is_one_shot {
                                format!(" | Timer: {}s (Runs: {})", sched.interval_secs, sched.iteration_count)
                            } else {
                                let max_str = sched.max_iterations.map(|m| format!("/{}", m)).unwrap_or_default();
                                format!(" | Schedule: Every {}s (Runs: {}{})", sched.interval_secs, sched.iteration_count, max_str)
                            }
                        } else {
                            String::new()
                        };
                        out.push_str(&format!(
                            "• [{}] {} ({:?}) | Status: {:?} | URL: {} | PID: {} | CPU: {:.1}% | RSS: {:.1}MB{}\n",
                            p.id,
                            p.name,
                            p.process_type,
                            p.status,
                            url_disp,
                            p.pid.unwrap_or(0),
                            p.cpu_percent,
                            p.memory_rss_mb,
                            sched_disp,
                        ));
                    }
                    if let Some((engine, pid, port, uptime)) = browser_details {
                        out.push_str(&format!(
                            "• [browser] {} (Browser) | Status: Running | URL: http://127.0.0.1:{} | PID: {} | Uptime: {}s\n",
                            engine, port, pid, uptime
                        ));
                    }
                    Ok(out)
                }
            }
            "logs" => {
                let id_str = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .or_else(|| opt_str(args, "target"))
                    .ok_or_else(|| ToolError::invalid_args("minitask", "Missing required parameter 'id' or 'task_id'"))?;
                let id = DevProcessId::from(id_str);
                let tail = opt_u64(args, "tail").unwrap_or(50) as usize;
                let filter = opt_str(args, "filter");

                let logs = registry.logs(&id, tail, filter).await?;
                if logs.is_empty() {
                    return Ok(format!("ℹ No logs recorded for process '{}'.", id_str));
                }

                let mut out = format!("📜 Recent logs for process '{}' (last {} lines):\n\n", id_str, logs.len());
                for line in logs {
                    out.push_str(&line);
                    out.push('\n');
                }
                Ok(out)
            }
            "browser_close" => {
                let force = opt_bool(args, "force", false);
                let stopped = crate::tools::browser::BrowserManager::shutdown_live_engine_opt(force).await?;
                let is_running = crate::tools::browser::BrowserManager::is_live_engine_running().await;
                match (stopped, is_running) {
                    (true, _) => {
                        let _ = registry.stop(&DevProcessId::from("browser")).await;
                        Ok("✔ Browser session closed successfully. All browser processes terminated.".to_string())
                    }
                    (false, true) => {
                        Ok("ℹ Active GUI browser window preserved for user viewing. Pass force=true to terminate.".to_string())
                    }
                    (false, false) => {
                        Ok("ℹ No active browser session was running.".to_string())
                    }
                }
            }
            "stop" | "kill" => {
                let id_opt = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .or_else(|| opt_str(args, "target"));

                if id_opt == Some("all") {
                    if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                        pool.kill_all().await;
                    }
                    let count = registry.kill_all().await?;
                    let _ = crate::tools::browser::BrowserManager::shutdown_live_engine_opt(true).await;
                    return Ok(format!("✔ Terminated all {} active development processes, MiniPub workers, and browser sessions.", count));
                }

                let id_to_stop = match id_opt {
                    Some(s) => s.to_string(),
                    None => {
                        let list = registry.list().await;
                        if list.len() == 1 {
                            list[0].id.as_str().to_string()
                        } else if list.is_empty() {
                            if crate::tools::browser::BrowserManager::is_live_engine_running().await {
                                let _ = crate::tools::browser::BrowserManager::shutdown_live_engine_opt(true).await;
                                return Ok("✔ Active browser session closed successfully.".to_string());
                            }
                            return Ok("ℹ No active development processes or browser sessions to stop.".to_string());
                        } else {
                            return Err(ToolError::invalid_args("minitask", "Multiple processes are active. Please specify 'id' or 'task_id' (or 'all').").into());
                        }
                    }
                };

                if id_to_stop == "browser" || id_to_stop == "chrome" {
                    let stopped = crate::tools::browser::BrowserManager::shutdown_live_engine_opt(true).await?;
                    let _ = registry.stop(&DevProcessId::from("browser")).await;
                    if stopped {
                        return Ok("✔ Browser session closed successfully.".to_string());
                    } else {
                        return Ok("ℹ No active browser session was running.".to_string());
                    }
                }

                // Also check if id_to_stop refers to a MiniPub subagent in SubagentPool
                let minipub_id = id_to_stop.strip_prefix("worker-").unwrap_or(&id_to_stop);
                let mut killed_minipub = false;
                if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                    if pool.kill_subagent(minipub_id).await.is_ok() {
                        killed_minipub = true;
                    }
                }

                let id = DevProcessId::from(id_to_stop.clone());
                let reg_res = registry.stop(&id).await;
                if reg_res.is_err() && !killed_minipub {
                    let worker_prefixed = DevProcessId::from(format!("worker-{}", minipub_id));
                    if registry.stop(&worker_prefixed).await.is_err() {
                        reg_res?;
                    }
                }

                // If no more frontend/backend servers are active, automatically shut down any paired live browser session
                let mut browser_msg = String::new();
                let remaining_frontends = registry.list_filtered(Some(DevProcessType::Frontend)).await;
                let remaining_backends = registry.list_filtered(Some(DevProcessType::Backend)).await;
                if remaining_frontends.is_empty()
                    && remaining_backends.is_empty()
                    && crate::tools::browser::BrowserManager::is_live_engine_running().await
                {
                    let _ = crate::tools::browser::BrowserManager::shutdown_live_engine().await;
                    browser_msg = " and closed paired browser session".to_string();
                }

                Ok(format!(
                    "✔ Process/MiniPub worker '{}' stopped successfully{}.",
                    id_to_stop, browser_msg
                ))
            }
            "restart" => {
                let id_str = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .or_else(|| opt_str(args, "target"))
                    .ok_or_else(|| ToolError::invalid_args("minitask", "Missing required parameter 'id' or 'task_id'"))?;
                let id = DevProcessId::from(id_str);
                let summary = registry.restart(workspace_root, &id).await?;
                Ok(format!(
                    "🔄 Process '{}' restarted successfully (New PID: {}).",
                    summary.id,
                    summary.pid.unwrap_or(0)
                ))
            }
            "resources" => {
                let res = registry.resources().await;
                Ok(format!(
                    "📈 Runtime Resource Telemetry:\n• Active Processes: {}\n• Total CPU Usage: {:.1}%\n• Total RSS Memory: {:.1} MB\n• Listening Ports: {:?}",
                    res.total_active_processes,
                    res.total_cpu_percent,
                    res.total_memory_rss_mb,
                    res.active_ports,
                ))
            }
            "kill_all" | "stop_all" => {
                if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                    pool.kill_all().await;
                }
                let count = registry.kill_all().await?;
                let _ = crate::tools::browser::BrowserManager::shutdown_live_engine().await;
                Ok(format!("✔ Terminated {} active development processes, MiniPub workers, and browser sessions.", count))
            }
            "screenshot" => {
                let explicit_url = opt_str(args, "url");
                let id_opt = opt_str(args, "id").map(DevProcessId::from);
                let custom_path = opt_str(args, "path");
                let mode = parse_browser_mode(args);

                let target_url = if let Some(u) = explicit_url {
                    u.to_string()
                } else if let Some(ref id) = id_opt {
                    let summary = registry.get(id).await.ok_or_else(|| {
                        DevError::NotFound(format!("Process '{}' not found for screenshot", id))
                    })?;
                    summary.url.ok_or_else(|| {
                        DevError::InvalidRequest(format!(
                            "Process '{}' has not reported any listening port/URL yet",
                            id
                        ))
                    })?
                } else {
                    let list = registry.list().await;
                    list.into_iter()
                        .find_map(|p| p.url)
                        .ok_or_else(|| {
                            ToolError::InvalidArguments {
                                name: "minitask".to_string(),
                                reason: "Action 'screenshot' requires either 'url', 'id' of a running process, or an active dev server with an open port.".to_string(),
                            }
                        })?
                };

                // Navigate and snapshot DOM
                let _ = crate::tools::browser::BrowserController::navigate_and_snapshot(
                    &target_url,
                    mode,
                    workspace_root,
                )
                .await?;

                // Brief pause for client hydration / animation
                tokio::time::sleep(Duration::from_millis(600)).await;

                // Take screenshot
                let result_msg = crate::tools::browser::BrowserController::take_screenshot(
                    mode,
                    workspace_root,
                    custom_path,
                )
                .await?;

                Ok(format!(
                    "📸 Screenshot captured successfully for '{}':\n• {}",
                    target_url, result_msg
                ))
            }
            "audit" | "qa_audit" => {
                let mode = parse_browser_mode(args);
                let explicit_url = opt_str(args, "url");
                let id_opt = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .map(DevProcessId::from);

                let target_url: Option<String> = if let Some(u) = explicit_url {
                    Some(u.to_string())
                } else if let Some(ref id) = id_opt {
                    let summary = registry.get(id).await.ok_or_else(|| {
                        DevError::NotFound(format!("Process '{}' not found for QA audit", id))
                    })?;
                    summary.url
                } else {
                    let list = registry.list().await;
                    list.into_iter().find_map(|p| p.url)
                };

                let report = crate::tools::browser::BrowserController::run_qa_audit(
                    target_url.as_deref(),
                    mode,
                    workspace_root,
                )
                .await?;
                Ok(report)
            }
            "inspect" | "inspect_dom" => {
                let mode = parse_browser_mode(args);
                let explicit_url = opt_str(args, "url");
                let id_opt = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .map(DevProcessId::from);

                let target_url: Option<String> = if let Some(u) = explicit_url {
                    Some(u.to_string())
                } else if let Some(ref id) = id_opt {
                    let summary = registry.get(id).await.ok_or_else(|| {
                        DevError::NotFound(format!("Process '{}' not found for DOM inspection", id))
                    })?;
                    summary.url
                } else {
                    None
                };

                if let Some(url) = target_url {
                    let _ = crate::tools::browser::BrowserController::navigate_and_snapshot(
                        &url,
                        mode,
                        workspace_root,
                    )
                    .await?;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }

                let report = crate::tools::browser::BrowserController::inspect_visual_dom(
                    mode,
                    workspace_root,
                )
                .await?;
                Ok(report)
            }
            "batch" | "browser_batch" => {
                let mode = parse_browser_mode(args);
                let actions_val = args.get("actions").ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "minitask".to_string(),
                        reason: "Action 'batch' requires 'actions' array of browser operations".to_string(),
                    }
                })?;
                let steps: Vec<crate::tools::browser::BatchStep> =
                    serde_json::from_value(actions_val.clone()).map_err(|e| {
                        ToolError::InvalidArguments {
                            name: "minitask".to_string(),
                            reason: format!("Failed parsing batch actions: {}", e),
                        }
                    })?;

                crate::tools::browser::BrowserController::execute_batch(
                    &steps,
                    mode,
                    workspace_root,
                )
                .await
            }
            "mock_route" => {
                let mode = parse_browser_mode(args);
                let clear = args.get("clear").and_then(|c| c.as_bool()).unwrap_or(false);
                let pattern = opt_str(args, "pattern").unwrap_or("*");
                let status = args.get("status").and_then(|s| s.as_u64()).unwrap_or(200) as u16;
                let body = opt_str(args, "body").unwrap_or("{}");
                let content_type = opt_str(args, "content_type");

                crate::tools::browser::BrowserController::mock_route(
                    pattern,
                    status,
                    body,
                    content_type,
                    clear,
                    mode,
                    workspace_root,
                )
                .await
            }
            "browser_status" | "browser_debug" => {
                let mode = parse_browser_mode(args);
                if !crate::tools::browser::BrowserManager::is_live_engine_running().await {
                    return Ok("ℹ No active browser session is currently running.".to_string());
                }
                crate::tools::browser::BrowserController::get_debug_bundle(mode, workspace_root)
                    .await
            }
            "emulate" | "browser_emulate" => {
                let mode = parse_browser_mode(args);
                let viewport = opt_str(args, "viewport");
                let network = opt_str(args, "network");
                let width = opt_u64(args, "custom_width")
                    .or_else(|| opt_u64(args, "width"))
                    .map(|w| w as u32);
                let height = opt_u64(args, "custom_height")
                    .or_else(|| opt_u64(args, "height"))
                    .map(|h| h as u32);

                crate::tools::browser::BrowserController::emulate_device_and_network(
                    viewport,
                    network,
                    width,
                    height,
                    mode,
                    workspace_root,
                )
                .await
            }
            "save_state" | "browser_save_state" => {
                let mode = parse_browser_mode(args);
                let profile = opt_str(args, "profile")
                    .or_else(|| opt_str(args, "name"))
                    .unwrap_or("default");

                crate::tools::browser::BrowserController::save_session_state(
                    profile,
                    mode,
                    workspace_root,
                )
                .await
            }
            "restore_state" | "browser_restore_state" => {
                let mode = parse_browser_mode(args);
                let profile = opt_str(args, "profile")
                    .or_else(|| opt_str(args, "name"))
                    .unwrap_or("default");

                crate::tools::browser::BrowserController::restore_session_state(
                    profile,
                    mode,
                    workspace_root,
                )
                .await
            }
            "pdf" | "browser_pdf" => {
                let mode = parse_browser_mode(args);
                let explicit_url = opt_str(args, "url");
                let id_opt = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .map(DevProcessId::from);

                let target_url: Option<String> = if let Some(u) = explicit_url {
                    Some(u.to_string())
                } else if let Some(ref id) = id_opt {
                    if id.as_str() != "browser" {
                        let summary = registry.get(id).await.ok_or_else(|| {
                            DevError::NotFound(format!("Process '{}' not found for PDF export", id))
                        })?;
                        summary.url
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(url) = target_url {
                    let _ = crate::tools::browser::BrowserController::navigate_and_snapshot(
                        &url,
                        mode,
                        workspace_root,
                    )
                    .await?;
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }

                let custom_path = opt_str(args, "path");
                let landscape = args.get("landscape").and_then(|v| v.as_bool()).unwrap_or(false);
                let print_bg = args.get("print_background").and_then(|v| v.as_bool()).unwrap_or(true);

                crate::tools::browser::BrowserController::export_pdf(
                    custom_path,
                    landscape,
                    print_bg,
                    mode,
                    workspace_root,
                )
                .await
            }
            "check_injection" | "browser_check_injection" | "scan_hidden" => {
                let mode = parse_browser_mode(args);
                let explicit_url = opt_str(args, "url");
                let id_opt = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .map(DevProcessId::from);

                let target_url: Option<String> = if let Some(u) = explicit_url {
                    Some(u.to_string())
                } else if let Some(ref id) = id_opt {
                    if id.as_str() != "browser" {
                        let summary = registry.get(id).await.ok_or_else(|| {
                            DevError::NotFound(format!("Process '{}' not found for security audit", id))
                        })?;
                        summary.url
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(url) = target_url {
                    let _ = crate::tools::browser::BrowserController::navigate_and_snapshot(
                        &url,
                        mode,
                        workspace_root,
                    )
                    .await?;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }

                crate::tools::browser::BrowserController::check_prompt_injections(
                    mode,
                    workspace_root,
                )
                .await
            }
            "hover" | "browser_hover" => {
                let mode = parse_browser_mode(args);
                let target_ref = opt_str(args, "ref")
                    .or_else(|| opt_str(args, "selector"))
                    .or_else(|| opt_str(args, "target"))
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "minitask".to_string(),
                        reason: "Action 'hover' requires 'ref' or 'selector' parameter".to_string(),
                    })?;
                crate::tools::browser::BrowserController::hover_and_snapshot(
                    target_ref,
                    mode,
                    workspace_root,
                )
                .await
            }
            "select_option" | "browser_select_option" => {
                let mode = parse_browser_mode(args);
                let target_ref = opt_str(args, "ref")
                    .or_else(|| opt_str(args, "selector"))
                    .or_else(|| opt_str(args, "target"))
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "minitask".to_string(),
                        reason: "Action 'select_option' requires 'ref' or 'selector' parameter".to_string(),
                    })?;
                let option_text = opt_str(args, "option")
                    .or_else(|| opt_str(args, "value"))
                    .or_else(|| opt_str(args, "text"))
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "minitask".to_string(),
                        reason: "Action 'select_option' requires 'option' or 'value' parameter".to_string(),
                    })?;
                crate::tools::browser::BrowserController::select_option_and_snapshot(
                    target_ref,
                    option_text,
                    mode,
                    workspace_root,
                )
                .await
            }
            "scroll_horizontal" | "browser_scroll_horizontal" => {
                let mode = parse_browser_mode(args);
                let direction = opt_str(args, "direction").unwrap_or("right");
                let pixels = args.get("distance").and_then(|d| d.as_i64()).map(|d| d as i32);
                let selector = opt_str(args, "selector").or_else(|| opt_str(args, "ref"));
                crate::tools::browser::BrowserController::scroll_horizontally(
                    direction,
                    pixels,
                    selector,
                    mode,
                    workspace_root,
                )
                .await
            }
            "badges" | "browser_badges" => {
                let mode = parse_browser_mode(args);
                let enable = args.get("enable").and_then(|v| v.as_bool()).unwrap_or(true);
                crate::tools::browser::BrowserController::toggle_visual_badges(
                    enable,
                    mode,
                    workspace_root,
                )
                .await
            }
            "aura" | "browser_aura" | "glow" => {
                let mode = parse_browser_mode(args);
                let enable = args.get("enable").and_then(|v| v.as_bool()).unwrap_or(true);
                let status = opt_str(args, "status").or_else(|| opt_str(args, "prompt"));
                crate::tools::browser::BrowserController::toggle_simulator_aura(
                    enable,
                    status,
                    mode,
                    workspace_root,
                )
                .await
            }
            "tabs" | "browser_tabs" => {
                let mode = parse_browser_mode(args);
                let subaction = opt_str(args, "subaction")
                    .or_else(|| opt_str(args, "tab_action"))
                    .unwrap_or("list");
                let url = opt_str(args, "url");
                let target_id = opt_str(args, "target_id")
                    .or_else(|| opt_str(args, "tab_id"))
                    .or_else(|| opt_str(args, "id"));
                crate::tools::browser::BrowserController::manage_tabs(
                    subaction,
                    url,
                    target_id,
                    mode,
                    workspace_root,
                )
                .await
            }
            "metrics" | "browser_metrics" => {
                let mode = parse_browser_mode(args);
                let metrics = crate::tools::browser::BrowserController::get_page_metrics(
                    mode,
                    workspace_root,
                )
                .await?;
                let json_metrics = serde_json::to_string_pretty(&metrics)
                    .unwrap_or_else(|_| "{}".to_string());
                Ok(format!(
                    "📊 Page & Viewport Metrics (Alibaba Page-Agent compatible):\n```json\n{}\n```\n• Viewport: {}x{}\n• Full Page: {}x{}\n• Scroll Position: ({}, {})\n• Pages Remaining Below: {:.1} (total: {})",
                    json_metrics,
                    metrics.viewport_width,
                    metrics.viewport_height,
                    metrics.page_width,
                    metrics.page_height,
                    metrics.scroll_x,
                    metrics.scroll_y,
                    metrics.pages_below,
                    metrics.total_pages
                ))
            }
            unknown => Err(ToolError::InvalidArguments {
                name: "minitask".to_string(),
                reason: format!("Unknown action '{}'. Expected: start, scripts, targets, schedule, list, status, logs, stop, restart, resources, kill_all, screenshot, workers, swarms, probe_port, audit, inspect, batch, mock_route, browser_status, browser_close, emulate, save_state, restore_state, pdf, check_injection, hover, select_option, scroll_horizontal, badges, tabs, metrics", unknown),
            }.into()),
        }
    }.await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_minitask_schema_valid() {
        let schemas = get_schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "minitask");
    }

    #[tokio::test]
    async fn test_minitask_dispatch_lifecycle() {
        let temp = tempdir().unwrap();

        // 1. Start a mock server via canonical name "minitask"
        let start_args = json!({
            "action": "start",
            "command": "echo 'Server listening on http://localhost:8765'; sleep 30",
            "name": "mock-api",
            "process_type": "backend"
        });

        let start_res = dispatch("minitask", &start_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(start_res.contains("Development process launched successfully"));

        // Wait brief moment for logs and ports
        let mut list_res = String::new();
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let list_args = json!({ "action": "list" });
            if let Some(Ok(res)) = dispatch("minitask", &list_args, temp.path()).await {
                if res.contains("mock-api") {
                    list_res = res;
                    break;
                }
                list_res = res;
            }
        }
        assert!(list_res.contains("mock-api"));

        // 3. Resources
        let res_args = json!({ "action": "resources" });
        let res_output = dispatch("minitask", &res_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(res_output.contains("Runtime Resource Telemetry"));

        // 4. Kill all
        let kill_args = json!({ "action": "kill_all" });
        let kill_res = dispatch("minitask", &kill_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(kill_res.contains("Terminated"));
    }

    #[tokio::test]
    async fn test_minitask_workers_dispatch() {
        let temp = tempdir().unwrap();
        let registry = get_global_dev_registry();

        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel_flag);

        let dev_id = DevProcessId::from("worker-subagent-test-99");
        let handle = registry
            .register_worker(
                dev_id.clone(),
                "Subagent (Tester) - Verify Auth".to_string(),
                "minicode run --tools tester 'test'".to_string(),
                temp.path().to_path_buf(),
                std::process::id(),
                0,
                Some(Arc::new(move || {
                    cancel_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                })),
            )
            .await;

        handle.append_log("Worker initializing sandbox...").await;

        // 1. Query via action: "workers"
        let workers_args = json!({ "action": "workers" });
        let workers_res = dispatch("minitask", &workers_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(workers_res.contains("Subagent (Tester) - Verify Auth"));
        assert!(workers_res.contains("worker-subagent-test-99"));

        // 2. Query via action: "list", process_type: "worker"
        let list_worker_args = json!({ "action": "list", "process_type": "worker" });
        let list_worker_res = dispatch("minitask", &list_worker_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(list_worker_res.contains("Subagent (Tester) - Verify Auth"));

        // 3. Query logs via raw id
        let logs_args = json!({ "action": "logs", "id": "subagent-test-99" });
        let logs_res = dispatch("minitask", &logs_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(logs_res.contains("Worker initializing sandbox"));

        // 4. Stop worker
        let stop_args = json!({ "action": "stop", "id": "subagent-test-99" });
        let stop_res = dispatch("minitask", &stop_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(stop_res.contains("stopped successfully"));
        assert!(cancel_flag.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_minitask_probe_port_dispatch() {
        let temp = tempdir().unwrap();

        // 1. Probe a high free port
        let free_port = 48877;
        let probe_args = json!({
            "action": "probe_port",
            "port": free_port
        });

        let probe_res = dispatch("minitask", &probe_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(probe_res.contains("AVAILABLE"));

        // 2. Bind a socket and probe it
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let bound_port = listener.local_addr().unwrap().port();

        let occ_args = json!({
            "action": "probe_port",
            "port": bound_port
        });
        let occ_res = dispatch("minitask", &occ_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(occ_res.contains("OCCUPIED"));
        assert!(occ_res.contains("Next Available Port:"));
    }

    #[tokio::test]
    async fn test_minitask_port_conflict_fallback_dispatch() {
        let temp = tempdir().unwrap();

        // Bind port to force conflict
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let bound_port = listener.local_addr().unwrap().port();

        let start_args = json!({
            "action": "start",
            "command": format!("echo 'Server on port'; sleep 30"),
            "name": "auto-shift-service",
            "port_hint": bound_port,
            "port_policy": "fallback"
        });

        let start_res = dispatch("minitask", &start_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(start_res.contains("Port Conflict Resolved"));
        assert!(start_res.contains(&format!("{} occupied", bound_port)));

        // Clean up
        let stop_args = json!({ "action": "stop", "name": "auto-shift-service" });
        let _ = dispatch("minitask", &stop_args, temp.path()).await;
    }

    #[tokio::test]
    async fn test_minitask_schedule_tool_dispatch() {
        let temp = tempdir().unwrap();
        let args = json!({
            "action": "schedule",
            "command": "echo 'Heartbeat check'",
            "name": "system-heartbeat",
            "interval_seconds": 30
        });

        let res = dispatch("minitask", &args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(res.contains("Scheduled task registered successfully"));
        assert!(res.contains("30s"));

        // List should include schedule info
        let list_args = json!({ "action": "list" });
        let list_res = dispatch("minitask", &list_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(list_res.contains("system-heartbeat"));
        assert!(list_res.contains("30s"));

        // Clean up
        let stop_args = json!({ "action": "stop", "name": "system-heartbeat" });
        let _ = dispatch("minitask", &stop_args, temp.path()).await;
    }

    #[tokio::test]
    async fn test_minitask_swarms_dispatch() {
        let temp = tempdir().unwrap();
        let registry = get_global_dev_registry();

        let handle = registry
            .register_swarm_process(
                DevProcessId::from("swarm-unit-dispatch"),
                "Dispatch Test Swarm".to_string(),
                "minicode swarm run dispatch".to_string(),
                temp.path().to_path_buf(),
                std::process::id(),
                0,
                None,
            )
            .await;

        let args = json!({ "action": "swarms" });
        let res = dispatch("minitask", &args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(res.contains("swarm-unit-dispatch"));
        assert!(res.contains("Dispatch Test Swarm"));

        // List with process_type="swarm"
        let list_args = json!({ "action": "list", "process_type": "swarm" });
        let list_res = dispatch("minitask", &list_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(list_res.contains("swarm-unit-dispatch"));

        handle
            .update_status(crate::dev::models::DevProcessStatus::Stopped)
            .await;
        let _ = registry
            .stop(&DevProcessId::from("swarm-unit-dispatch"))
            .await;
    }

    #[tokio::test]
    async fn test_minitask_browser_actions_schema() {
        let schemas = get_schemas();
        let minitask = &schemas[0];
        let actions = minitask.parameters["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();

        assert!(actions.contains(&"start"));
        assert!(actions.contains(&"scripts"));
        assert!(actions.contains(&"schedule"));
        assert!(actions.contains(&"list"));
        assert!(actions.contains(&"status"));
        assert!(actions.contains(&"logs"));
        assert!(actions.contains(&"stop"));
        assert!(actions.contains(&"restart"));
        assert!(actions.contains(&"resources"));
        assert!(actions.contains(&"probe_port"));
        assert!(actions.contains(&"workers"));
        assert!(actions.contains(&"swarms"));
        assert!(actions.contains(&"screenshot"));
        assert!(actions.contains(&"audit"));
    }

    #[tokio::test]
    async fn test_minitask_browser_close_and_status_dispatch() {
        let temp = tempdir().unwrap();

        // 1. Query browser_status when no engine running
        let status_args = json!({ "action": "browser_status" });
        let status_res = dispatch("minitask", &status_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(status_res.contains("No active browser session"));

        // 2. Query status for id="browser"
        let proc_status_args = json!({ "action": "status", "id": "browser" });
        let proc_status_res = dispatch("minitask", &proc_status_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(proc_status_res.contains("No active browser session"));

        // 3. browser_close action when not running
        let close_args = json!({ "action": "browser_close" });
        let close_res = dispatch("minitask", &close_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(close_res.contains("No active browser session"));
    }

    #[tokio::test]
    async fn test_minitask_page_agent_dispatch_validations() {
        let temp = tempdir().unwrap();

        // Hover requires ref or selector
        let hover_args = json!({ "action": "hover" });
        let hover_err = dispatch("minitask", &hover_args, temp.path())
            .await
            .unwrap();
        assert!(hover_err.is_err());

        // Select option requires option or value
        let select_args = json!({ "action": "select_option", "ref": "@v1:e1" });
        let select_err = dispatch("minitask", &select_args, temp.path())
            .await
            .unwrap();
        assert!(select_err.is_err());
    }

    #[tokio::test]
    async fn test_minitask_scripts_discovery_and_dispatch() {
        let temp = tempdir().unwrap();
        let pkg_json = r#"{
            "scripts": {
                "dev": "vite",
                "build": "vite build"
            }
        }"#;
        std::fs::write(temp.path().join("package.json"), pkg_json).unwrap();

        // 1. minitask(action="scripts")
        let scripts_args = json!({ "action": "scripts" });
        let scripts_res = dispatch("minitask", &scripts_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(scripts_res.contains("dev"));
        assert!(scripts_res.contains("build"));

        // 2. minitask(action="targets")
        let targets_args = json!({ "action": "targets" });
        let targets_res = dispatch("minitask", &targets_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(targets_res.contains("dev"));
    }
}
