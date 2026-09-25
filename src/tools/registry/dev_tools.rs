//! MiniDev Runtime Orchestrator Tool (Tool 168: `mini_dev`).
//! Provides unified lifecycle management (CRUD), port auto-discovery,
//! process group isolation, and resource telemetry for development servers,
//! backends, Docker containers, and background scripts.

use crate::agent::provider::ToolSchema;
use crate::dev::models::{DevProcessId, DevProcessType, SpawnDevRequest};
use crate::dev::registry::get_global_dev_registry;
use crate::error::{DevError, Result, ToolError};
use crate::tools::param::*;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Returns the schema for `mini_dev` (Tool 168).
pub fn get_schemas() -> Vec<ToolSchema> {
    vec![ToolSchema {
        name: "mini_dev".to_string(),
        description: "Manage long-running development servers, backend APIs, Docker stacks, scripts, and browsers with full lifecycle CRUD, dynamic port discovery, process group isolation, and real-time resource tracking (CPU/RSS memory). Automatically terminates all managed processes when minicode exits.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["start", "list", "status", "logs", "stop", "restart", "resources", "kill_all", "screenshot", "workers"],
                    "description": "Lifecycle action to perform: 'start' (launch process), 'list'/'status' (inspect active processes), 'logs' (tail output), 'stop' (gracefully terminate process), 'restart' (cycle process), 'resources' (CPU & memory telemetry), 'kill_all' (terminate all active processes), 'screenshot' (capture visual PNG of running server or URL), 'workers' (list active autonomous subagents and delegated tasks)"
                },
                "command": {
                    "type": "string",
                    "description": "Shell command to run (required for 'start', e.g. 'npm run dev', 'cargo run', 'python app.py')"
                },
                "name": {
                    "type": "string",
                    "description": "Optional human-readable label for the process (e.g. 'frontend-vite', 'backend-axum', 'auth-service')"
                },
                "process_type": {
                    "type": "string",
                    "enum": ["frontend", "backend", "docker", "script", "browser", "worker", "subagent"],
                    "description": "Category of process (default: 'frontend')"
                },
                "id": {
                    "type": "string",
                    "description": "Process ID (required for 'status', 'logs', 'stop', 'restart', or target for 'screenshot')"
                },
                "url": {
                    "type": "string",
                    "description": "Target URL to capture for 'screenshot' action (optional, defaults to primary URL of running process)"
                },
                "path": {
                    "type": "string",
                    "description": "Target file path relative to workspace to save screenshot PNG (optional, defaults to .minicode/screenshots/...)"
                },
                "mode": {
                    "type": "string",
                    "enum": ["headless", "gui"],
                    "description": "Browser execution mode for 'screenshot' ('headless' or 'gui', default: 'headless')"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Subdirectory relative to workspace root in which to run the command"
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
                }
            },
            "required": ["action"]
        }),
    }]
}

/// Dispatches execution of the `mini_dev` tool.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    if tool_name != "mini_dev" {
        return None;
    }

    Some(async move {
        let action = require_str(args, "action", "mini_dev")?;
        let registry = get_global_dev_registry();

        match action {
            "start" => {
                let command = require_str(args, "command", "mini_dev")?.to_string();
                let name = opt_str(args, "name").map(|s| s.to_string());
                let p_type_str = opt_str(args, "process_type").unwrap_or("frontend");
                let process_type: DevProcessType = p_type_str.parse().unwrap_or(DevProcessType::Frontend);
                let working_dir = opt_str(args, "working_dir")
                    .or_else(|| opt_str(args, "dir"))
                    .or_else(|| opt_str(args, "path"))
                    .map(std::path::PathBuf::from);
                let port_hint = opt_u64(args, "port_hint").map(|p| p as u16);

                let req = SpawnDevRequest {
                    command,
                    name,
                    process_type,
                    working_dir,
                    extra_env: HashMap::new(),
                    port_hint,
                    max_memory_mb: None,
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

                Ok(format!(
                    "🚀 Development process launched successfully:\n• ID: {}\n• Name: {}\n• Type: {:?}\n• PID: {}\n• Status: {:?}\n• Primary URL: {}\n• Active Ports: {:?}",
                    live_summary.id,
                    live_summary.name,
                    live_summary.process_type,
                    live_summary.pid.unwrap_or(0),
                    live_summary.status,
                    url_str,
                    live_summary.ports,
                ))
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
            "list" => {
                let filter_type = opt_str(args, "process_type")
                    .map(DevProcessType::from_str_loose);
                let list = registry.list_filtered(filter_type).await;
                if list.is_empty() {
                    if let Some(ft) = filter_type {
                        return Ok(format!("ℹ No processes of type '{:?}' are currently running.", ft));
                    } else {
                        return Ok("ℹ No development processes are currently running.".to_string());
                    }
                }

                let header = if let Some(ft) = filter_type {
                    format!("📋 Managed Processes [{:?}] ({} active):\n\n", ft, list.len())
                } else {
                    format!("📋 Managed Development Processes ({} active):\n\n", list.len())
                };
                let mut out = header;
                for p in list {
                    let url_disp = p.url.as_deref().unwrap_or("-");
                    out.push_str(&format!(
                        "• [{}] {} ({:?}) | Status: {:?} | URL: {} | PID: {} | CPU: {:.1}% | RSS: {:.1}MB | Uptime: {}s\n",
                        p.id,
                        p.name,
                        p.process_type,
                        p.status,
                        url_disp,
                        p.pid.unwrap_or(0),
                        p.cpu_percent,
                        p.memory_rss_mb,
                        p.uptime_secs,
                    ));
                }
                Ok(out)
            }
            "status" => {
                if let Some(id_str) = opt_str(args, "id") {
                    let id = DevProcessId::from(id_str);
                    let summary = registry
                        .get(&id)
                        .await
                        .ok_or_else(|| DevError::NotFound(id_str.to_string()))?;

                    let url_disp = summary.url.as_deref().unwrap_or("none");
                    Ok(format!(
                        "📊 Process Status for '{}':\n• Name: {}\n• Type: {:?}\n• Status: {:?}\n• PID: {}\n• URL: {}\n• Ports: {:?}\n• CPU: {:.1}%\n• Memory RSS: {:.1} MB\n• Uptime: {}s",
                        summary.id,
                        summary.name,
                        summary.process_type,
                        summary.status,
                        summary.pid.unwrap_or(0),
                        url_disp,
                        summary.ports,
                        summary.cpu_percent,
                        summary.memory_rss_mb,
                        summary.uptime_secs
                    ))
                } else {
                    // Fall back to listing all processes
                    let list = registry.list().await;
                    if list.is_empty() {
                        return Ok("ℹ No development processes are currently running.".to_string());
                    }
                    let mut out = format!("📋 Managed Development Processes ({} active):\n\n", list.len());
                    for p in list {
                        let url_disp = p.url.as_deref().unwrap_or("-");
                        out.push_str(&format!(
                            "• [{}] {} ({:?}) | Status: {:?} | URL: {} | PID: {} | CPU: {:.1}% | RSS: {:.1}MB\n",
                            p.id,
                            p.name,
                            p.process_type,
                            p.status,
                            url_disp,
                            p.pid.unwrap_or(0),
                            p.cpu_percent,
                            p.memory_rss_mb,
                        ));
                    }
                    Ok(out)
                }
            }
            "logs" => {
                let id_str = require_str(args, "id", "mini_dev")?;
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
            "stop" => {
                let id_str = require_str(args, "id", "mini_dev")?;
                let id = DevProcessId::from(id_str);
                registry.stop(&id).await?;
                Ok(format!("✔ Process/worker '{}' stopped successfully.", id_str))
            }
            "restart" => {
                let id_str = require_str(args, "id", "mini_dev")?;
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
            "kill_all" => {
                let count = registry.kill_all().await?;
                Ok(format!("✔ Terminated {} active development processes.", count))
            }
            "screenshot" => {
                let explicit_url = opt_str(args, "url");
                let id_opt = opt_str(args, "id").map(DevProcessId::from);
                let custom_path = opt_str(args, "path");
                let mode_str = opt_str(args, "mode").unwrap_or("headless");
                let mode = match mode_str {
                    "gui" => crate::tools::browser::BrowserMode::Gui,
                    _ => crate::tools::browser::BrowserMode::Headless,
                };

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
                                name: "mini_dev".to_string(),
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
            unknown => Err(ToolError::InvalidArguments {
                name: "mini_dev".to_string(),
                reason: format!("Unknown action '{}'. Expected: start, list, status, logs, stop, restart, resources, kill_all, screenshot, workers", unknown),
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
    async fn test_mini_dev_schema_valid() {
        let schemas = get_schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "mini_dev");
    }

    #[tokio::test]
    async fn test_mini_dev_dispatch_lifecycle() {
        let temp = tempdir().unwrap();

        // 1. Start a mock server
        let start_args = json!({
            "action": "start",
            "command": "echo 'Server listening on http://localhost:8765'; sleep 30",
            "name": "mock-api",
            "process_type": "backend"
        });

        let start_res = dispatch("mini_dev", &start_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(start_res.contains("Development process launched successfully"));

        // Wait brief moment for logs and ports
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 2. List
        let list_args = json!({ "action": "list" });
        let list_res = dispatch("mini_dev", &list_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(list_res.contains("mock-api"));

        // 3. Resources
        let res_args = json!({ "action": "resources" });
        let res_output = dispatch("mini_dev", &res_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(res_output.contains("Runtime Resource Telemetry"));

        // 4. Kill all
        let kill_args = json!({ "action": "kill_all" });
        let kill_res = dispatch("mini_dev", &kill_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(kill_res.contains("Terminated"));
    }

    #[tokio::test]
    async fn test_mini_dev_workers_dispatch() {
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
        let workers_res = dispatch("mini_dev", &workers_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(workers_res.contains("Subagent (Tester) - Verify Auth"));
        assert!(workers_res.contains("worker-subagent-test-99"));

        // 2. Query via action: "list", process_type: "worker"
        let list_worker_args = json!({ "action": "list", "process_type": "worker" });
        let list_worker_res = dispatch("mini_dev", &list_worker_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(list_worker_res.contains("Subagent (Tester) - Verify Auth"));

        // 3. Query logs via raw id
        let logs_args = json!({ "action": "logs", "id": "subagent-test-99" });
        let logs_res = dispatch("mini_dev", &logs_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(logs_res.contains("Worker initializing sandbox"));

        // 4. Stop worker
        let stop_args = json!({ "action": "stop", "id": "subagent-test-99" });
        let stop_res = dispatch("mini_dev", &stop_args, temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(stop_res.contains("stopped successfully"));
        assert!(cancel_flag.load(std::sync::atomic::Ordering::SeqCst));
    }
}
