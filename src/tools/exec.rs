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

static CONFIGURED_EXEC_TIMEOUT_SECS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(crate::constants::EXEC_DEFAULT_TIMEOUT_SECS);

/// Updates the configured base execution timeout from `config.agent.timeout` or CLI `--timeout`.
pub fn set_configured_exec_timeout(secs: u64) {
    CONFIGURED_EXEC_TIMEOUT_SECS.store(secs.max(5), std::sync::atomic::Ordering::Relaxed);
}

/// Returns the active configured base execution timeout in seconds.
pub fn get_configured_exec_timeout() -> u64 {
    CONFIGURED_EXEC_TIMEOUT_SECS
        .load(std::sync::atomic::Ordering::Relaxed)
        .max(5)
}

/// Determines the optimal execution timeout dynamically based on command characteristics,
/// environment configurations, and command workload intensity.
///
/// Prevents premature timeout when an LLM echoes schema defaults (e.g. 30s) on heavy build or install tasks,
/// while dynamically scaling time based on environment settings and hardware concurrency.
pub fn resolve_smart_exec_timeout(command_str: &str, explicit_timeout: Option<u64>) -> Duration {
    // 1. Dynamic base timeout from environment, runtime config.toml / CLI (-t), or default
    let env_timeout = std::env::var("MINICODE_CMD_TIMEOUT")
        .or_else(|_| std::env::var("MINICODE_TIMEOUT"))
        .ok()
        .and_then(|v| v.parse::<u64>().ok());

    let base_timeout_secs = env_timeout.unwrap_or_else(get_configured_exec_timeout);

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
pub fn is_daemon_or_server_command(workspace_root: &Path, cmd: &str) -> bool {
    let trimmed = cmd.trim();
    let lower = trimmed.to_lowercase();

    // Explicitly exclude process management, HTTP probes, inspection, or terminating commands
    if lower.starts_with("kill ")
        || lower.starts_with("pkill ")
        || lower.starts_with("killall ")
        || lower.starts_with("pgrep ")
        || lower.starts_with("ps ")
        || lower.starts_with("grep ")
        || lower.starts_with("echo ")
        || lower.starts_with("which ")
        || lower.starts_with("cat ")
        || lower.starts_with("curl ")
        || lower.starts_with("wget ")
        || lower.starts_with("timeout ")
        || lower.starts_with("sleep ")
        || lower.starts_with("ss ")
        || lower.starts_with("lsof ")
        || lower.starts_with("netstat ")
        || lower.starts_with("git ")
        || lower.starts_with("gh ")
    {
        return false;
    }

    // Commands that pipe to output filters (tail, head, grep, wc) expect immediate synchronous stdout
    if lower.contains(" | tail")
        || lower.contains(" | head")
        || lower.contains(" | grep")
        || lower.contains(" | cat")
        || lower.contains(" | wc")
    {
        return false;
    }

    // Build, check, test, lint, and version check commands are synchronous batch operations, never daemons
    if crate::utils::has_word(&lower, "build")
        || crate::utils::has_word(&lower, "test")
        || crate::utils::has_word(&lower, "check")
        || crate::utils::has_word(&lower, "lint")
        || lower.contains("--version")
        || crate::utils::has_word(&lower, "-v")
    {
        return false;
    }

    crate::dev::scripts::WorkspaceScriptDiscovery::is_server_command(workspace_root, cmd)
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
        working_dir: None,
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
            "Background task launched successfully and registered with MiniTask Manager:\n• Task ID: {}\n• Name: {}\n• Type: {}\n• PID: {}\n• Status: {}\n",
            updated.id.as_str(),
            updated.name,
            updated.process_type,
            updated.pid.map(|p| p.to_string()).unwrap_or_else(|| "none".to_string()),
            updated.status
        ));

        if let Some(ref url) = updated.url {
            output.push_str(&format!("• Primary URL: {}\n", url));
        } else if !updated.ports.is_empty() {
            let p = updated.ports[0];
            let host = crate::dev::ports::resolve_listening_loopback_host(p).unwrap_or("localhost");
            output.push_str(&format!(
                "• Active Ports: {:?}\n• URL: http://{}:{}\n",
                updated.ports, host, p
            ));
        } else if let Some(hint) = port_hint {
            let host =
                crate::dev::ports::resolve_listening_loopback_host(hint).unwrap_or("localhost");
            output.push_str(&format!(
                "• Expected Port: {}\n• URL: http://{}:{}\n",
                hint, host, hint
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

/// Table-driven classifier for build, compile, test, bundle, package install, and runner workloads
/// across all major languages and stacks (JS/TS/Bun/Node/Deno, Rust, Python, Go, C/C++/Zig, JVM, .NET, Ruby, PHP, Docker).
/// Returns a short descriptive task slug when the command should be supervised in `minitask` (`MiniDevRegistry`).
pub fn classify_background_task_name(cmd: &str) -> Option<&'static str> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();

    // Exclude fast metadata/help probes and simple pipes to head/tail/grep/wc
    if lower.contains("--version")
        || lower.contains(" --help")
        || crate::utils::has_word(&lower, "-v")
        || crate::utils::has_word(&lower, "-h")
        || lower.contains(" | head")
        || lower.contains(" | tail")
        || lower.contains(" | wc")
    {
        return None;
    }

    const PREFIX_RULES: &[(&str, &str)] = &[
        ("bun build", "bun-build"),
        ("bun test", "bun-test"),
        ("bun install", "bun-install"),
        ("bun i", "bun-install"),
        ("bun add", "bun-install"),
        ("bun run", "bun-run"),
        ("bun x ", "bunx-task"),
        ("bunx ", "bunx-task"),
        ("npm run build", "npm-build"),
        ("npm test", "npm-test"),
        ("npm run test", "npm-test"),
        ("npm install", "npm-install"),
        ("npm i", "npm-install"),
        ("npm ci", "npm-ci"),
        ("npm run ", "npm-run"),
        ("npx ", "npx-task"),
        ("pnpm build", "pnpm-build"),
        ("pnpm test", "pnpm-test"),
        ("pnpm install", "pnpm-install"),
        ("pnpm i", "pnpm-install"),
        ("pnpm run ", "pnpm-run"),
        ("yarn build", "yarn-build"),
        ("yarn test", "yarn-test"),
        ("yarn install", "yarn-install"),
        ("yarn ", "yarn-task"),
        ("deno test", "deno-test"),
        ("deno compile", "deno-compile"),
        ("deno task", "deno-task"),
        ("deno run", "deno-run"),
        ("tsc", "tsc-check"),
        ("vite build", "vite-build"),
        ("next build", "next-build"),
        ("vitest", "vitest"),
        ("jest", "jest"),
        ("playwright test", "playwright-test"),
        ("cargo build", "cargo-build"),
        ("cargo check", "cargo-check"),
        ("cargo test", "cargo-test"),
        ("cargo clippy", "cargo-clippy"),
        ("cargo run", "cargo-run"),
        ("cargo bench", "cargo-bench"),
        ("cargo install", "cargo-install"),
        ("rustc ", "rustc-compile"),
        ("pytest", "pytest"),
        ("python -m pytest", "pytest"),
        ("python3 -m pytest", "pytest"),
        ("python -m unittest", "py-unittest"),
        ("python3 -m unittest", "py-unittest"),
        ("uv run", "uv-run"),
        ("uv sync", "uv-sync"),
        ("uv pip install", "uv-pip-install"),
        ("pip install", "pip-install"),
        ("pip3 install", "pip-install"),
        ("poetry install", "poetry-install"),
        ("poetry run", "poetry-run"),
        ("mypy", "mypy-check"),
        ("pyright", "pyright-check"),
        ("ruff check", "ruff-check"),
        ("go build", "go-build"),
        ("go test", "go-test"),
        ("go run", "go-run"),
        ("golangci-lint", "golangci-lint"),
        ("make", "make-build"),
        ("cmake", "cmake-build"),
        ("ninja", "ninja-build"),
        ("gcc ", "gcc-compile"),
        ("g++ ", "gpp-compile"),
        ("clang ", "clang-compile"),
        ("clang++ ", "clangpp-compile"),
        ("zig build", "zig-build"),
        ("zig test", "zig-test"),
        ("gradle ", "gradle-task"),
        ("./gradlew", "gradle-task"),
        ("mvn ", "maven-task"),
        ("./mvnw", "maven-task"),
        ("dotnet build", "dotnet-build"),
        ("dotnet test", "dotnet-test"),
        ("dotnet run", "dotnet-run"),
        ("bundle exec", "bundle-exec"),
        ("rspec", "rspec-test"),
        ("composer install", "composer-install"),
        ("phpunit", "phpunit-test"),
        ("mix compile", "mix-compile"),
        ("mix test", "mix-test"),
        ("swift build", "swift-build"),
        ("swift test", "swift-test"),
        ("flutter build", "flutter-build"),
        ("flutter test", "flutter-test"),
        ("dart compile", "dart-compile"),
        ("dart test", "dart-test"),
        ("docker build", "docker-build"),
        ("docker compose", "docker-compose"),
    ];

    for segment in lower.split(&[';', '&'][..]) {
        let seg = segment.trim();
        // Strip leading env assignments like `CI=1 CARGO_BUILD_JOBS=1 cargo test`
        let effective = seg
            .split_whitespace()
            .skip_while(|tok| tok.contains('=') && !tok.starts_with('-'))
            .collect::<Vec<_>>()
            .join(" ");
        for (prefix, label) in PREFIX_RULES {
            if effective == prefix.trim() || effective.starts_with(prefix) {
                return Some(*label);
            }
        }
    }

    None
}

/// Executes a non-daemon command inside `MiniDevRegistry` (`minitask`) with Antigravity-style
/// `wait_ms_before_async` auto-backgrounding.
///
/// - If the command finishes within `wait_ms`, marks it as reported in `MiniDevRegistry` and returns
///   the full RTK-compacted + fault-localized output synchronously.
/// - If the command is still running after `wait_ms`, leaves it running in `MiniDevRegistry` (`minitask`)
///   and returns a structured background task handoff with `task_id`, live log file path, and preview logs.
pub async fn exec_managed_background_task(
    workspace_root: &Path,
    command_str: &str,
    wait_ms: u64,
    explicit_context: Option<usize>,
) -> Result<String> {
    let registry = crate::dev::registry::get_global_dev_registry();
    let clean_cmd = command_str.trim();
    let name_slug = classify_background_task_name(clean_cmd).unwrap_or("bg-cmd");

    let req = crate::dev::models::SpawnDevRequest {
        command: clean_cmd.to_string(),
        name: Some(name_slug.to_string()),
        process_type: crate::dev::models::DevProcessType::Script,
        working_dir: None,
        extra_env: std::collections::HashMap::new(),
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: Some(crate::dev::models::RestartPolicy::Never),
    };

    let (summary, handle) = registry
        .spawn_with_options(workspace_root, req, false)
        .await?;

    let clamped_wait_ms = wait_ms.clamp(50, 120_000);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(clamped_wait_ms);
    loop {
        let notified = handle.completion_notify.notified();
        if !handle.status.read().await.is_alive() {
            break;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        if tokio::time::timeout(deadline - now, notified)
            .await
            .is_err()
        {
            break;
        }
    }

    let final_status = handle.status.read().await.clone();
    if !final_status.is_alive() {
        // Command finished within `wait_ms`! Mark as already reported so AgentLoop does not duplicate it.
        registry.mark_task_reported(&summary.id);

        let lines: Vec<String> = {
            let lock = handle.logs.read().await;
            lock.iter().cloned().collect()
        };
        let mut combined = String::new();
        let mut stderr_only = String::new();
        for line in lines {
            if let Some(rest) = line.strip_prefix("[STDERR] ") {
                if !stderr_only.is_empty() {
                    stderr_only.push('\n');
                }
                stderr_only.push_str(rest);
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str("[stderr]: ");
                combined.push_str(rest);
            } else {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&line);
            }
        }

        let exit_code = match final_status {
            crate::dev::models::DevProcessStatus::Exited(code) => code,
            crate::dev::models::DevProcessStatus::Stopped => Some(0),
            crate::dev::models::DevProcessStatus::Degraded(_) => Some(1),
            _ => Some(0),
        };

        return finalize_command_output(
            workspace_root,
            clean_cmd,
            combined,
            &stderr_only,
            exit_code,
            explicit_context,
        );
    }

    // Still running after `wait_ms` -> return Antigravity-style async background task handoff!
    let preview_lines = handle.get_logs(20, None).await;
    let log_path_display = handle
        .log_path
        .as_ref()
        .map(|p| {
            p.strip_prefix(workspace_root)
                .unwrap_or(p.as_path())
                .display()
                .to_string()
        })
        .unwrap_or_else(|| format!(".minicode/logs/tasks/{}.log", summary.id.as_str()));

    let mut out = format!(
        "⏳ Command still running after {}ms — running asynchronously in `minitask` (`MiniDevRegistry`).\n\
         • Task ID: `{}`\n\
         • Name: `{}`\n\
         • PID: `{}`\n\
         • Status: `Running`\n\
         • Command: `{}`\n\
         • Live Log File: `{}`\n",
        clamped_wait_ms,
        summary.id.as_str(),
        summary.name,
        handle.pid(),
        clean_cmd,
        log_path_display
    );

    if !preview_lines.is_empty() {
        out.push_str("• Initial Output:\n```\n");
        out.push_str(&preview_lines.join("\n"));
        out.push_str("\n```\n");
    }

    out.push_str(&format!(
        "\n🔔 **Reactive Notification:** The system will automatically notify you with `<completed_background_tasks>` as soon as `{id}` finishes — do NOT poll in a loop.\n\
         🛠️ **Task Controls:** You may continue parallel work now, or call `minitask(action=\"await\", id=\"{id}\")`, `minitask(action=\"logs\", id=\"{id}\")`, `minitask(action=\"send_input\", id=\"{id}\", input=\"...\")`, or `minitask(action=\"kill\", id=\"{id}\")`.",
        id = summary.id.as_str()
    ));

    Ok(out)
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

/// Executes a shell command with Antigravity-style `wait_ms_before_async` and `run_in_background` support.
pub async fn exec_cmd_with_async_options(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
    explicit_context: Option<usize>,
    is_daemon: bool,
    wait_ms_before_async: Option<u64>,
    run_in_background: Option<bool>,
) -> Result<String> {
    if is_daemon || is_daemon_or_server_command(workspace_root, command_str) {
        return exec_daemon_or_server(workspace_root, command_str).await;
    }

    if run_in_background == Some(true) {
        let wait_ms = wait_ms_before_async.unwrap_or(500);
        return exec_managed_background_task(
            workspace_root,
            command_str,
            wait_ms,
            explicit_context,
        )
        .await;
    }

    if let Some(wait_ms) = wait_ms_before_async {
        return exec_managed_background_task(
            workspace_root,
            command_str,
            wait_ms,
            explicit_context,
        )
        .await;
    }

    if classify_background_task_name(command_str).is_some() {
        let default_wait_ms = timeout_secs
            .map(|s| (s.saturating_mul(1000)).clamp(500, 15_000))
            .unwrap_or(10_000);
        return exec_managed_background_task(
            workspace_root,
            command_str,
            default_wait_ms,
            explicit_context,
        )
        .await;
    }

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
    if is_daemon || is_daemon_or_server_command(workspace_root, command_str) {
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
    tokio_cmd.stdin(std::process::Stdio::null());
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
            if child_pid > 0 {
                crate::dev::registry::get_global_dev_registry().unregister_external_pid(child_pid);
            }
            s
        }
        Ok(Err(e)) => {
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

    let exit_code = status.code();
    finalize_command_output(
        workspace_root,
        command_str,
        combined,
        &stderr_str,
        exit_code,
        explicit_context,
    )
}

fn finalize_command_output(
    workspace_root: &Path,
    command_str: &str,
    mut combined: String,
    stderr_str: &str,
    exit_code: Option<i32>,
    explicit_context: Option<usize>,
) -> Result<String> {
    if combined.len() > crate::constants::EXEC_MAX_OUTPUT_BYTES {
        let valid_end = combined.floor_char_boundary(crate::constants::EXEC_MAX_OUTPUT_BYTES);
        let truncated = &combined[..valid_end];
        combined = format!(
            "{}\n\n[... Output truncated: exceeded max limit ...]",
            truncated
        );
    }

    let status_code = exit_code.unwrap_or(crate::constants::SIGNAL_KILLED_EXIT_CODE);

    // Preserve full execution output on disk if output exceeds threshold (Phase 115, Phase 117)
    let context_window = explicit_context.unwrap_or_else(|| resolve_context_window(workspace_root));
    let line_threshold = if context_window >= crate::constants::CONTEXT_WINDOW_128K {
        crate::constants::DONUT_EXTENDED_THRESHOLD_LINES
    } else {
        crate::constants::DONUT_STANDARD_THRESHOLD_LINES
    };

    let total_lines = combined.lines().count();
    let total_bytes = combined.len();
    let is_truncated =
        total_lines > line_threshold || total_bytes > crate::constants::EXEC_MAX_OUTPUT_BYTES;

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

    let compacted_lines = compacted.lines().count();
    let significantly_compacted = (total_lines > 40 || total_bytes > 4096)
        && (compacted_lines * 100 < total_lines.max(1) * 65
            || compacted.len() * 100 < total_bytes.max(1) * 65);

    if is_truncated || significantly_compacted {
        let logs_dir = workspace_root.join(".minicode").join("logs");
        if let Err(e) = std::fs::create_dir_all(&logs_dir) {
            tracing::debug!("Could not create .minicode/logs: {}", e);
        }
        let log_file = logs_dir.join("last_exec.log");
        let redacted_raw = crate::sandbox::redact::SecretRedactor::global().redact(&combined);
        if let Err(e) = std::fs::write(&log_file, &redacted_raw) {
            tracing::debug!("Could not write last_exec.log: {}", e);
        }
        let spillover_path =
            super::middleware::save_spillover_output(workspace_root, "exec_cmd", &combined)
                .unwrap_or_else(|| ".minicode/logs/last_exec.log".to_string());
        let log_notice = format!(
            "\n[Full untruncated output ({} lines, {}B) saved to {} (and .minicode/logs/last_exec.log) — use read_file or grep_search on this path if you need omitted lines]",
            total_lines, total_bytes, spillover_path
        );
        if !compacted.contains("last_exec.log") && !compacted.contains("tmp/tool_outputs/") {
            compacted.push_str(&log_notice);
        }
    }
    compacted = crate::security::sanitize_text(&compacted);

    if status_code != 0 {
        let trailing_cmd = command_str
            .rsplit([';', '&', '|'])
            .next()
            .unwrap_or(command_str)
            .trim();
        let trailing_prog = trailing_cmd.split_whitespace().next().unwrap_or("");
        if status_code == 1
            && stderr_str.trim().is_empty()
            && matches!(trailing_prog, "grep" | "egrep" | "fgrep" | "rg")
        {
            return if compacted.trim().is_empty() {
                Ok("(0 matches found)".to_string())
            } else {
                Ok(format!("{}\n(0 matches found)", compacted.trim_end()))
            };
        }

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

    #[tokio::test]
    async fn test_exec_grep_zero_matches_succeeds_cleanly() {
        let temp_dir = std::env::temp_dir();
        let out = exec_cmd(
            &temp_dir,
            "echo 'checked'; printf 'alpha\\nbeta\\n' | grep 'nonexistent_token_xyz'",
            Some(5),
        )
        .await
        .expect("grep with 0 matches and empty stderr should succeed");
        assert!(out.contains("checked"));
        assert!(out.contains("(0 matches found)"));
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

    #[test]
    fn test_classify_background_task_name() {
        assert_eq!(
            classify_background_task_name("bun build src/index.ts --outdir dist"),
            Some("bun-build")
        );
        assert_eq!(
            classify_background_task_name("CARGO_BUILD_JOBS=1 cargo test --lib"),
            Some("cargo-test")
        );
        assert_eq!(
            classify_background_task_name("pytest -q tests/"),
            Some("pytest")
        );
        assert_eq!(
            classify_background_task_name("go build ./..."),
            Some("go-build")
        );
        assert_eq!(
            classify_background_task_name("tsc --noEmit"),
            Some("tsc-check")
        );
        assert_eq!(classify_background_task_name("cargo --version"), None);
        assert_eq!(classify_background_task_name("ls -la"), None);
    }

    #[tokio::test]
    async fn test_exec_cmd_wait_ms_before_async_promotion_and_completion() {
        let temp = tempfile::tempdir().expect("tempdir");

        // 1. Fast command finishes within wait_ms_before_async (2000ms) -> returns output directly
        let fast_out = exec_cmd_with_async_options(
            temp.path(),
            "echo 'fast-build-ok'",
            Some(5),
            None,
            false,
            Some(2000),
            None,
        )
        .await
        .expect("fast command succeeds");
        assert!(fast_out.contains("fast-build-ok"));

        // 2. Slow command exceeds wait_ms_before_async (150ms) -> promotes to async task in minitask
        let async_out = exec_cmd_with_async_options(
            temp.path(),
            "echo 'starting-bundle'; sleep 1; echo 'bundle-complete'",
            Some(10),
            None,
            false,
            Some(150),
            None,
        )
        .await
        .expect("async handoff succeeds");
        assert!(
            async_out.contains("running asynchronously in `minitask`"),
            "Expected minitask async handoff, got:\n{}",
            async_out
        );
        assert!(async_out.contains(".minicode/logs/tasks/"));

        // Wait for background task to finish and verify reactive harvest
        let registry = crate::dev::registry::get_global_dev_registry();
        registry
            .wait_until_active_tasks_settle(Duration::from_secs(4))
            .await;
        let (_ids, block) = registry
            .harvest_newly_completed_tasks_block(temp.path())
            .await
            .expect("expected harvested completed task");
        assert!(block.contains("<completed_background_tasks>"));
        assert!(block.contains("bundle-complete"));
    }
}
