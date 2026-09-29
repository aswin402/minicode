use crate::error::{Result, ToolError};
use crate::sandbox::env::build_sanitized_command;
#[cfg(target_os = "linux")]
use crate::sandbox::landlock::apply_landlock_sandbox;
use std::path::Path;
use std::time::Duration;

/// Resolves the context window token limit for tool execution.
/// First checks the global active context limit, then environment variables, then workspace configuration.
pub fn resolve_context_window(workspace_root: &Path) -> usize {
    let active = crate::context::budget::donut::get_active_context_limit();
    if active > 0 {
        return active;
    }

    if let Ok(val) = std::env::var("MINICODE_CONTEXT_WINDOW") {
        if let Ok(parsed) = val.parse::<usize>() {
            return parsed;
        }
    }

    if let Ok(model) = std::env::var("MINICODE_MODEL") {
        return crate::agent::models::get_model_context_limit(&model);
    }

    // Check workspace-local config.toml if it exists
    let local_cfg = workspace_root
        .join(crate::constants::WORKSPACE_DIR_NAME)
        .join(crate::constants::CONFIG_FILE_NAME);
    if local_cfg.exists() {
        if let Ok(content) = std::fs::read_to_string(&local_cfg) {
            if let Ok(val) = toml::from_str::<serde_json::Value>(&content) {
                if let Some(ctx) = val
                    .get("provider")
                    .and_then(|p| p.get("context_window"))
                    .and_then(|c| c.as_u64())
                {
                    return ctx as usize;
                }
                if let Some(model) = val
                    .get("provider")
                    .and_then(|p| p.get("model"))
                    .and_then(|m| m.as_str())
                {
                    return crate::agent::models::get_model_context_limit(model);
                }
            }
        }
    }

    0
}

/// Determines the optimal execution timeout dynamically based on command characteristics,
/// environment configurations, and command workload intensity.
///
/// Prevents premature timeout when an LLM echoes schema defaults (e.g. 30s) on heavy build or install tasks,
/// while dynamically scaling time based on environment settings and hardware concurrency.
pub fn resolve_smart_exec_timeout(command_str: &str, explicit_timeout: Option<u64>) -> Duration {
    // 1. Dynamic base timeout from environment or default
    let env_timeout = std::env::var("MINICODE_CMD_TIMEOUT")
        .or_else(|_| std::env::var("MINICODE_TIMEOUT"))
        .ok()
        .and_then(|v| v.parse::<u64>().ok());

    let base_timeout_secs = env_timeout.unwrap_or(crate::constants::EXEC_DEFAULT_TIMEOUT_SECS);

    // 2. Classify command intensity and compute dynamic duration
    let cmd = command_str.trim().to_lowercase();
    let (dynamic_floor_secs, is_heavy) = compute_dynamic_command_floor(&cmd, base_timeout_secs);

    // 3. Resolve with explicit timeout (if provided by LLM or caller)
    if let Some(explicit) = explicit_timeout {
        if is_heavy {
            // If heavy task and caller passed a low explicit timeout (e.g. schema default 30s or lower),
            // dynamically elevate to dynamic_floor_secs.
            // If caller explicitly asked for more time (e.g. 300s), respect their choice.
            Duration::from_secs(std::cmp::max(explicit, dynamic_floor_secs))
        } else {
            // For standard lightweight commands, respect whatever explicit timeout the caller requested
            Duration::from_secs(explicit)
        }
    } else {
        Duration::from_secs(dynamic_floor_secs)
    }
}

/// Dynamically calculates the minimum execution timeout based on command semantics,
/// package manager flags, and environment hardware concurrency.
/// Returns (computed_timeout_seconds, is_heavy_operation).
fn compute_dynamic_command_floor(cmd: &str, base_secs: u64) -> (u64, bool) {
    // Check for build-specific environment overrides
    if let Ok(val) = std::env::var("MINICODE_BUILD_TIMEOUT_SECS") {
        if let Ok(custom) = val.parse::<u64>() {
            return (custom, true);
        }
    }

    // Detect system concurrency (low core counts need more time for compilation & extraction)
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let concurrency_multiplier = if cores <= 2 { 1.5 } else { 1.0 };

    // Heavy initial downloads & package resolution
    let is_heavy_install = cmd.contains("npm install")
        || cmd.contains("npm i ")
        || cmd == "npm i"
        || cmd.contains("npm ci")
        || cmd.contains("pnpm install")
        || cmd.contains("pnpm i ")
        || cmd == "pnpm i"
        || cmd.contains("yarn install")
        || cmd.contains("bun install")
        || cmd.contains("cargo fetch")
        || cmd.contains("pip install")
        || cmd.contains("uv sync")
        || cmd.contains("poetry install");

    // Compilation & multi-step builds
    let is_heavy_build = cmd.contains("cargo build")
        || cmd.contains("cargo test")
        || cmd.contains("cargo check")
        || cmd.contains("go build")
        || cmd.contains("go test")
        || cmd.contains("mvn ")
        || cmd.contains("gradle ")
        || cmd.contains("vite build")
        || cmd.contains("next build")
        || cmd.contains("tsc ");

    if is_heavy_install {
        // Fresh installs: 4x base scaled by hardware concurrency (e.g. 30s * 4 * 1.0 = 120s, or 180s on low-core machines)
        (
            ((base_secs as f64 * 4.0 * concurrency_multiplier) as u64).max(120),
            true,
        )
    } else if is_heavy_build {
        // Builds & tests: 3x base scaled by hardware concurrency (e.g. 30s * 3 = 90s)
        (
            ((base_secs as f64 * 3.0 * concurrency_multiplier) as u64).max(90),
            true,
        )
    } else if cmd.starts_with("npm ")
        || cmd.starts_with("pnpm ")
        || cmd.starts_with("yarn ")
        || cmd.starts_with("bun ")
        || cmd.starts_with("cargo ")
        || cmd.starts_with("uv ")
        || cmd.starts_with("pip ")
        || cmd.starts_with("pip3 ")
    {
        // General package manager commands
        (
            ((base_secs as f64 * 2.0 * concurrency_multiplier) as u64).max(60),
            true,
        )
    } else {
        (base_secs, false)
    }
}

/// Determines if a shell command represents a persistent web server, dev daemon, or background process.
pub fn is_daemon_or_server_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    let lower = trimmed.to_lowercase();

    // Explicitly exclude process management, inspection, or terminating commands
    if lower.starts_with("kill ")
        || lower.starts_with("pkill ")
        || lower.starts_with("killall ")
        || lower.starts_with("pgrep ")
        || lower.starts_with("ps ")
        || lower.starts_with("grep ")
        || lower.starts_with("echo ")
        || lower.starts_with("which ")
        || lower.starts_with("cat ")
    {
        return false;
    }

    if trimmed.ends_with('&') {
        return true;
    }
    lower.contains("python3 -m http.server")
        || lower.contains("python -m http.server")
        || lower.contains("python3 -m simplehttpserver")
        || lower.contains("python -m simplehttpserver")
        || lower.contains("npx serve")
        || lower.contains("npx http-server")
        || lower.contains("npm run dev")
        || lower.contains("npm run start")
        || lower.contains("npm start")
        || lower.contains("pnpm dev")
        || lower.contains("pnpm start")
        || lower.contains("yarn dev")
        || lower.contains("yarn start")
        || lower.contains("bun dev")
        || lower.contains("bun run dev")
        || lower.contains("vite")
        || lower.contains("cargo watch")
        || lower.contains("uvicorn ")
        || lower.contains("flask run")
        || lower.contains("fastapi dev")
        || lower.contains("fastapi run")
        || lower.contains("docker run")
        || lower.contains("docker compose up")
        || lower.contains("docker-compose up")
}

/// Spawns a persistent background dev server or daemon task under MiniDevRegistry / MiniTask Manager.
pub async fn exec_daemon_or_server(workspace_root: &Path, command_str: &str) -> Result<String> {
    let registry = crate::dev::registry::get_global_dev_registry();
    let clean_cmd = command_str.trim().trim_end_matches('&').trim();
    let port_hint = if clean_cmd.contains("http.server") || clean_cmd.contains("SimpleHTTPServer") {
        clean_cmd
            .split_whitespace()
            .find_map(|w| {
                w.parse::<u16>()
                    .ok()
                    .filter(|&p| (1024..=65535).contains(&p))
            })
            .or(Some(8000))
    } else {
        crate::dev::ports::scan_ports_from_output(command_str)
            .into_iter()
            .next()
    };
    let lower = clean_cmd.to_lowercase();
    let name_prefix = if lower.contains("http.server") || lower.contains("simplehttpserver") {
        "python-webserver"
    } else if lower.contains("vite") {
        "vite-dev"
    } else if lower.contains("cargo watch") {
        "cargo-watch"
    } else if lower.contains("uvicorn") || lower.contains("fastapi") || lower.contains("flask") {
        "backend-api"
    } else {
        "bg-task"
    };

    let req = crate::dev::models::SpawnDevRequest {
        command: clean_cmd.to_string(),
        name: Some(name_prefix.to_string()),
        process_type: crate::dev::models::DevProcessType::from_str_loose(clean_cmd),
        working_dir: Some(workspace_root.to_path_buf()),
        extra_env: std::collections::HashMap::new(),
        port_hint,
        max_memory_mb: None,
        port_policy: Some(crate::dev::models::PortConflictPolicy::Fallback),
        restart_policy: Some(crate::dev::models::RestartPolicy::Never),
    };

    let summary = registry.spawn(workspace_root, req).await?;

    // Wait 800ms for process to initialize or report immediate fast crash
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // Inspect updated status after startup
    if let Some(updated) = registry.get(&summary.id).await {
        if let crate::dev::models::DevProcessStatus::Exited(Some(code)) = updated.status {
            if code != 0 {
                let logs = registry
                    .logs(&summary.id, 20, None)
                    .await
                    .unwrap_or_default();
                return Err(ToolError::CommandExec(format!(
                    "Background process '{}' (PID {:?}) failed to start (exit code {}):\n{}",
                    summary.name,
                    summary.pid,
                    code,
                    logs.join("\n")
                ))
                .into());
            }
        }

        let mut output = String::new();
        output.push_str(&format!(
            "🚀 Background task launched successfully and registered with MiniTask Manager:\n• Task ID: {}\n• Name: {}\n• Type: {}\n• PID: {}\n• Status: {}\n",
            updated.id.as_str(),
            updated.name,
            updated.process_type,
            updated.pid.map(|p| p.to_string()).unwrap_or_else(|| "none".to_string()),
            updated.status
        ));

        if let Some(ref url) = updated.url {
            output.push_str(&format!("• Primary URL: {}\n", url));
        } else if !updated.ports.is_empty() {
            output.push_str(&format!(
                "• Active Ports: {:?}\n• URL: http://localhost:{}\n",
                updated.ports, updated.ports[0]
            ));
        } else if let Some(hint) = port_hint {
            output.push_str(&format!(
                "• Expected Port: {}\n• URL: http://localhost:{}\n",
                hint, hint
            ));
        }

        output.push_str("\nProcess is actively running and supervised under MiniTask Manager (with automatic OOM & runaway watchdog).\nUse 'minitask' or '/tasks' in TUI to inspect logs, view telemetry, or stop the task.");

        Ok(output)
    } else {
        Ok(format!(
            "🚀 Background process launched with ID: {}",
            summary.id.as_str()
        ))
    }
}

/// Executes a shell command inside the sandboxed workspace environment using active context window limit.
pub async fn exec_cmd(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
) -> Result<String> {
    exec_cmd_with_context(workspace_root, command_str, timeout_secs, None).await
}

/// Executes a shell command with an optional explicit context window limit.
pub async fn exec_cmd_with_context(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
    explicit_context: Option<usize>,
) -> Result<String> {
    exec_cmd_full(
        workspace_root,
        command_str,
        timeout_secs,
        explicit_context,
        false,
    )
    .await
}

/// Executes a shell command with full control over context limit and background daemon execution.
pub async fn exec_cmd_full(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
    explicit_context: Option<usize>,
    is_daemon: bool,
) -> Result<String> {
    if is_daemon || is_daemon_or_server_command(command_str) {
        return exec_daemon_or_server(workspace_root, command_str).await;
    }

    let timeout = resolve_smart_exec_timeout(command_str, timeout_secs);

    let mut std_cmd = build_sanitized_command("sh", workspace_root);
    std_cmd.arg("-c").arg(command_str);

    #[cfg(target_os = "linux")]
    {
        let ws = workspace_root.to_path_buf();
        unsafe {
            use std::os::unix::process::CommandExt;
            std_cmd.pre_exec(move || {
                let _ = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                apply_landlock_sandbox(&ws, true).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("Landlock sandbox failed: {}", e),
                    )
                })
            });
        }
    }

    let mut tokio_cmd = tokio::process::Command::from(std_cmd);
    tokio_cmd.kill_on_drop(true);
    tokio_cmd.stdout(std::process::Stdio::piped());
    tokio_cmd.stderr(std::process::Stdio::piped());

    #[cfg(unix)]
    {
        tokio_cmd.process_group(0);
    }

    let mut child = tokio_cmd
        .spawn()
        .map_err(|e| ToolError::CommandExec(format!("Process spawn error: {}", e)))?;

    let child_pid = child.id().unwrap_or(0);
    if child_pid > 0 {
        crate::dev::registry::get_global_dev_registry().register_external_pid(child_pid);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let max_read_bytes = (crate::constants::EXEC_MAX_OUTPUT_BYTES as u64) + 1;

    let read_stdout = async {
        if let Some(mut out) = stdout {
            use tokio::io::AsyncReadExt;
            let mut limited = (&mut out).take(max_read_bytes);
            let _ = limited.read_to_end(&mut stdout_buf).await;
            // Drain any remaining output to sink so child process does not block on full OS pipe buffer
            let _ = tokio::io::copy(&mut out, &mut tokio::io::sink()).await;
        }
    };

    let read_stderr = async {
        if let Some(mut err) = stderr {
            use tokio::io::AsyncReadExt;
            let mut limited = (&mut err).take(max_read_bytes);
            let _ = limited.read_to_end(&mut stderr_buf).await;
            // Drain any remaining output to sink so child process does not block on full OS pipe buffer
            let _ = tokio::io::copy(&mut err, &mut tokio::io::sink()).await;
        }
    };

    let run_fut = async {
        tokio::join!(read_stdout, read_stderr);
        child.wait().await
    };

    let status = match tokio::time::timeout(timeout, run_fut).await {
        Ok(Ok(s)) => {
            #[cfg(unix)]
            if child_pid > 0 {
                let is_any_alive = unsafe {
                    libc::kill(-(child_pid as i32), 0) == 0 || libc::kill(child_pid as i32, 0) == 0
                };
                if !is_any_alive {
                    crate::dev::registry::get_global_dev_registry()
                        .unregister_external_pid(child_pid);
                }
            }
            s
        }
        Ok(Err(e)) => {
            #[cfg(unix)]
            if child_pid > 0 {
                crate::dev::registry::get_global_dev_registry().unregister_external_pid(child_pid);
            }
            return Err(ToolError::CommandExec(format!("Process execution error: {}", e)).into());
        }
        Err(_) => {
            #[cfg(unix)]
            if child_pid > 0 {
                let p_i32 = child_pid as i32;
                let descendants = crate::dev::ports::find_all_descendants(child_pid);
                unsafe {
                    let _ = libc::kill(-p_i32, libc::SIGTERM);
                    let _ = libc::kill(p_i32, libc::SIGTERM);
                    for &d in &descendants {
                        let _ = libc::kill(d as i32, libc::SIGTERM);
                        let _ = libc::kill(-(d as i32), libc::SIGTERM);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(
                    crate::constants::PROCESS_KILL_GRACE_PERIOD_MS,
                ))
                .await;
                unsafe {
                    let _ = libc::kill(-p_i32, libc::SIGKILL);
                    let _ = libc::kill(p_i32, libc::SIGKILL);
                    for &d in &descendants {
                        let _ = libc::kill(d as i32, libc::SIGKILL);
                        let _ = libc::kill(-(d as i32), libc::SIGKILL);
                    }
                }
                crate::dev::registry::get_global_dev_registry().unregister_external_pid(child_pid);
            }
            let _ = child.kill().await;
            return Err(ToolError::CommandTimeout {
                timeout_secs: timeout.as_secs(),
            }
            .into());
        }
    };

    let stdout_str = String::from_utf8_lossy(&stdout_buf);
    let stderr_str = String::from_utf8_lossy(&stderr_buf);

    let mut combined = String::new();
    if !stdout_str.is_empty() {
        combined.push_str(&stdout_str);
    }
    if !stderr_str.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str("[stderr]: ");
        combined.push_str(&stderr_str);
    }

    if combined.len() > crate::constants::EXEC_MAX_OUTPUT_BYTES {
        let valid_end = combined.floor_char_boundary(crate::constants::EXEC_MAX_OUTPUT_BYTES);
        let truncated = &combined[..valid_end];
        combined = format!(
            "{}\n\n[... Output truncated: exceeded max limit ...]",
            truncated
        );
    }

    let status_code = status
        .code()
        .unwrap_or(crate::constants::SIGNAL_KILLED_EXIT_CODE);
    let exit_code = status.code();

    // Preserve full execution output on disk if output exceeds threshold (Phase 115, Phase 117)
    let context_window = explicit_context.unwrap_or_else(|| resolve_context_window(workspace_root));
    let line_threshold = if context_window >= crate::constants::CONTEXT_WINDOW_128K {
        crate::constants::DONUT_EXTENDED_THRESHOLD_LINES
    } else {
        crate::constants::DONUT_STANDARD_THRESHOLD_LINES
    };

    let total_lines = combined.lines().count();
    let is_truncated =
        total_lines > line_threshold || combined.len() > crate::constants::EXEC_MAX_OUTPUT_BYTES;

    let log_notice = if is_truncated {
        let logs_dir = workspace_root.join(".minicode").join("logs");
        if let Err(e) = std::fs::create_dir_all(&logs_dir) {
            tracing::debug!("Could not create .minicode/logs: {}", e);
        }
        let log_file = logs_dir.join("last_exec.log");
        if let Err(e) = std::fs::write(&log_file, &combined) {
            tracing::debug!("Could not write last_exec.log: {}", e);
        }
        format!(
            "\n[... Truncated: Full {} lines saved to .minicode/logs/last_exec.log. Use read_file with start_line/end_line to inspect specific lines ...]",
            total_lines
        )
    } else {
        String::new()
    };

    let rtk_res = super::rtk_filter::RtkFilter::filter_for_context(
        command_str,
        &combined,
        exit_code,
        context_window,
    );
    let mut compacted = super::compactor::compact_tool_output_for_context(
        command_str,
        &rtk_res.content,
        exit_code,
        context_window,
    );
    if !log_notice.is_empty() && !compacted.contains("last_exec.log") {
        compacted.push_str(&log_notice);
    }
    compacted = crate::security::sanitize_text(&compacted);

    if !status.success() {
        let frames = crate::context::search::fault_localizer::FaultLocalizer::extract_trace_frames(
            &combined,
        );
        let mut fault_hint = String::new();
        if !frames.is_empty() {
            fault_hint.push_str("\n\n[Where] 📍 Probable Fault Sites:\n");
            let mut seen_locations = std::collections::HashSet::new();
            for frame in frames.iter().take(4) {
                let key = (frame.file_path.clone(), frame.line_number);
                if seen_locations.insert(key) {
                    let start_suggest = frame.line_number.saturating_sub(10).max(1);
                    let end_suggest = frame.line_number + 15;
                    fault_hint.push_str(&format!(
                        "  • `{}:{}` -> Suggested action: `read_file(path: \"{}\", start_line: {}, end_line: {})`\n",
                        frame.file_path, frame.line_number, frame.file_path, start_suggest, end_suggest
                    ));
                }
            }
            fault_hint.push_str("\n[Suggested Next Action]\nInspect the failing code envelopes at the fault sites above and apply surgical repairs using `patch_file`.");
        }

        let out_body = if compacted.trim().is_empty() {
            "(No output captured on stdout/stderr)".to_string()
        } else {
            compacted
        };

        return Err(ToolError::CommandExec(format!(
            "Command exited with non-zero status ({status_code}):\n{}{}",
            out_body, fault_hint
        ))
        .into());
    }

    if compacted.trim().is_empty() {
        Ok("Command executed successfully (no output).".to_string())
    } else {
        Ok(compacted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exec_echo() {
        let temp_dir = std::env::temp_dir();
        let out = exec_cmd(&temp_dir, "echo 'hello minicode'", Some(5))
            .await
            .unwrap();
        assert!(out.contains("hello minicode"));
    }

    #[tokio::test]
    async fn test_exec_timeout() {
        let temp_dir = std::env::temp_dir();
        let res = exec_cmd(&temp_dir, "sleep 3", Some(1)).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_exec_nonzero_exit_returns_err_with_fault_sites() {
        let temp_dir = std::env::temp_dir();
        let out = exec_cmd(
            &temp_dir,
            "echo 'error[E0425]: cannot find value `foo` in this scope\n  --> src/main.rs:42:15'; exit 1",
            Some(5),
        )
        .await;
        assert!(out.is_err());
        let err_msg = out.unwrap_err().to_string();
        assert!(err_msg.contains("non-zero status (1)"));
        assert!(err_msg.contains("Probable Fault Sites"));
        assert!(err_msg.contains("src/main.rs:42"));
    }

    #[test]
    fn test_resolve_smart_exec_timeout() {
        // Explicit timeout for lightweight commands is honored
        assert_eq!(
            resolve_smart_exec_timeout("ls -la", Some(10)),
            Duration::from_secs(10)
        );

        // Package managers dynamically scale up even if caller passed schema default 30s
        assert!(resolve_smart_exec_timeout("npm install lucide-react", Some(30)).as_secs() >= 120);

        // Heavy install operations dynamically scale to at least 120s
        assert!(resolve_smart_exec_timeout("npm install lucide-react", None).as_secs() >= 120);
        assert!(resolve_smart_exec_timeout("pnpm add react", None).as_secs() >= 60);
        assert!(resolve_smart_exec_timeout("cargo test -j 1", None).as_secs() >= 90);
        assert!(resolve_smart_exec_timeout("uv sync", None).as_secs() >= 120);

        // Explicit large timeout is honored
        assert_eq!(
            resolve_smart_exec_timeout("npm install lucide-react", Some(300)),
            Duration::from_secs(300)
        );

        // Regular commands stay at default 30s (or dynamic base)
        assert_eq!(
            resolve_smart_exec_timeout("ls -la", None),
            Duration::from_secs(30)
        );
        assert_eq!(
            resolve_smart_exec_timeout("cat src/main.rs", None),
            Duration::from_secs(30)
        );
    }
}
