use super::accessibility::AccessibilityManager;
use super::driver::CdpClient;
use super::engine::{BrowserEngine, BrowserMode, EngineConfig, GUI_PRIORITY, HEADLESS_PRIORITY};
use crate::constants::{BROWSER_CDP_BASE_PORT, BROWSER_STARTUP_TIMEOUT_MS};
use crate::error::{Result, ToolError};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};

#[derive(Debug, Clone, Default)]
pub struct SpaceScopeConfig {
    pub space: Option<String>,
    pub proxy: Option<String>,
}

tokio::task_local! {
    static ACTIVE_SPACE_SCOPE: SpaceScopeConfig;
}

/// Runs `fut` inside a task-local isolated `BrowserSpace` scope (`ego-lite` concurrent co-browsing).
/// Safe across `tokio::spawn` parallel subagents because `task_local!` is bound per async task, not OS thread.
pub async fn with_browser_space<F, T>(space: Option<String>, proxy: Option<String>, fut: F) -> T
where
    F: Future<Output = T>,
{
    let scope = SpaceScopeConfig {
        space: space
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        proxy: proxy
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    };
    if scope.space.is_none() && scope.proxy.is_none() {
        fut.await
    } else {
        ACTIVE_SPACE_SCOPE.scope(scope, fut).await
    }
}

/// Reads the current task-local `SpaceScopeConfig`, if set.
pub fn current_space_scope() -> SpaceScopeConfig {
    ACTIVE_SPACE_SCOPE
        .try_with(Clone::clone)
        .unwrap_or_default()
}

async fn kill_engine_process_tree(pid: u32) {
    #[cfg(unix)]
    {
        let p_i32 = pid as i32;
        let descendants = crate::dev::ports::find_all_descendants(pid);
        unsafe {
            let _ = libc::kill(-p_i32, libc::SIGTERM);
            let _ = libc::kill(p_i32, libc::SIGTERM);
            for &desc in &descendants {
                let _ = libc::kill(desc as i32, libc::SIGTERM);
                let _ = libc::kill(-(desc as i32), libc::SIGTERM);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        unsafe {
            let _ = libc::kill(-p_i32, libc::SIGKILL);
            let _ = libc::kill(p_i32, libc::SIGKILL);
            for &desc in &descendants {
                let _ = libc::kill(desc as i32, libc::SIGKILL);
                let _ = libc::kill(-(desc as i32), libc::SIGKILL);
            }
        }
    }
    crate::dev::registry::get_global_dev_registry().unregister_external_pid(pid);
}

/// Running browser process with assigned CDP port and metadata
pub struct EngineProcess {
    #[allow(dead_code)]
    pub config: EngineConfig,
    pub child: Child,
    #[allow(dead_code)]
    pub cdp_port: u16,
    pub cdp_http_url: String,
}

impl EngineProcess {
    /// Kill process and all descendants cleanly
    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(pid) = self.child.id() {
            kill_engine_process_tree(pid).await;
        }
        let _ = self.child.kill().await;
        Ok(())
    }
}

/// Ownership state for an isolated `BrowserSpace` (`ego-lite` + HITL handoff model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceOwner {
    Agent = 0,
    User = 1,
}

/// Isolated background browsing workspace (`ego-lite` "Space") with its own `BrowserContext`,
/// target `sessionId`, independent ARIA revision table, optional proxy, and ownership state.
pub struct BrowserSpace {
    pub name: String,
    pub context_id: Option<String>,
    pub target_id: String,
    pub cdp: Arc<CdpClient>,
    pub accessibility: Arc<tokio::sync::Mutex<AccessibilityManager>>,
    pub proxy_server: Option<String>,
    owner: std::sync::atomic::AtomicU8,
    pub handoff_reason: tokio::sync::Mutex<Option<String>>,
    pub created_at: Instant,
}

impl BrowserSpace {
    pub fn owner(&self) -> SpaceOwner {
        if self.owner.load(std::sync::atomic::Ordering::Relaxed) == 1 {
            SpaceOwner::User
        } else {
            SpaceOwner::Agent
        }
    }

    pub async fn set_owner(&self, owner: SpaceOwner, reason: Option<String>) {
        self.owner
            .store(owner as u8, std::sync::atomic::Ordering::Relaxed);
        *self.handoff_reason.lock().await = reason;
    }
}

/// Persistent engine handle shared across tool calls so page state,
/// ARIA references, isolated Spaces, and console history survive between invocations.
pub struct SharedEngine {
    pub process: EngineProcess,
    pub cdp: Arc<CdpClient>,
    pub accessibility: Arc<tokio::sync::Mutex<AccessibilityManager>>,
    pub spaces: tokio::sync::Mutex<HashMap<String, Arc<BrowserSpace>>>,
    pub started_at: Instant,
    pub aura_enabled: std::sync::atomic::AtomicBool,
    pub gui_promoted: std::sync::atomic::AtomicBool,
    pub default_owner: std::sync::atomic::AtomicU8,
}

impl SharedEngine {
    /// Returns whether the visual simulator aura should be shown on pages in this browser session.
    pub fn is_aura_enabled(&self) -> bool {
        self.aura_enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Updates the session-persistent visual simulator aura preference (`browser_aura`).
    pub fn set_aura_enabled(&self, enabled: bool) {
        self.aura_enabled
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    /// Returns the effective `BrowserMode`, accounting for live zero-restart headless -> GUI promotions.
    pub fn effective_mode(&self) -> BrowserMode {
        if self.process.config.mode == BrowserMode::Gui
            || self.gui_promoted.load(std::sync::atomic::Ordering::Relaxed)
        {
            BrowserMode::Gui
        } else {
            BrowserMode::Headless
        }
    }

    pub fn set_gui_promoted(&self, gui: bool) {
        self.gui_promoted
            .store(gui, std::sync::atomic::Ordering::Relaxed);
    }

    /// Resolves `(cdp, accessibility, optional_space)` from the current task-local `SpaceScopeConfig`
    /// (falling back to the primary tab in O(1) when no `space` was requested).
    pub async fn active_session(
        &self,
    ) -> Result<(
        Arc<CdpClient>,
        Arc<tokio::sync::Mutex<AccessibilityManager>>,
        Option<Arc<BrowserSpace>>,
    )> {
        let scope = current_space_scope();
        self.resolve_space(scope.space.as_deref(), scope.proxy.as_deref())
            .await
    }

    /// Resolves `(cdp, accessibility, optional_space)` for `space_name` (`None` or `"default"` uses primary tab;
    /// any other name lazily spawns or reuses an isolated background `BrowserSpace`).
    pub async fn resolve_space(
        &self,
        space_name: Option<&str>,
        proxy_server: Option<&str>,
    ) -> Result<(
        Arc<CdpClient>,
        Arc<tokio::sync::Mutex<AccessibilityManager>>,
        Option<Arc<BrowserSpace>>,
    )> {
        let trimmed = space_name.map(str::trim).filter(|s| {
            !s.is_empty() && !s.eq_ignore_ascii_case("default") && !s.eq_ignore_ascii_case("main")
        });
        let Some(raw_name) = trimmed else {
            return Ok((Arc::clone(&self.cdp), Arc::clone(&self.accessibility), None));
        };
        let safe_name: String = raw_name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(48)
            .collect();
        if safe_name.is_empty() {
            return Ok((Arc::clone(&self.cdp), Arc::clone(&self.accessibility), None));
        }

        let mut map = self.spaces.lock().await;
        if let Some(existing) = map.get(&safe_name) {
            return Ok((
                Arc::clone(&existing.cdp),
                Arc::clone(&existing.accessibility),
                Some(Arc::clone(existing)),
            ));
        }

        let (space_cdp, context_id, target_id) =
            self.cdp.attach_isolated_space(proxy_server).await?;
        let space = Arc::new(BrowserSpace {
            name: safe_name.clone(),
            context_id,
            target_id,
            cdp: Arc::new(space_cdp),
            accessibility: Arc::new(tokio::sync::Mutex::new(AccessibilityManager::new())),
            proxy_server: proxy_server.map(str::to_string),
            owner: std::sync::atomic::AtomicU8::new(SpaceOwner::Agent as u8),
            handoff_reason: tokio::sync::Mutex::new(None),
            created_at: Instant::now(),
        });
        map.insert(safe_name, Arc::clone(&space));
        Ok((
            Arc::clone(&space.cdp),
            Arc::clone(&space.accessibility),
            Some(space),
        ))
    }

    /// Lists all active isolated Spaces and their ownership / context metadata.
    pub async fn list_spaces_summary(&self) -> Vec<serde_json::Value> {
        let map = self.spaces.lock().await;
        let mut out = Vec::with_capacity(map.len() + 1);
        let default_owner = if self
            .default_owner
            .load(std::sync::atomic::Ordering::Relaxed)
            == 1
        {
            "user"
        } else {
            "agent"
        };
        out.push(serde_json::json!({
            "space": "default",
            "owner": default_owner,
            "mode": format!("{}", self.effective_mode()),
            "isolated_context": false,
        }));
        for sp in map.values() {
            let reason = sp.handoff_reason.lock().await.clone();
            out.push(serde_json::json!({
                "space": sp.name,
                "context_id": sp.context_id,
                "target_id": sp.target_id,
                "owner": if sp.owner() == SpaceOwner::User { "user" } else { "agent" },
                "proxy": sp.proxy_server,
                "handoff_reason": reason,
                "uptime_secs": sp.created_at.elapsed().as_secs(),
                "isolated_context": true,
            }));
        }
        out
    }

    /// Disposes and removes a named isolated Space without disturbing other Spaces or the user's primary tab.
    pub async fn close_space(&self, space_name: &str) -> bool {
        let mut map = self.spaces.lock().await;
        if let Some(sp) = map.remove(space_name.trim()) {
            self.cdp
                .dispose_isolated_space(sp.context_id.as_deref(), &sp.target_id)
                .await;
            return true;
        }
        false
    }
}

static LIVE_ENGINE: tokio::sync::Mutex<Option<Arc<SharedEngine>>> =
    tokio::sync::Mutex::const_new(None);

impl BrowserManager {
    /// Returns the pooled live engine, launching or relaunching as needed.
    ///
    /// The engine stays alive across tool calls; the child process is killed
    /// automatically (`kill_on_drop`) when the host process exits.
    pub async fn get_or_launch(
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<Arc<SharedEngine>> {
        let mut guard = LIVE_ENGINE.lock().await;
        if let Some(existing) = guard.as_ref() {
            let needs_gui_upgrade =
                mode == BrowserMode::Gui && existing.effective_mode() == BrowserMode::Headless;
            if needs_gui_upgrade {
                // Zero-restart live GTK window promotion when running MiniBrowse:
                // preserves 100% of cookies, OAuth/SSO state, DOM inputs, and background Spaces!
                if existing.process.config.engine == BrowserEngine::MiniBrowse {
                    if let Ok(res) = existing.cdp.set_window_mode(true, false, None).await {
                        if res.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                            tracing::info!(
                                "Promoted live MiniBrowse headless session to GUI window in-place (zero state loss)"
                            );
                            existing.set_gui_promoted(true);
                            existing.set_aura_enabled(true);
                            return Ok(Arc::clone(existing));
                        }
                    }
                }
                tracing::info!("Upgrading pooled headless browser engine to GUI mode via restart");
                if let Some(old) = guard.take() {
                    if let Some(pid) = old.process.child.id() {
                        kill_engine_process_tree(pid).await;
                    }
                }
            } else {
                let alive = existing
                    .cdp
                    .send_command(
                        "Runtime.evaluate",
                        serde_json::json!({ "expression": "1 + 1", "returnByValue": true }),
                    )
                    .await
                    .is_ok();
                if alive {
                    return Ok(Arc::clone(existing));
                }
                tracing::warn!("Pooled browser engine died; relaunching");
                if let Some(dead) = guard.take() {
                    if let Some(pid) = dead.process.child.id() {
                        kill_engine_process_tree(pid).await;
                    }
                }
            }
        }

        let config = Self::discover_engine(mode, workspace_root).ok_or_else(|| {
            ToolError::CommandExec(
                "No browser engine (MiniBrowse, Chrome, Obscura, Firefox) found on PATH"
                    .to_string(),
            )
        })?;
        let mut proc = Self::spawn_engine(&config).await?;
        let cdp = match CdpClient::connect(&proc.cdp_http_url).await {
            Ok(cdp) => Arc::new(cdp),
            Err(e) => {
                let _ = proc.shutdown().await;
                return Err(e);
            }
        };
        tracing::info!(engine = %config.engine, "Browser engine ready (persistent session)");
        let is_gui = config.mode == BrowserMode::Gui;
        let handle = Arc::new(SharedEngine {
            aura_enabled: std::sync::atomic::AtomicBool::new(is_gui),
            gui_promoted: std::sync::atomic::AtomicBool::new(is_gui),
            default_owner: std::sync::atomic::AtomicU8::new(SpaceOwner::Agent as u8),
            process: proc,
            cdp,
            accessibility: Arc::new(tokio::sync::Mutex::new(AccessibilityManager::new())),
            spaces: tokio::sync::Mutex::new(HashMap::new()),
            started_at: Instant::now(),
        });
        *guard = Some(Arc::clone(&handle));
        Ok(handle)
    }

    /// Shuts down the pooled live engine, killing the child process, its process group,
    /// and all child/descendant processes cleanly.
    /// Preserves GUI browser instances unless `force: true` is passed.
    pub async fn shutdown_live_engine() -> Result<bool> {
        Self::shutdown_live_engine_opt(false).await
    }

    /// Shuts down the live engine with optional forced override for GUI instances.
    pub async fn shutdown_live_engine_opt(force: bool) -> Result<bool> {
        let mut guard = LIVE_ENGINE.lock().await;
        if let Some(ref engine) = *guard {
            if engine.effective_mode() == BrowserMode::Gui && !force {
                tracing::info!("Live browser engine is running in GUI mode; preserving window for user viewing.");
                return Ok(false);
            }
        }

        if let Some(engine) = guard.take() {
            crate::tools::browser::BrowserController::reset_screenshot_counter();
            if let Some(p) = engine.process.child.id() {
                kill_engine_process_tree(p).await;
            }
            tracing::info!("Live browser engine shut down cleanly.");
            return Ok(true);
        }
        Ok(false)
    }

    /// Returns the execution mode (`Headless` or `Gui`) of the currently running live engine, if any.
    #[allow(dead_code)]
    pub async fn live_engine_mode() -> Option<BrowserMode> {
        let guard = LIVE_ENGINE.lock().await;
        guard.as_ref().map(|e| e.effective_mode())
    }

    /// Checks if a live browser engine is currently running and alive.
    pub async fn is_live_engine_running() -> bool {
        Self::get_live_engine_details().await.is_some()
    }

    /// Checks synchronously if a live browser engine is currently running and alive.
    pub fn is_live_engine_running_sync() -> bool {
        if let Ok(guard) = LIVE_ENGINE.try_lock() {
            if let Some(ref engine) = *guard {
                let pid = engine.process.child.id().unwrap_or(0);
                pid > 0 && crate::dev::ports::is_process_running(pid)
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Gets summary of current live engine if running: (engine_name, pid, cdp_port).
    pub async fn get_live_engine_info() -> Option<(String, u32, u16)> {
        Self::get_live_engine_details()
            .await
            .map(|(name, pid, port, _)| (name, pid, port))
    }

    /// Gets detailed live engine telemetry: (engine_name, pid, cdp_port, uptime_secs).
    pub async fn get_live_engine_details() -> Option<(String, u32, u16, u64)> {
        let mut guard = LIVE_ENGINE.lock().await;
        if let Some(ref engine) = *guard {
            let pid = engine.process.child.id().unwrap_or(0);
            if pid == 0 || !crate::dev::ports::is_process_running(pid) {
                if let Some(dead) = guard.take() {
                    if let Some(p) = dead.process.child.id() {
                        crate::dev::registry::get_global_dev_registry().unregister_external_pid(p);
                    }
                }
                return None;
            }
            let uptime = engine.started_at.elapsed().as_secs();
            Some((
                engine.process.config.engine.to_string(),
                pid,
                engine.process.cdp_port,
                uptime,
            ))
        } else {
            None
        }
    }
}
#[derive(Clone, Default)]
pub struct BrowserManager;

fn parse_engine_name(name: &str) -> Option<BrowserEngine> {
    match name.trim().to_lowercase().as_str() {
        "minibrowse" | "mini-browse" | "mb" => Some(BrowserEngine::MiniBrowse),
        "obscura" => Some(BrowserEngine::Obscura),
        "firefox" | "ff" => Some(BrowserEngine::Firefox),
        "chrome" | "chromium" | "google-chrome" => Some(BrowserEngine::Chrome),
        _ => None,
    }
}

impl BrowserManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

    /// Discovers an available browser binary according to priority rules and configuration
    pub fn discover_engine(mode: BrowserMode, workspace_root: &Path) -> Option<EngineConfig> {
        let default_priority = match mode {
            BrowserMode::Headless => HEADLESS_PRIORITY,
            BrowserMode::Gui => GUI_PRIORITY,
        };

        // 1. MINICODE_BROWSER env var override
        let priority: Vec<BrowserEngine> = if let Ok(name) = std::env::var("MINICODE_BROWSER") {
            if let Some(engine) = parse_engine_name(&name) {
                vec![engine]
            } else {
                default_priority.to_vec()
            }
        } else if let Ok(cfg) = crate::config::Config::load(Some(workspace_root), None) {
            // 2. Explicit default engine configured in settings
            if let Some(ref forced) = cfg.browser.default_engine {
                if let Some(engine) = parse_engine_name(forced) {
                    vec![engine]
                } else {
                    default_priority.to_vec()
                }
            } else {
                // 3. Configured priority list for active mode
                let configured_names = match mode {
                    BrowserMode::Headless => &cfg.browser.headless_priority,
                    BrowserMode::Gui => &cfg.browser.gui_priority,
                };
                let mut parsed: Vec<BrowserEngine> = configured_names
                    .iter()
                    .filter_map(|n| parse_engine_name(n))
                    .collect();
                // Upgrade legacy default ["obscura", "chrome", "firefox"] or ["chrome", "obscura", "firefox"]
                // in saved user configs so MiniBrowse is preferred first, followed by Chrome.
                if parsed.first() == Some(&BrowserEngine::Obscura)
                    && parsed.contains(&BrowserEngine::Chrome)
                {
                    parsed.retain(|&e| e != BrowserEngine::Chrome);
                    parsed.insert(0, BrowserEngine::Chrome);
                }
                if !parsed.is_empty() && !parsed.contains(&BrowserEngine::MiniBrowse) {
                    parsed.insert(0, BrowserEngine::MiniBrowse);
                }
                if parsed.is_empty() {
                    default_priority.to_vec()
                } else {
                    parsed
                }
            }
        } else {
            default_priority.to_vec()
        };

        for &engine in &priority {
            if let Some(binary_path) = find_binary_for_engine(engine) {
                let port = find_available_port(BROWSER_CDP_BASE_PORT);
                let ws_hash = {
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut s = DefaultHasher::new();
                    workspace_root.hash(&mut s);
                    format!("{:08x}", s.finish() as u32)
                };
                let profile_dir = std::env::temp_dir()
                    .join("minicode")
                    .join("browser_profiles")
                    .join(format!(
                        "{}_{}_{}",
                        ws_hash,
                        engine.to_string().to_lowercase(),
                        mode
                    ));

                return Some(EngineConfig::new(
                    engine,
                    mode,
                    port,
                    binary_path,
                    profile_dir,
                ));
            }
        }

        None
    }

    /// Prepares isolated profile directory with engine-specific preferences
    pub fn setup_profile_dir(config: &EngineConfig) -> Result<()> {
        std::fs::create_dir_all(&config.profile_dir).map_err(|e| {
            ToolError::CommandExec(format!(
                "Failed to create browser profile dir '{}': {}",
                config.profile_dir.display(),
                e
            ))
        })?;

        // For Firefox: write user.js enabling remote debugging without prompts
        if config.engine == BrowserEngine::Firefox {
            let user_js_path = config.profile_dir.join("user.js");
            let user_js_content = r#"
user_pref("devtools.debugger.remote-enabled", true);
user_pref("devtools.chrome.enabled", true);
user_pref("devtools.debugger.prompt-connection", false);
user_pref("remote.active-protocols", 3);
"#;
            let _ = std::fs::write(&user_js_path, user_js_content);
        }

        Ok(())
    }

    /// Builds command-line arguments for launching the browser engine
    pub fn build_launch_args(config: &EngineConfig) -> Vec<String> {
        let port_str = config.cdp_port.to_string();
        let profile_str = config.profile_dir.to_string_lossy().to_string();

        let mut args = match config.engine {
            BrowserEngine::MiniBrowse => {
                let mode_flag = match config.mode {
                    BrowserMode::Headless => "--headless",
                    BrowserMode::Gui => "--gui",
                };
                vec![
                    "serve".to_string(),
                    "--port".to_string(),
                    port_str,
                    mode_flag.to_string(),
                    "--width".to_string(),
                    "1440".to_string(),
                    "--height".to_string(),
                    "900".to_string(),
                    "--adblock".to_string(),
                ]
            }
            BrowserEngine::Obscura => {
                // Global flags must precede the `serve` subcommand.
                // --allow-private-network matches minicode's own policy of
                // permitting loopback dev servers in validate_browser_url.
                vec![
                    "--stealth".to_string(),
                    "--allow-private-network".to_string(),
                    "serve".to_string(),
                    "--port".to_string(),
                    port_str,
                ]
            }
            BrowserEngine::Firefox => match config.mode {
                BrowserMode::Headless => {
                    vec![
                        "--headless".to_string(),
                        "--remote-debugging-port".to_string(),
                        port_str,
                        "--profile".to_string(),
                        profile_str,
                        "--no-remote".to_string(),
                    ]
                }
                BrowserMode::Gui => {
                    vec![
                        "--remote-debugging-port".to_string(),
                        port_str,
                        "--profile".to_string(),
                        profile_str,
                        "--no-remote".to_string(),
                    ]
                }
            },
            BrowserEngine::Chrome => match config.mode {
                BrowserMode::Headless => {
                    vec![
                        "--headless=new".to_string(),
                        "--window-size=1440,900".to_string(),
                        format!("--remote-debugging-port={}", port_str),
                        format!("--user-data-dir={}", profile_str),
                        "--disable-gpu".to_string(),
                        "--no-sandbox".to_string(),
                        "--disable-dev-shm-usage".to_string(),
                        "--disable-backgrounding-occluded-windows".to_string(),
                        "--disable-renderer-backgrounding".to_string(),
                        "--disable-background-timer-throttling".to_string(),
                        "--disable-ipc-flooding-protection".to_string(),
                        "--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling"
                            .to_string(),
                        "--no-first-run".to_string(),
                        "--no-default-browser-check".to_string(),
                    ]
                }
                BrowserMode::Gui => {
                    vec![
                        "--window-size=1440,900".to_string(),
                        format!("--remote-debugging-port={}", port_str),
                        format!("--user-data-dir={}", profile_str),
                        "--disable-gpu".to_string(),
                        "--disable-dev-shm-usage".to_string(),
                        "--disable-backgrounding-occluded-windows".to_string(),
                        "--disable-renderer-backgrounding".to_string(),
                        "--disable-background-timer-throttling".to_string(),
                        "--disable-ipc-flooding-protection".to_string(),
                        "--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling"
                            .to_string(),
                        "--no-first-run".to_string(),
                        "--no-default-browser-check".to_string(),
                    ]
                }
            },
        };

        args.extend(config.extra_args.clone());
        args
    }

    /// Spawns the browser engine process and waits until the CDP endpoint is responsive
    pub async fn spawn_engine(config: &EngineConfig) -> Result<EngineProcess> {
        Self::setup_profile_dir(config)?;
        let args = Self::build_launch_args(config);

        tracing::info!(
            engine = %config.engine,
            mode = %config.mode,
            port = config.cdp_port,
            binary = %config.binary_path.display(),
            "Launching browser engine"
        );

        let mut cmd = Command::new(&config.binary_path);
        cmd.args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        // Pass performance budgets for Obscura if applicable
        if config.engine == BrowserEngine::Obscura {
            cmd.env("OBSCURA_SCRIPT_DEADLINE_MS", "15000");
            cmd.env("OBSCURA_MODULE_BUDGET_MS", "5000");
        }

        let child = cmd.spawn().map_err(|e| {
            ToolError::CommandExec(format!(
                "Failed to spawn {} ({}): {}",
                config.engine,
                config.binary_path.display(),
                e
            ))
        })?;

        if let Some(pid) = child.id() {
            crate::dev::registry::get_global_dev_registry().register_external_pid(pid);
        }

        let cdp_http_url = format!("{}{}", crate::constants::CDP_HOST_PREFIX, config.cdp_port);

        // Poll CDP readiness
        let start = Instant::now();
        let timeout = Duration::from_millis(BROWSER_STARTUP_TIMEOUT_MS);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(400))
            .build()
            .unwrap_or_default();

        let mut ready = false;
        let version_url = format!("{}/json/version", cdp_http_url);

        while start.elapsed() < timeout {
            if let Ok(resp) = client.get(&version_url).send().await {
                if resp.status().is_success() {
                    ready = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if !ready {
            tracing::warn!(
                engine = %config.engine,
                port = config.cdp_port,
                "Browser did not respond to /json/version within timeout; proceeding with best effort"
            );
        }

        Ok(EngineProcess {
            config: config.clone(),
            child,
            cdp_port: config.cdp_port,
            cdp_http_url,
        })
    }
}

/// Helper to search local install paths, sibling build artifacts, and system PATH for a BrowserEngine
fn find_binary_for_engine(engine: BrowserEngine) -> Option<PathBuf> {
    if engine == BrowserEngine::MiniBrowse {
        // 1. Sibling binary next to the running `minicode` executable (covers ~/.local/bin, target/release, target/debug)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let sibling = dir.join("minibrowse");
                if sibling.is_file() {
                    return Some(sibling);
                }
                // If running inside `target/debug/deps/<test-bin>`, check parent `target/debug/minibrowse` & `target/release/minibrowse`
                if let Some(target_profile_dir) = dir.parent() {
                    let profile_bin = target_profile_dir.join("minibrowse");
                    if profile_bin.is_file() {
                        return Some(profile_bin);
                    }
                    if let Some(target_root) = target_profile_dir.parent() {
                        for prof in ["release", "debug"] {
                            let cand = target_root.join(prof).join("minibrowse");
                            if cand.is_file() {
                                return Some(cand);
                            }
                        }
                    }
                }
            }
        }
        // 2. Standard user global install directory (~/.local/bin/minibrowse)
        if let Some(home) = dirs::home_dir() {
            let local_bin = home.join(".local").join("bin").join("minibrowse");
            if local_bin.is_file() {
                return Some(local_bin);
            }
        }
        // 3. System PATH lookup
        if let Ok(path) = which::which("minibrowse") {
            return Some(path);
        }
        return None;
    }

    let candidates: &[&str] = match engine {
        BrowserEngine::MiniBrowse => &["minibrowse"],
        BrowserEngine::Obscura => &["obscura"],
        BrowserEngine::Firefox => &[
            "firefox",
            "firefox-esr",
            "firefox-developer-edition",
            "firefox-nightly",
        ],
        BrowserEngine::Chrome => &[
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "brave-browser",
            "brave",
            "msedge",
        ],
    };

    for &name in candidates {
        if let Ok(path) = which::which(name) {
            return Some(path);
        }
    }
    None
}

/// Finds an available TCP port starting from a base port
fn find_available_port(base_port: u16) -> u16 {
    crate::dev::ports::find_next_available_port(base_port, 50).unwrap_or(base_port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_launch_args_minibrowse_headless_and_gui() {
        let headless_cfg = EngineConfig::new(
            BrowserEngine::MiniBrowse,
            BrowserMode::Headless,
            9221,
            PathBuf::from("/usr/local/bin/minibrowse"),
            PathBuf::from("/tmp/mb_profile"),
        );
        let h_args = BrowserManager::build_launch_args(&headless_cfg);
        assert_eq!(
            h_args,
            vec![
                "serve",
                "--port",
                "9221",
                "--headless",
                "--width",
                "1440",
                "--height",
                "900",
                "--adblock",
            ]
        );

        let gui_cfg = EngineConfig::new(
            BrowserEngine::MiniBrowse,
            BrowserMode::Gui,
            9221,
            PathBuf::from("/usr/local/bin/minibrowse"),
            PathBuf::from("/tmp/mb_gui_profile"),
        );
        let g_args = BrowserManager::build_launch_args(&gui_cfg);
        assert!(g_args.contains(&"--gui".to_string()));
        assert!(!g_args.contains(&"--headless".to_string()));
        assert_eq!(
            parse_engine_name("minibrowse"),
            Some(BrowserEngine::MiniBrowse)
        );
    }

    #[test]
    fn test_build_launch_args_obscura() {
        let config = EngineConfig::new(
            BrowserEngine::Obscura,
            BrowserMode::Headless,
            9222,
            PathBuf::from("/usr/bin/obscura"),
            PathBuf::from("/tmp/profile"),
        );
        let args = BrowserManager::build_launch_args(&config);
        assert_eq!(
            args,
            vec![
                "--stealth",
                "--allow-private-network",
                "serve",
                "--port",
                "9222"
            ]
        );
    }

    #[test]
    fn test_build_launch_args_firefox_headless() {
        let config = EngineConfig::new(
            BrowserEngine::Firefox,
            BrowserMode::Headless,
            9223,
            PathBuf::from("/usr/bin/firefox"),
            PathBuf::from("/tmp/ff_profile"),
        );
        let args = BrowserManager::build_launch_args(&config);
        assert!(args.contains(&"--headless".to_string()));
        assert!(args.contains(&"--remote-debugging-port".to_string()));
        assert!(args.contains(&"9223".to_string()));
        assert!(args.contains(&"--no-remote".to_string()));
    }

    #[test]
    fn test_build_launch_args_chrome_headless() {
        let config = EngineConfig::new(
            BrowserEngine::Chrome,
            BrowserMode::Headless,
            9224,
            PathBuf::from("/usr/bin/google-chrome"),
            PathBuf::from("/tmp/cr_profile"),
        );
        let args = BrowserManager::build_launch_args(&config);
        assert!(args.contains(&"--headless=new".to_string()));
        assert!(args.contains(&"--remote-debugging-port=9224".to_string()));
        assert!(args.contains(&"--disable-gpu".to_string()));
        assert!(args.contains(&"--user-data-dir=/tmp/cr_profile".to_string()));
    }

    #[test]
    fn test_build_launch_args_firefox_gui() {
        let config = EngineConfig::new(
            BrowserEngine::Firefox,
            BrowserMode::Gui,
            9225,
            PathBuf::from("/usr/bin/firefox"),
            PathBuf::from("/tmp/ff_gui_profile"),
        );
        let args = BrowserManager::build_launch_args(&config);
        assert!(!args.contains(&"--headless".to_string()));
        assert!(args.contains(&"--remote-debugging-port".to_string()));
    }

    #[test]
    fn test_find_available_port() {
        let port = find_available_port(9222);
        assert!(port >= 9222);
    }

    #[test]
    fn test_minitest59_chrome_gui_launch_args_disable_occlusion() {
        let config = EngineConfig::new(
            BrowserEngine::Chrome,
            BrowserMode::Gui,
            9226,
            PathBuf::from("/usr/bin/google-chrome"),
            PathBuf::from("/tmp/cr_gui_profile"),
        );
        let args = BrowserManager::build_launch_args(&config);
        assert!(!args.contains(&"--headless=new".to_string()));
        assert!(args.contains(&"--disable-backgrounding-occluded-windows".to_string()));
        assert!(args.contains(&"--disable-renderer-backgrounding".to_string()));
        assert!(args.contains(&"--disable-background-timer-throttling".to_string()));
        assert!(args.contains(&"--disable-ipc-flooding-protection".to_string()));
        assert!(args.contains(
            &"--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling".to_string()
        ));
    }
}
