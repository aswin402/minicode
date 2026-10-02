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
        description: "Unified Task & Process Vault (minitask). Supervises and tracks all long-running development servers, web applications, background workers, subagents, scripts, scheduled tasks, and browser sessions. Provides full lifecycle CRUD (start, schedule, list, status, logs, stop, kill, restart, resources, kill_all), automatic port discovery, process group isolation, OOM/runaway watchdogs, and clean teardown. Automatically terminates all managed tasks when minicode exits.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "start", "schedule", "list", "ps", "status", "logs", "stop", "kill", "restart",
                        "resources", "kill_all", "stop_all", "screenshot", "workers", "swarms",
                        "probe_port", "check_port", "audit", "qa_audit", "inspect", "inspect_dom",
                        "batch", "browser_batch", "mock_route", "browser_status", "browser_debug",
                        "browser_close", "emulate", "browser_emulate", "save_state", "browser_save_state",
                        "restore_state", "browser_restore_state", "pdf", "browser_pdf",
                        "check_injection", "browser_check_injection", "scan_hidden",
                        "hover", "browser_hover", "select_option", "browser_select_option",
                        "scroll_horizontal", "browser_scroll_horizontal",
                        "badges", "browser_badges", "tabs", "browser_tabs",
                        "metrics", "browser_metrics"
                    ],
                    "description": "Lifecycle action to perform: 'start' (launch process), 'schedule' (register interval task, watcher, or one-shot timer), 'list'/'status'/'ps' (inspect active processes and schedules), 'logs' (tail output), 'stop'/'kill' (gracefully terminate process), 'restart' (cycle process), 'resources' (CPU & memory telemetry), 'kill_all'/'stop_all' (terminate all active processes and browser engines), 'screenshot' (capture visual PNG of running server or URL), 'workers' (list active autonomous subagents and delegated tasks), 'swarms' (list active multi-agent swarms and swarm workers), 'probe_port' (inspect if a port is in use and find conflicting PID/fallback port), 'audit'/'qa_audit' (run automated in-page QA audit on running server or URL), 'inspect'/'inspect_dom' (deep DOM grounding & Shadow DOM inspection), 'batch'/'browser_batch' (execute multi-step browser actions pipeline), 'mock_route' (intercept/mock API routes over CDP), 'browser_status'/'browser_debug' (browser diagnostics bundle), 'browser_close' (terminate active browser session), 'emulate'/'browser_emulate' (set viewport/device presets and network throttling), 'save_state'/'browser_save_state' (save cookies/localStorage to profile), 'restore_state'/'browser_restore_state' (restore cookies/localStorage from profile), 'pdf'/'browser_pdf' (export page to PDF report), 'check_injection'/'browser_check_injection' (scan DOM for hidden text and prompt injections), 'hover'/'browser_hover' (hover over element), 'select_option'/'browser_select_option' (select dropdown option), 'scroll_horizontal'/'browser_scroll_horizontal' (scroll horizontally), 'badges'/'browser_badges' (toggle numbered badges), 'tabs'/'browser_tabs' (multi-tab management), 'metrics'/'browser_metrics' (page and scroll metrics)"
                },
                "command": {
                    "type": "string",
                    "description": "Shell command to run (required for 'start', e.g. 'npm run dev', 'cargo run', 'python app.py', or command for 'schedule')"
                },
                "prompt": {
                    "type": "string",
                    "description": "Task instructions, prompt, or command alias for 'schedule'"
                },
                "name": {
                    "type": "string",
                    "description": "Optional human-readable label for the process or scheduled task (e.g. 'frontend-vite', 'health-check', 'auth-service')"
                },
                "process_type": {
                    "type": "string",
                    "enum": ["frontend", "backend", "docker", "script", "browser", "worker", "subagent", "cron", "timer", "swarm"],
                    "description": "Category of process (default: 'frontend', can be 'frontend', 'backend', 'docker', 'script', 'browser', 'worker', 'subagent', 'cron', 'timer', 'swarm')"
                },
                "interval_seconds": {
                    "type": "integer",
                    "description": "Interval in seconds between periodic executions for 'schedule' (default: 60)"
                },
                "duration_seconds": {
                    "type": "integer",
                    "description": "Delay in seconds before firing a one-shot timer for 'schedule'"
                },
                "cron": {
                    "type": "string",
                    "description": "Cron expression or description for recurring scheduled tasks"
                },
                "cron_expression": {
                    "type": "string",
                    "description": "Cron expression alias for 'cron'"
                },
                "max_iterations": {
                    "type": "integer",
                    "description": "Maximum number of iterations before automatically terminating a scheduled task (unlimited if omitted)"
                },
                "id": {
                    "type": "string",
                    "description": "Process ID (required for 'status', 'logs', 'stop', 'kill', 'restart', or target for 'screenshot')"
                },
                "task_id": {
                    "type": "string",
                    "description": "Process/Task ID alias for 'id' (accepted for 'status', 'logs', 'stop', 'kill', 'restart')"
                },
                "process_id": {
                    "type": "string",
                    "description": "Process ID alias for 'id'"
                },
                "port": {
                    "type": "integer",
                    "description": "Target port number to probe (required for 'probe_port'/'check_port')"
                },
                "port_policy": {
                    "type": "string",
                    "enum": ["fallback", "error", "kill", "ignore"],
                    "description": "Port conflict strategy: 'fallback' (auto-assign next available port and inject PORT=...), 'error' (fail immediately), 'kill' (terminate conflicting process), 'ignore' (proceed anyway). Default: 'fallback'"
                },
                "restart_policy": {
                    "type": "string",
                    "enum": ["never", "on_failure", "always"],
                    "description": "Auto-restart watchdog supervision: 'never' (default), 'on_failure' (restart if exit code is non-zero), 'always' (restart on any exit)"
                },
                "auto_restart": {
                    "type": "boolean",
                    "description": "Convenience toggle for auto-restart on failure with 3 retries (default: false)"
                },
                "max_retries": {
                    "type": "integer",
                    "description": "Maximum number of watchdog restart attempts (default: 3)"
                },
                "backoff_ms": {
                    "type": "integer",
                    "description": "Base backoff in milliseconds between watchdog restarts (default: 1000)"
                },
                "url": {
                    "type": "string",
                    "description": "Target URL to capture, audit, or inspect for 'screenshot', 'audit', 'inspect' (optional, defaults to primary URL of running process)"
                },
                "path": {
                    "type": "string",
                    "description": "Target file path relative to workspace to save screenshot PNG (optional, defaults to .minicode/screenshots/...)"
                },
                "mode": {
                    "type": "string",
                    "enum": ["headless", "gui"],
                    "description": "Browser execution mode for browser actions ('headless' or 'gui', default: 'headless')"
                },
                "actions": {
                    "type": "array",
                    "description": "Ordered array of browser action objects to execute sequentially for 'batch'/'browser_batch' (e.g. [{\"action\": \"navigate\", \"url\": \"...\"}, {\"action\": \"fill\", \"ref\": \"@v1:e2\", \"text\": \"alice\"}, {\"action\": \"click\", \"ref\": \"@v1:e3\"}])",
                    "items": {
                        "type": "object"
                    }
                },
                "pattern": {
                    "type": "string",
                    "description": "URL pattern or wildcard to intercept for 'mock_route' (e.g. '*/api/*', 'https://api.example.com/*')"
                },
                "status": {
                    "type": "integer",
                    "description": "Mocked HTTP status code for 'mock_route' (e.g. 200, 401, 404, 500). Default: 200"
                },
                "body": {
                    "type": "string",
                    "description": "Mocked HTTP response body string for 'mock_route' (usually JSON)"
                },
                "content_type": {
                    "type": "string",
                    "description": "Response Content-Type header for 'mock_route' (default: 'application/json')"
                },
                "clear": {
                    "type": "boolean",
                    "description": "Clear all active mock routes if true (used by 'mock_route')"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Subdirectory relative to workspace root in which to run the command"
                },
                "env": {
                    "type": "object",
                    "description": "Optional environment variables key-value map to inject into the process"
                },
                "port_hint": {
                    "type": "integer",
                    "description": "Optional expected localhost port (e.g. 3000, 5173, 8080) for expedited health verification"
                },
                "tail": {
                    "type": "integer",
                    "description": "Number of recent log lines to return (default: 50, used by 'logs')"
                },
                "filter": {
                    "type": "string",
                    "description": "Optional substring filter to apply when querying process logs"
                },
                "viewport": {
                    "type": "string",
                    "enum": ["mobile", "mobile_large", "iphone", "tablet", "ipad", "desktop", "desktop_wide", "reset"],
                    "description": "Device viewport preset for 'emulate'/'browser_emulate'"
                },
                "network": {
                    "type": "string",
                    "enum": ["offline", "slow_3g", "fast_3g", "cable", "wifi", "reset"],
                    "description": "Network throttling condition preset for 'emulate'/'browser_emulate'"
                },
                "profile": {
                    "type": "string",
                    "description": "Named session state profile identifier for 'save_state'/'restore_state' (e.g. 'auth_user')"
                },
                "landscape": {
                    "type": "boolean",
                    "description": "Paper orientation for 'pdf' (true for landscape, false for portrait, default: false)"
                },
                "print_background": {
                    "type": "boolean",
                    "description": "Print background graphics and colors for 'pdf' (default: true)"
                },
                "custom_width": {
                    "type": "integer",
                    "description": "Custom viewport width in pixels for 'emulate'"
                },
                "custom_height": {
                    "type": "integer",
                    "description": "Custom viewport height in pixels for 'emulate'"
                },
                "ref": {
                    "type": "string",
                    "description": "ARIA element reference (e.g. '@v1:e2') or target selector for 'hover' or 'select_option'"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for element interaction, container scrolling, or inspection"
                },
                "option": {
                    "type": "string",
                    "description": "Dropdown option text or value to choose for 'select_option'"
                },
                "direction": {
                    "type": "string",
                    "enum": ["left", "right", "up", "down"],
                    "description": "Scroll direction for 'scroll_horizontal' ('left' or 'right')"
                },
                "distance": {
                    "type": "integer",
                    "description": "Number of pixels to scroll horizontally (default: 500)"
                },
                "subaction": {
                    "type": "string",
                    "enum": ["list", "create", "new", "switch", "close"],
                    "description": "Subaction for 'tabs' ('list', 'create', 'switch', 'close')"
                },
                "target_id": {
                    "type": "string",
                    "description": "Target/Tab ID for browser tab operations ('switch', 'close')"
                },
                "enable": {
                    "type": "boolean",
                    "description": "Toggle high-contrast numbered visual badges on or off for 'badges' (default: true)"
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
        let action = require_str(args, "action", "minitask")?;
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
                        "⚠️ Port {} is OCCUPIED:\n• Conflicting PID: {}\n• Process: {}\n• Command: {}\n• Next Available Port: {}\n\nJSON:\n{}",
                        port, pid_str, proc_str, cmd_str, sugg_str, result_json
                    ))
                } else {
                    Ok(format!(
                        "✔ Port {} is AVAILABLE (ready for binding)\n\nJSON:\n{}",
                        port, result_json
                    ))
                }
            }
            "start" => {
                let command = require_str(args, "command", "minitask")?.to_string();
                let name = opt_str(args, "name").map(|s| s.to_string());
                let p_type_str = opt_str(args, "process_type").unwrap_or("frontend");
                let process_type: DevProcessType = p_type_str.parse().unwrap_or(DevProcessType::Frontend);
                let working_dir = opt_str(args, "working_dir")
                    .or_else(|| opt_str(args, "dir"))
                    .or_else(|| opt_str(args, "path"))
                    .map(std::path::PathBuf::from);
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

                // Brief grace pause to allow early port output detection from stdout
                tokio::time::sleep(Duration::from_millis(300)).await;

                // Re-fetch live summary with detected ports
                let live_summary = registry.get(&summary.id).await.unwrap_or(summary);

                let url_str = live_summary
                    .url
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
            "workers" => {
                let list = registry.list_workers().await;
                if list.is_empty() {
                    return Ok("ℹ No autonomous subagents or delegated workers are currently active.".to_string());
                }

                let mut out = format!("🤖 Active Autonomous Subagents & Workers ({} active):\n\n", list.len());
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
                let browser_details = if filter_type.is_none() || filter_type == Some(DevProcessType::Chrome) {
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
            "stop" | "kill" | "browser_close" => {
                if action == "browser_close" {
                    let stopped = crate::tools::browser::BrowserManager::shutdown_live_engine().await?;
                    let _ = registry.stop(&DevProcessId::from("browser")).await;
                    if stopped {
                        return Ok("✔ Browser session closed successfully. All browser processes terminated.".to_string());
                    } else {
                        return Ok("ℹ No active browser session was running.".to_string());
                    }
                }

                let id_opt = opt_str(args, "id")
                    .or_else(|| opt_str(args, "task_id"))
                    .or_else(|| opt_str(args, "process_id"))
                    .or_else(|| opt_str(args, "target"));

                if id_opt == Some("all") {
                    let count = registry.kill_all().await?;
                    let _ = crate::tools::browser::BrowserManager::shutdown_live_engine().await;
                    return Ok(format!("✔ Terminated all {} active development processes and browser sessions.", count));
                }

                let id_to_stop = match id_opt {
                    Some(s) => s.to_string(),
                    None => {
                        let list = registry.list().await;
                        if list.len() == 1 {
                            list[0].id.as_str().to_string()
                        } else if list.is_empty() {
                            if crate::tools::browser::BrowserManager::is_live_engine_running().await {
                                let _ = crate::tools::browser::BrowserManager::shutdown_live_engine().await;
                                return Ok("✔ Active browser session closed successfully.".to_string());
                            }
                            return Ok("ℹ No active development processes or browser sessions to stop.".to_string());
                        } else {
                            return Err(ToolError::invalid_args("minitask", "Multiple processes are active. Please specify 'id' or 'task_id' (or 'all').").into());
                        }
                    }
                };

                if id_to_stop == "browser" || id_to_stop == "chrome" {
                    let stopped = crate::tools::browser::BrowserManager::shutdown_live_engine().await?;
                    let _ = registry.stop(&DevProcessId::from("browser")).await;
                    if stopped {
                        return Ok("✔ Browser session closed successfully.".to_string());
                    } else {
                        return Ok("ℹ No active browser session was running.".to_string());
                    }
                }

                let id = DevProcessId::from(id_to_stop.clone());
                registry.stop(&id).await?;

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
                    "✔ Process/worker '{}' stopped successfully{}.",
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
                let count = registry.kill_all().await?;
                let _ = crate::tools::browser::BrowserManager::shutdown_live_engine().await;
                Ok(format!("✔ Terminated {} active development processes and browser sessions.", count))
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
                reason: format!("Unknown action '{}'. Expected: start, schedule, list, status, logs, stop, restart, resources, kill_all, screenshot, workers, swarms, probe_port, audit, inspect, batch, mock_route, browser_status, browser_close, emulate, save_state, restore_state, pdf, check_injection, hover, select_option, scroll_horizontal, badges, tabs, metrics", unknown),
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
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 2. List
        let list_args = json!({ "action": "list" });
        let list_res = dispatch("minitask", &list_args, temp.path())
            .await
            .unwrap()
            .unwrap();
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
        let kill_args = json!({ "action": "kill_all" });
        let _ = dispatch("minitask", &kill_args, temp.path()).await;
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
        let kill_args = json!({ "action": "kill_all" });
        let _ = dispatch("minitask", &kill_args, temp.path()).await;
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

        assert!(actions.contains(&"audit"));
        assert!(actions.contains(&"qa_audit"));
        assert!(actions.contains(&"inspect"));
        assert!(actions.contains(&"inspect_dom"));
        assert!(actions.contains(&"batch"));
        assert!(actions.contains(&"browser_batch"));
        assert!(actions.contains(&"mock_route"));
        assert!(actions.contains(&"browser_status"));
        assert!(actions.contains(&"browser_debug"));
        assert!(actions.contains(&"browser_close"));
        assert!(actions.contains(&"emulate"));
        assert!(actions.contains(&"browser_emulate"));
        assert!(actions.contains(&"save_state"));
        assert!(actions.contains(&"browser_save_state"));
        assert!(actions.contains(&"restore_state"));
        assert!(actions.contains(&"browser_restore_state"));
        assert!(actions.contains(&"pdf"));
        assert!(actions.contains(&"browser_pdf"));
        assert!(actions.contains(&"check_injection"));
        assert!(actions.contains(&"browser_check_injection"));
        assert!(actions.contains(&"hover"));
        assert!(actions.contains(&"browser_hover"));
        assert!(actions.contains(&"select_option"));
        assert!(actions.contains(&"browser_select_option"));
        assert!(actions.contains(&"scroll_horizontal"));
        assert!(actions.contains(&"browser_scroll_horizontal"));
        assert!(actions.contains(&"badges"));
        assert!(actions.contains(&"browser_badges"));
        assert!(actions.contains(&"tabs"));
        assert!(actions.contains(&"browser_tabs"));
        assert!(actions.contains(&"metrics"));
        assert!(actions.contains(&"browser_metrics"));
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
}
