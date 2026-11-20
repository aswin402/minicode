use super::accessibility::AccessibilityManager;
use super::driver::CdpClient;
use super::engine::{
    BrowserEngine, BrowserMode, BrowserPurpose, EngineConfig, DEBUG_PRIORITY, GUI_PRIORITY,
    HEADLESS_PRIORITY,
};
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
    pub engine: Option<BrowserEngine>,
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
    with_browser_scope(space, proxy, None, fut).await
}

/// Runs `fut` inside a task-local `SpaceScopeConfig` carrying optional `space`, `proxy`, and per-call `engine` override.
pub async fn with_browser_scope<F, T>(
    space: Option<String>,
    proxy: Option<String>,
    engine: Option<BrowserEngine>,
    fut: F,
) -> T
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
        engine,
    };
    if scope.space.is_none() && scope.proxy.is_none() && scope.engine.is_none() {
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
        tokio::time::sleep(Duration::from_millis(60)).await;
        let late_descendants = crate::dev::ports::find_all_descendants(pid);
        unsafe {
            let _ = libc::kill(-p_i32, libc::SIGKILL);
            let _ = libc::kill(p_i32, libc::SIGKILL);
            for &desc in descendants.iter().chain(late_descendants.iter()) {
                let _ = libc::kill(desc as i32, libc::SIGKILL);
                let _ = libc::kill(-(desc as i32), libc::SIGKILL);
            }
            // Reap killed browser child immediately so it never lingers as a <defunct> zombie
            let mut status = 0;
            let _ = libc::waitpid(p_i32, &mut status, libc::WNOHANG);
        }
    }
    crate::dev::registry::get_global_dev_registry().unregister_external_pid(pid);
}

/// Running browser process (or attached external `DevToolsActivePort` session) with assigned CDP/WebDriver port and metadata
pub struct EngineProcess {
    #[allow(dead_code)]
    pub config: EngineConfig,
    pub child: Option<Child>,
    pub external_attached: bool,
    #[allow(dead_code)]
    pub cdp_port: u16,
    pub cdp_http_url: String,
}

impl EngineProcess {
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(Child::id)
    }

    /// Kill spawned child process and all descendants cleanly (no-op for attached external browsers).
    pub async fn shutdown(&mut self) -> Result<()> {
        if self.external_attached {
            return Ok(());
        }
        if let Some(ref mut ch) = self.child {
            if let Some(pid) = ch.id() {
                kill_engine_process_tree(pid).await;
            }
            let _ = ch.kill().await;
        }
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
            "engine": self.process.config.engine.to_string(),
            "owner": default_owner,
            "mode": format!("{}", self.effective_mode()),
            "isolated_context": false,
        }));
        for sp in map.values() {
            let reason = sp.handoff_reason.lock().await.clone();
            out.push(serde_json::json!({
                "space": sp.name,
                "engine": self.process.config.engine.to_string(),
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

/// Cooldown window before retrying a browser engine that failed startup/connect in this session.
/// Set to 1 hour (cleared immediately if `browser.default_engine` is updated in settings) so a
/// broken headless engine binary (e.g. Firefox without geckodriver) never stalls mid-session.
const ENGINE_FAILURE_COOLDOWN: Duration = Duration::from_secs(3600);

/// Multi-engine pool maintaining both the active general browser (`MiniBrowse` primary, or
/// user-requested/configured `Chrome` / `Firefox` / `Safari`) and a dedicated `MiniBrowse`
/// debug instance without closing the user's open browser windows.
#[derive(Default)]
struct EnginePool {
    engines: HashMap<BrowserEngine, Arc<SharedEngine>>,
    active_general: Option<BrowserEngine>,
    /// Engines that recently failed to spawn or establish CDP/WebDriver handshakes in this session.
    failed_engines: HashMap<(BrowserEngine, BrowserMode), Instant>,
}

impl EnginePool {
    fn primary_live_engine(&self) -> Option<&Arc<SharedEngine>> {
        if let Some(eng) = self.active_general {
            if let Some(handle) = self.engines.get(&eng) {
                return Some(handle);
            }
        }
        if let Some(mb) = self.engines.get(&BrowserEngine::MiniBrowse) {
            return Some(mb);
        }
        self.engines.values().next()
    }

    fn is_in_failure_cooldown(&self, engine: BrowserEngine, mode: BrowserMode) -> bool {
        self.failed_engines
            .get(&(engine, mode))
            .is_some_and(|ts| ts.elapsed() < ENGINE_FAILURE_COOLDOWN)
    }

    fn mark_engine_failed(&mut self, engine: BrowserEngine, mode: BrowserMode) {
        self.failed_engines.insert((engine, mode), Instant::now());
        if self.active_general == Some(engine) {
            self.active_general = None;
        }
    }

    fn clear_engine_failure(&mut self, engine: BrowserEngine, mode: BrowserMode) {
        self.failed_engines.remove(&(engine, mode));
    }
}

static LIVE_POOL: tokio::sync::Mutex<Option<EnginePool>> = tokio::sync::Mutex::const_new(None);

static LIVE_BROWSER_TELEMETRY: std::sync::Mutex<Option<(usize, String)>> =
    std::sync::Mutex::new(None);

static LIVE_BROWSER_MEM_CACHE: std::sync::Mutex<Option<(Instant, u64)>> =
    std::sync::Mutex::new(None);

impl BrowserManager {
    /// Returns the pooled general browser engine (`MiniBrowse` primary by default, or `Chrome`,
    /// `Firefox`, `Safari`, `Obscura` when requested via `engine="..."` or configured in settings).
    pub async fn get_or_launch(
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<Arc<SharedEngine>> {
        let scope_engine = current_space_scope().engine;
        Self::get_or_launch_for(BrowserPurpose::General, mode, scope_engine, workspace_root).await
    }

    /// Returns the dedicated `MiniBrowse` debug engine for Web, CSS/DOM, QA, Research, and
    /// Flutter/Dart VM diagnostics — keeping any open `Chrome` / `Firefox` / `Safari` window alive!
    ///
    /// When another browser is currently active in GUI mode and `explicit_mode` was omitted,
    /// launches companion `MiniBrowse` in headless mode (~28MB) to avoid popping up a second window.
    pub async fn get_or_launch_debug(
        explicit_mode: Option<BrowserMode>,
        workspace_root: &Path,
    ) -> Result<Arc<SharedEngine>> {
        let desired_mode = match explicit_mode {
            Some(m) => m,
            None => {
                let guard = LIVE_POOL.lock().await;
                if let Some(pool) = guard.as_ref() {
                    if let Some(mb) = pool.engines.get(&BrowserEngine::MiniBrowse) {
                        mb.effective_mode()
                    } else {
                        BrowserMode::Headless
                    }
                } else {
                    BrowserMode::Headless
                }
            }
        };
        let scope_engine = current_space_scope().engine;
        Self::get_or_launch_for(
            BrowserPurpose::Debug,
            desired_mode,
            scope_engine,
            workspace_root,
        )
        .await
    }

    /// Resolves and launches/reuses an engine from the multi-engine pool according to `purpose`,
    /// `mode`, optional per-call `explicit_engine`, and user configuration.
    pub async fn get_or_launch_for(
        purpose: BrowserPurpose,
        mode: BrowserMode,
        explicit_engine: Option<BrowserEngine>,
        workspace_root: &Path,
    ) -> Result<Arc<SharedEngine>> {
        let mut guard = LIVE_POOL.lock().await;
        let pool = guard.get_or_insert_with(EnginePool::default);

        // Determine sticky live candidate:
        // - For General calls with no explicit per-call override and no env override,
        //   continue using `pool.active_general` if already alive (preserves ARIA ref continuity
        //   and avoids retrying a broken `default_engine` once fallback succeeded).
        let sticky_candidate = if purpose == BrowserPurpose::General
            && explicit_engine.is_none()
            && std::env::var("MINICODE_BROWSER").is_err()
        {
            let cfg_forced = crate::config::Config::load(Some(workspace_root), None)
                .ok()
                .and_then(|c| c.browser.default_engine)
                .and_then(|s| parse_engine_name(&s));
            match (cfg_forced, pool.active_general) {
                (Some(forced), Some(active))
                    if forced == active || pool.is_in_failure_cooldown(forced, mode) =>
                {
                    Some(active)
                }
                (None, Some(active)) => Some(active),
                _ => None,
            }
        } else {
            None
        };

        // Build ordered launch candidates: sticky live engine first (if any), followed by all
        // installed engines in resolved priority order.
        let mut candidates =
            Self::discover_all_engines_for(purpose, mode, explicit_engine, workspace_root);
        if let Some(sticky_eng) = sticky_candidate {
            if let Some(existing) = pool.engines.get(&sticky_eng) {
                let existing_cfg = existing.process.config.clone();
                candidates.retain(|c| c.engine != sticky_eng);
                candidates.insert(0, existing_cfg);
            }
        }

        if candidates.is_empty() {
            return Err(missing_engine_error(explicit_engine).into());
        }

        // Partition healthy vs recently-failed engines when caller did not pin an explicit engine:
        // skip engines in failure cooldown while healthy fallbacks exist, keeping them only as a
        // last resort so a broken default (e.g., Firefox headless WebDriver) never hangs every call
        // for 8s or degrades to plain HTTP while MiniBrowse/Chrome are ready.
        if explicit_engine.is_none() && candidates.len() > 1 {
            let (healthy, cooling): (Vec<_>, Vec<_>) = candidates
                .into_iter()
                .partition(|c| !pool.is_in_failure_cooldown(c.engine, mode));
            candidates = if healthy.is_empty() { cooling } else { healthy };
        }

        let mut last_err: Option<crate::error::MinicodeError> = None;

        for mut launch_cfg in candidates {
            let chosen_engine = launch_cfg.engine;

            if let Some(existing) = pool.engines.get(&chosen_engine).cloned() {
                let needs_gui_upgrade =
                    mode == BrowserMode::Gui && existing.effective_mode() == BrowserMode::Headless;
                if needs_gui_upgrade {
                    if existing.process.config.engine == BrowserEngine::MiniBrowse {
                        if let Ok(res) = existing.cdp.set_window_mode(true, false, None).await {
                            if res.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                                tracing::info!(
                                    "Promoted live MiniBrowse headless session to GUI window in-place (zero state loss)"
                                );
                                existing.set_gui_promoted(true);
                                existing.set_aura_enabled(true);
                                if purpose == BrowserPurpose::General {
                                    pool.active_general = Some(chosen_engine);
                                }
                                return Ok(existing);
                            }
                        }
                    }
                    tracing::info!(
                        engine = %chosen_engine,
                        "Upgrading pooled headless browser engine to GUI mode via restart"
                    );
                    if let Some(old) = pool.engines.remove(&chosen_engine) {
                        if !old.process.external_attached {
                            if let Some(pid) = old.process.pid() {
                                kill_engine_process_tree(pid).await;
                            }
                        }
                    }
                } else {
                    let alive = existing
                        .cdp
                        .send_command_with_timeout(
                            "Runtime.evaluate",
                            serde_json::json!({ "expression": "1 + 1", "returnByValue": true }),
                            Duration::from_secs(2),
                        )
                        .await
                        .is_ok();
                    if alive {
                        if purpose == BrowserPurpose::General {
                            pool.active_general = Some(chosen_engine);
                        }
                        pool.clear_engine_failure(chosen_engine, mode);
                        return Ok(existing);
                    }
                    tracing::warn!(engine = %chosen_engine, "Pooled browser engine died; relaunching");
                    if let Some(dead) = pool.engines.remove(&chosen_engine) {
                        if !dead.process.external_attached {
                            if let Some(pid) = dead.process.pid() {
                                kill_engine_process_tree(pid).await;
                            }
                        }
                    }
                }
            }

            launch_cfg.mode = mode;
            launch_cfg.cdp_port = find_available_port(BROWSER_CDP_BASE_PORT);

            let mut proc = match Self::spawn_engine(&launch_cfg).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        engine = %chosen_engine,
                        mode = %mode,
                        error = %e,
                        "Failed to spawn browser engine; trying next fallback engine if available"
                    );
                    pool.mark_engine_failed(chosen_engine, mode);
                    last_err = Some(e);
                    continue;
                }
            };

            let cdp = match CdpClient::connect_for_engine(
                &proc.cdp_http_url,
                launch_cfg.engine,
                launch_cfg.mode,
            )
            .await
            {
                Ok(cdp) => Arc::new(cdp),
                Err(e) => {
                    tracing::warn!(
                        engine = %chosen_engine,
                        mode = %mode,
                        error = %e,
                        "Browser engine handshake failed; shutting down and trying next fallback engine"
                    );
                    let _ = proc.shutdown().await;
                    pool.mark_engine_failed(chosen_engine, mode);
                    last_err = Some(e);
                    continue;
                }
            };

            tracing::info!(
                engine = %launch_cfg.engine,
                purpose = ?purpose,
                "Browser engine ready (persistent multi-engine pool)"
            );
            let is_gui = launch_cfg.mode == BrowserMode::Gui;
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
            pool.clear_engine_failure(chosen_engine, mode);
            pool.engines.insert(chosen_engine, Arc::clone(&handle));
            if purpose == BrowserPurpose::General || pool.active_general.is_none() {
                pool.active_general = Some(chosen_engine);
            }
            Self::record_live_activity(1, None, "ready");
            return Ok(handle);
        }

        Err(last_err.unwrap_or_else(|| missing_engine_error(explicit_engine).into()))
    }

    /// Updates the preferred general engine pointer when the user changes `browser.default_engine`
    /// in `/settings` or `update_agent_config`.
    pub async fn notify_config_updated(default_engine: Option<&str>) {
        let mut guard = LIVE_POOL.lock().await;
        if let Some(ref mut pool) = *guard {
            pool.active_general = default_engine.and_then(parse_engine_name);
            pool.failed_engines.clear();
        }
    }

    /// Returns an already-running pooled browser engine without spawning any new process.
    /// Honors task-local `engine` override when present, otherwise returns `primary_live_engine()`.
    pub async fn peek_live_engine() -> Option<Arc<SharedEngine>> {
        let scope_engine = current_space_scope().engine;
        let guard = LIVE_POOL.lock().await;
        let pool = guard.as_ref()?;
        if let Some(eng) = scope_engine {
            pool.engines.get(&eng).cloned()
        } else {
            pool.primary_live_engine().cloned()
        }
    }

    /// When a non-MiniBrowse general browser (`Chrome`, `Firefox`, `Safari`) is currently active,
    /// returns `(engine_type, current_url_opt, formatted_runtime_logs)` so `MiniBrowse` debug tools
    /// can automatically inspect the same active URL and fuse live runtime logs from that browser!
    pub async fn snapshot_companion_general_context(
    ) -> Option<(BrowserEngine, Option<String>, String)> {
        let companion = {
            let guard = LIVE_POOL.lock().await;
            let pool = guard.as_ref()?;
            let active = pool.active_general?;
            if active == BrowserEngine::MiniBrowse {
                return None;
            }
            pool.engines.get(&active).cloned().map(|e| (active, e))
        }?;
        let (eng, handle) = companion;
        let url_opt = handle
            .cdp
            .evaluate_js("window.location.href")
            .await
            .ok()
            .map(|u| u.trim().trim_matches('"').to_string())
            .filter(|u| {
                u.starts_with("http://") || u.starts_with("https://") || u.starts_with("file://")
            });
        let logs = handle.cdp.debug_collector().format_report();
        Some((eng, url_opt, logs))
    }

    /// Discovers Chrome / Brave / Chromium / Edge `DevToolsActivePort` files (`browser-harness` pattern)
    /// and attaches to the user's running browser in a fresh background tab without stealing focus or killing on close.
    pub async fn attach_local_running_browser() -> Result<String> {
        let (port, ws_url, browser_label) = discover_devtools_active_port().ok_or_else(|| {
            ToolError::CommandExec(
                "No running local Chrome/Brave/Chromium DevToolsActivePort found. Enable remote debugging in `chrome://inspect/#remote-debugging` (or `brave://inspect/#remote-debugging`) or set `MINICODE_CDP_WS_URL`.".to_string(),
            )
        })?;

        let cdp = if ws_url.starts_with("ws://") || ws_url.starts_with("wss://") {
            CdpClient::connect_browser_ws_background(&ws_url).await?
        } else {
            CdpClient::connect(&format!("http://127.0.0.1:{port}")).await?
        };

        let mut guard = LIVE_POOL.lock().await;
        let pool = guard.get_or_insert_with(EnginePool::default);
        if let Some(old) = pool.engines.remove(&BrowserEngine::Chrome) {
            if !old.process.external_attached {
                if let Some(pid) = old.process.pid() {
                    kill_engine_process_tree(pid).await;
                }
            }
        }

        let cfg = EngineConfig::new(
            BrowserEngine::Chrome,
            BrowserMode::Gui,
            port,
            PathBuf::from(&browser_label),
            std::env::temp_dir(),
        );
        let proc = EngineProcess {
            config: cfg,
            child: None,
            external_attached: true,
            cdp_port: port,
            cdp_http_url: format!("http://127.0.0.1:{port}"),
        };
        let handle = Arc::new(SharedEngine {
            aura_enabled: std::sync::atomic::AtomicBool::new(false),
            gui_promoted: std::sync::atomic::AtomicBool::new(true),
            default_owner: std::sync::atomic::AtomicU8::new(SpaceOwner::Agent as u8),
            process: proc,
            cdp: Arc::new(cdp),
            accessibility: Arc::new(tokio::sync::Mutex::new(AccessibilityManager::new())),
            spaces: tokio::sync::Mutex::new(HashMap::new()),
            started_at: Instant::now(),
        });
        pool.engines.insert(BrowserEngine::Chrome, handle);
        pool.active_general = Some(BrowserEngine::Chrome);
        Self::record_live_activity(1, None, "attached");
        Ok(format!(
            "🔗 Attached to running local `{browser_label}` on port `{port}` (`DevToolsActivePort`) in an isolated background tab (your active tabs and browser process will never be closed by minicode; debug tools continue using `MiniBrowse`)."
        ))
    }

    /// Updates lock-free inline TUI browser activity telemetry (`agweb` live multi-Space visibility).
    pub fn record_live_activity(space_count: usize, space_name: Option<&str>, action: &str) {
        let label = match space_name.filter(|s| !s.is_empty() && *s != "default") {
            Some(sp) => format!("{sp}:{action}"),
            None => action.to_string(),
        };
        if let Ok(mut g) = LIVE_BROWSER_TELEMETRY.lock() {
            *g = Some((space_count.max(1), label));
        }
    }

    /// Reads a compact status-bar badge string synchronously (`<1µs` cached, non-blocking for Ratatui render loop),
    /// including full process-tree PSS memory across all live pooled browser processes.
    pub fn live_spaces_badge_sync() -> Option<String> {
        let (pids, engine_tag) = {
            let Ok(guard) = LIVE_POOL.try_lock() else {
                return None;
            };
            let pool = guard.as_ref()?;
            if pool.engines.is_empty() {
                return None;
            }
            let pids: Vec<u32> = pool
                .engines
                .values()
                .filter_map(|e| e.process.pid())
                .collect();
            let active_eng = pool
                .primary_live_engine()
                .map(|e| e.process.config.engine)
                .unwrap_or(BrowserEngine::MiniBrowse);
            let tag = match active_eng {
                BrowserEngine::MiniBrowse => "web",
                BrowserEngine::Chrome => "web:chrome",
                BrowserEngine::Firefox => "web:firefox",
                BrowserEngine::Safari => "web:safari",
                BrowserEngine::Obscura => "web:obscura",
            };
            (pids, tag)
        };

        let pss_mb_opt = if pids.is_empty() {
            None
        } else {
            let mut cache = LIVE_BROWSER_MEM_CACHE.lock().ok()?;
            if let Some((ts, cached_mb)) = *cache {
                if ts.elapsed() < Duration::from_secs(3) {
                    Some(cached_mb)
                } else {
                    let total: u64 = pids
                        .iter()
                        .filter_map(|&pid| crate::dev::ports::sample_process_tree_memory_mb(pid))
                        .map(|(pss, _, _)| pss)
                        .sum();
                    *cache = Some((Instant::now(), total));
                    (total > 0).then_some(total)
                }
            } else {
                let total: u64 = pids
                    .iter()
                    .filter_map(|&pid| crate::dev::ports::sample_process_tree_memory_mb(pid))
                    .map(|(pss, _, _)| pss)
                    .sum();
                *cache = Some((Instant::now(), total));
                (total > 0).then_some(total)
            }
        };

        let guard = LIVE_BROWSER_TELEMETRY.lock().ok()?;
        let (spaces, ref last_act) = *guard.as_ref()?;
        let mem_suffix = pss_mb_opt
            .map(|mb| format!(" • {mb}MB"))
            .unwrap_or_default();
        if spaces > 1 {
            Some(format!(
                "{engine_tag}:{spaces} spaces [{last_act}{mem_suffix}]"
            ))
        } else {
            Some(format!("{engine_tag}:[{last_act}{mem_suffix}]"))
        }
    }

    /// Shuts down pooled live engines, killing child processes and descendants cleanly.
    /// Preserves GUI browser instances unless `force: true` is passed.
    pub async fn shutdown_live_engine() -> Result<bool> {
        Self::shutdown_live_engine_opt(false).await
    }

    /// Shuts down pooled live engines with optional forced override for GUI instances.
    pub async fn shutdown_live_engine_opt(force: bool) -> Result<bool> {
        Self::shutdown_engine_opt(None, force).await
    }

    /// Shuts down either a specific `target_engine` or all pooled engines (`None`), preserving GUI windows unless `force`.
    pub async fn shutdown_engine_opt(
        target_engine: Option<BrowserEngine>,
        force: bool,
    ) -> Result<bool> {
        let mut guard = LIVE_POOL.lock().await;
        let Some(ref mut pool) = *guard else {
            return Ok(false);
        };

        let keys: Vec<BrowserEngine> = match target_engine {
            Some(e) => vec![e],
            None => pool.engines.keys().copied().collect(),
        };

        let mut stopped_any = false;
        for eng in keys {
            let should_preserve = pool.engines.get(&eng).is_some_and(|engine| {
                !engine.process.external_attached
                    && engine.effective_mode() == BrowserMode::Gui
                    && !force
            });
            if should_preserve {
                tracing::info!(
                    engine = %eng,
                    "Live browser engine is running in GUI mode; preserving window for user viewing."
                );
                continue;
            }
            if let Some(engine) = pool.engines.remove(&eng) {
                if !engine.process.external_attached {
                    if let Some(p) = engine.process.pid() {
                        kill_engine_process_tree(p).await;
                    }
                }
                if pool.active_general == Some(eng) {
                    pool.active_general = None;
                }
                stopped_any = true;
            }
        }

        if pool.engines.is_empty() {
            if let Ok(mut tg) = LIVE_BROWSER_TELEMETRY.lock() {
                *tg = None;
            }
            if let Ok(mut mc) = LIVE_BROWSER_MEM_CACHE.lock() {
                *mc = None;
            }
            crate::tools::browser::BrowserController::reset_screenshot_counter();
        }
        if stopped_any {
            tracing::info!("Live browser engine(s) shut down / detached cleanly.");
        }
        Ok(stopped_any)
    }

    /// Returns the execution mode (`Headless` or `Gui`) of the currently running primary general engine, if any.
    #[allow(dead_code)]
    pub async fn live_engine_mode() -> Option<BrowserMode> {
        let guard = LIVE_POOL.lock().await;
        guard
            .as_ref()
            .and_then(|p| p.primary_live_engine())
            .map(|e| e.effective_mode())
    }

    /// Checks if any live browser engine is currently running and alive.
    pub async fn is_live_engine_running() -> bool {
        Self::get_live_engine_details().await.is_some()
    }

    /// Checks synchronously if any live browser engine is currently running and alive.
    pub fn is_live_engine_running_sync() -> bool {
        if let Ok(guard) = LIVE_POOL.try_lock() {
            if let Some(ref pool) = *guard {
                pool.engines.values().any(|engine| {
                    if engine.process.external_attached {
                        return true;
                    }
                    let pid = engine.process.pid().unwrap_or(0);
                    pid > 0 && crate::dev::ports::is_process_running(pid)
                })
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Gets summary of current live primary engine if running: (engine_name, pid, cdp_port).
    pub async fn get_live_engine_info() -> Option<(String, u32, u16)> {
        Self::get_live_engine_details()
            .await
            .map(|(name, pid, port, _)| (name, pid, port))
    }

    /// Gets detailed live engine telemetry: (engine_name, pid, cdp_port, uptime_secs).
    pub async fn get_live_engine_details() -> Option<(String, u32, u16, u64)> {
        let mut guard = LIVE_POOL.lock().await;
        let pool = guard.as_mut()?;

        // Reap any dead processes first
        let dead_keys: Vec<BrowserEngine> = pool
            .engines
            .iter()
            .filter_map(|(&eng, handle)| {
                if handle.process.external_attached {
                    return None;
                }
                let pid = handle.process.pid().unwrap_or(0);
                if pid == 0 || !crate::dev::ports::is_process_running(pid) {
                    Some(eng)
                } else {
                    None
                }
            })
            .collect();

        for dead_eng in dead_keys {
            if let Some(dead) = pool.engines.remove(&dead_eng) {
                if let Some(p) = dead.process.pid() {
                    crate::dev::registry::get_global_dev_registry().unregister_external_pid(p);
                }
            }
            if pool.active_general == Some(dead_eng) {
                pool.active_general = None;
            }
        }

        if pool.engines.is_empty() {
            if let Ok(mut tg) = LIVE_BROWSER_TELEMETRY.lock() {
                *tg = None;
            }
            return None;
        }

        let engine = pool.primary_live_engine()?;
        let uptime = engine.started_at.elapsed().as_secs();
        if engine.process.external_attached {
            return Some((
                "Chrome (Attached)".to_string(),
                0,
                engine.process.cdp_port,
                uptime,
            ));
        }
        let pid = engine.process.pid().unwrap_or(0);
        let label = if pool.engines.len() > 1 {
            let names: Vec<String> = pool.engines.keys().map(ToString::to_string).collect();
            format!(
                "{} [pool: {}]",
                engine.process.config.engine,
                names.join("+")
            )
        } else {
            engine.process.config.engine.to_string()
        };
        Some((label, pid, engine.process.cdp_port, uptime))
    }
}

#[derive(Clone, Default)]
pub struct BrowserManager;

fn missing_engine_error(explicit_engine: Option<BrowserEngine>) -> ToolError {
    match explicit_engine {
        Some(BrowserEngine::Safari) => ToolError::CommandExec(
            "Requested browser engine 'Safari' ('safaridriver') was not found on PATH (available on macOS via /usr/bin/safaridriver)."
                .to_string(),
        ),
        Some(eng) => ToolError::CommandExec(format!(
            "Requested browser engine '{eng}' was not found on PATH."
        )),
        None => ToolError::CommandExec(
            "No browser engine (MiniBrowse, Chrome, Firefox, Safari, Obscura) found on PATH"
                .to_string(),
        ),
    }
}

pub fn parse_engine_name(name: &str) -> Option<BrowserEngine> {
    BrowserEngine::parse(name)
}

impl BrowserManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

    /// Discovers an available general browser binary according to priority rules (`MiniBrowse` primary) and configuration.
    pub fn discover_engine(mode: BrowserMode, workspace_root: &Path) -> Option<EngineConfig> {
        Self::discover_engine_for(BrowserPurpose::General, mode, None, workspace_root)
    }

    /// Resolves the ordered engine priority chain for `purpose`, `mode`, optional per-call `explicit_engine`,
    /// environment overrides, and `Config` settings (useful for both launch resolution and deterministic testing).
    pub fn resolve_priority_chain(
        purpose: BrowserPurpose,
        mode: BrowserMode,
        explicit_engine: Option<BrowserEngine>,
        workspace_root: &Path,
    ) -> Vec<BrowserEngine> {
        // 1. Explicit per-tool-call engine requested by user/agent (`engine="chrome" | "firefox" | "safari" | "minibrowse"`)
        if let Some(eng) = explicit_engine {
            return vec![eng];
        }

        // 2. Debug workloads ALWAYS prioritize `MiniBrowse` (`minibrowser`), ignoring general `MINICODE_BROWSER`
        //    and `browser.default_engine` so debugging always gets MiniBrowse's native Shadow DOM, CSS layout,
        //    WCAG, and Flutter/Dart VM diagnostics!
        if purpose == BrowserPurpose::Debug {
            let mut chain = Vec::with_capacity(DEBUG_PRIORITY.len() + 1);
            if let Ok(dbg_env) = std::env::var("MINICODE_DEBUG_BROWSER") {
                if let Some(eng) = parse_engine_name(&dbg_env) {
                    chain.push(eng);
                }
            } else if let Ok(cfg) = crate::config::Config::load(Some(workspace_root), None) {
                if let Some(eng) = parse_engine_name(&cfg.browser.debug_engine) {
                    chain.push(eng);
                }
            }
            if !chain.contains(&BrowserEngine::MiniBrowse) {
                chain.insert(0, BrowserEngine::MiniBrowse);
            }
            for &fallback in DEBUG_PRIORITY {
                if !chain.contains(&fallback) {
                    chain.push(fallback);
                }
            }
            return chain;
        }

        // 3. General workloads: `MiniBrowse` is primary by default, or user-configured `Chrome` / `Firefox` / `Safari`
        let default_priority = match mode {
            BrowserMode::Headless => HEADLESS_PRIORITY,
            BrowserMode::Gui => GUI_PRIORITY,
        };

        if let Ok(name) = std::env::var("MINICODE_BROWSER") {
            if let Some(engine) = parse_engine_name(&name) {
                return vec![engine];
            }
            return default_priority.to_vec();
        }

        if let Ok(cfg) = crate::config::Config::load(Some(workspace_root), None) {
            if let Some(ref forced) = cfg.browser.default_engine {
                if !forced.trim().is_empty() && !forced.eq_ignore_ascii_case("auto") {
                    if let Some(engine) = parse_engine_name(forced) {
                        let mut list = vec![engine];
                        // Keep full configured + default fallback chain (MiniBrowse, Chrome, etc.)
                        // so a broken default engine degrades gracefully to another real browser
                        // instead of falling all the way back to a plain HTTP reader.
                        let configured_names = match mode {
                            BrowserMode::Headless => &cfg.browser.headless_priority,
                            BrowserMode::Gui => &cfg.browser.gui_priority,
                        };
                        for name in configured_names {
                            if let Some(fb) = parse_engine_name(name) {
                                if !list.contains(&fb) {
                                    list.push(fb);
                                }
                            }
                        }
                        for &fb in default_priority {
                            if !list.contains(&fb) {
                                list.push(fb);
                            }
                        }
                        return list;
                    }
                }
            }

            let configured_names = match mode {
                BrowserMode::Headless => &cfg.browser.headless_priority,
                BrowserMode::Gui => &cfg.browser.gui_priority,
            };
            let mut parsed: Vec<BrowserEngine> = configured_names
                .iter()
                .filter_map(|n| parse_engine_name(n))
                .collect();

            // Migrate legacy saved priority lists so MiniBrowse is always primary, followed by Chrome, Firefox, Safari
            if parsed.first() == Some(&BrowserEngine::Obscura)
                && parsed.contains(&BrowserEngine::Chrome)
            {
                parsed.retain(|&e| e != BrowserEngine::Chrome);
                parsed.insert(0, BrowserEngine::Chrome);
            }
            if !parsed.is_empty() && !parsed.contains(&BrowserEngine::MiniBrowse) {
                parsed.insert(0, BrowserEngine::MiniBrowse);
            }
            if !parsed.is_empty() && !parsed.contains(&BrowserEngine::Safari) {
                parsed.push(BrowserEngine::Safari);
            }
            if parsed.is_empty() {
                default_priority.to_vec()
            } else {
                parsed
            }
        } else {
            default_priority.to_vec()
        }
    }

    /// Discovers an available browser binary for `purpose` (`General` vs `Debug`), `mode`, and optional `explicit_engine`.
    /// Discovers all installed browser binaries in priority order for `purpose`, `mode`,
    /// and optional `explicit_engine`, enabling automatic fallback when a primary engine fails to start.
    pub fn discover_all_engines_for(
        purpose: BrowserPurpose,
        mode: BrowserMode,
        explicit_engine: Option<BrowserEngine>,
        workspace_root: &Path,
    ) -> Vec<EngineConfig> {
        let priority = Self::resolve_priority_chain(purpose, mode, explicit_engine, workspace_root);
        let ws_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut s = DefaultHasher::new();
            workspace_root.hash(&mut s);
            format!("{:08x}", s.finish() as u32)
        };

        let mut configs = Vec::new();
        for &engine in &priority {
            if let Some(binary_path) = find_binary_for_engine(engine) {
                let port = find_available_port(BROWSER_CDP_BASE_PORT);
                let profile_dir = std::env::temp_dir()
                    .join("minicode")
                    .join("browser_profiles")
                    .join(format!("{}_{}_{}", ws_hash, engine.as_config_slug(), mode));

                configs.push(EngineConfig::new(
                    engine,
                    mode,
                    port,
                    binary_path,
                    profile_dir,
                ));
            }
        }
        configs
    }

    /// Discovers the highest-priority available browser binary for `purpose` (`General` vs `Debug`),
    /// `mode`, and optional `explicit_engine`.
    pub fn discover_engine_for(
        purpose: BrowserPurpose,
        mode: BrowserMode,
        explicit_engine: Option<BrowserEngine>,
        workspace_root: &Path,
    ) -> Option<EngineConfig> {
        Self::discover_all_engines_for(purpose, mode, explicit_engine, workspace_root)
            .into_iter()
            .next()
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

        // For Firefox: write user.js enabling remote debugging (WebDriver BiDi + CDP) without prompts
        if config.engine == BrowserEngine::Firefox {
            let user_js_path = config.profile_dir.join("user.js");
            let user_js_content = r#"
user_pref("devtools.debugger.remote-enabled", true);
user_pref("devtools.chrome.enabled", true);
user_pref("devtools.debugger.prompt-connection", false);
user_pref("remote.active-protocols", 3);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("datareporting.policy.dataSubmissionEnabled", false);
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
                let (mode_flag, w_str, h_str) = match config.mode {
                    BrowserMode::Headless => ("--headless", "1440", "900"),
                    BrowserMode::Gui => ("--gui", "1280", "800"),
                };
                vec![
                    "serve".to_string(),
                    "--port".to_string(),
                    port_str,
                    mode_flag.to_string(),
                    "--width".to_string(),
                    w_str.to_string(),
                    "--height".to_string(),
                    h_str.to_string(),
                    "--adblock".to_string(),
                ]
            }
            BrowserEngine::Safari => {
                let bin_name = config
                    .binary_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("safaridriver")
                    .to_ascii_lowercase();
                if bin_name.contains("webkitwebdriver") {
                    vec![format!("--port={port_str}")]
                } else {
                    vec!["-p".to_string(), port_str]
                }
            }
            BrowserEngine::Obscura => {
                vec![
                    "--stealth".to_string(),
                    "--allow-private-network".to_string(),
                    "serve".to_string(),
                    "--port".to_string(),
                    port_str,
                ]
            }
            BrowserEngine::Firefox => {
                let bin_name = config
                    .binary_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("firefox")
                    .to_ascii_lowercase();
                if bin_name.contains("geckodriver") {
                    vec!["--port".to_string(), port_str]
                } else {
                    match config.mode {
                        BrowserMode::Headless => vec![
                            "--headless".to_string(),
                            "--remote-debugging-port".to_string(),
                            port_str,
                            "--profile".to_string(),
                            profile_str,
                            "--no-remote".to_string(),
                        ],
                        BrowserMode::Gui => vec![
                            "--remote-debugging-port".to_string(),
                            port_str,
                            "--profile".to_string(),
                            profile_str,
                            "--no-remote".to_string(),
                        ],
                    }
                }
            }
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

    /// Spawns the browser engine process and waits until the CDP / WebDriver / BiDi endpoint is responsive
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

        if config.engine == BrowserEngine::Obscura {
            cmd.env("OBSCURA_SCRIPT_DEADLINE_MS", "15000");
            cmd.env("OBSCURA_MODULE_BUDGET_MS", "5000");
        }

        let mut child = cmd.spawn().map_err(|e| {
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

        let start = Instant::now();
        let timeout = Duration::from_millis(BROWSER_STARTUP_TIMEOUT_MS);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(400))
            .build()
            .unwrap_or_default();

        let mut ready = false;
        let version_url = format!("{}/json/version", cdp_http_url);
        let status_url = format!("{}/status", cdp_http_url);

        while start.elapsed() < timeout {
            if let Ok(Some(exit_status)) = child.try_wait() {
                if let Some(pid) = child.id() {
                    crate::dev::registry::get_global_dev_registry().unregister_external_pid(pid);
                }
                return Err(ToolError::CommandExec(format!(
                    "{} ({}) exited immediately during startup (status: {})",
                    config.engine,
                    config.binary_path.display(),
                    exit_status
                ))
                .into());
            }
            if config.engine == BrowserEngine::Safari {
                if let Ok(resp) = client.get(&status_url).send().await {
                    if resp.status().is_success() {
                        ready = true;
                        break;
                    }
                }
            } else {
                if let Ok(resp) = client.get(&version_url).send().await {
                    if resp.status().is_success() {
                        ready = true;
                        break;
                    }
                }
                if config.engine == BrowserEngine::Firefox {
                    if let Ok(resp) = client.get(&status_url).send().await {
                        if resp.status().is_success() {
                            ready = true;
                            break;
                        }
                    }
                    // Firefox 141+ `--remote-debugging-port` speaks WebDriver BiDi on `/session`
                    // (HTTP GET `/json/version` returns 404 while TCP port is actively listening).
                    if crate::dev::ports::is_port_listening(config.cdp_port) {
                        tokio::time::sleep(Duration::from_millis(120)).await;
                        ready = true;
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if !ready {
            tracing::warn!(
                engine = %config.engine,
                port = config.cdp_port,
                "Browser did not respond within startup timeout; proceeding with best effort"
            );
        }

        Ok(EngineProcess {
            config: config.clone(),
            child: Some(child),
            external_attached: false,
            cdp_port: config.cdp_port,
            cdp_http_url,
        })
    }
}

/// Scans standard Chrome / Brave / Chromium / Edge user-data directories (or `MINICODE_CDP_WS_URL`)
/// for a live `DevToolsActivePort` file (`<port>\n/devtools/browser/<uuid>`).
pub fn discover_devtools_active_port() -> Option<(u16, String, String)> {
    if let Ok(ws_env) = std::env::var("MINICODE_CDP_WS_URL") {
        let trimmed = ws_env.trim();
        if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
            let port = url::Url::parse(trimmed)
                .ok()
                .and_then(|u| u.port())
                .unwrap_or(9222);
            return Some((port, trimmed.to_string(), "MINICODE_CDP_WS_URL".to_string()));
        }
    }

    let home = dirs::home_dir()?;
    let candidates: &[(&str, PathBuf)] = &[
        (
            "Google Chrome",
            home.join(".config/google-chrome/DevToolsActivePort"),
        ),
        (
            "Brave Browser",
            home.join(".config/BraveSoftware/Brave-Browser/DevToolsActivePort"),
        ),
        ("Chromium", home.join(".config/chromium/DevToolsActivePort")),
        (
            "Microsoft Edge",
            home.join(".config/microsoft-edge/DevToolsActivePort"),
        ),
        (
            "Google Chrome",
            home.join("Library/Application Support/Google/Chrome/DevToolsActivePort"),
        ),
        (
            "Brave Browser",
            home.join("Library/Application Support/BraveSoftware/Brave-Browser/DevToolsActivePort"),
        ),
    ];

    for (label, path) in candidates {
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut lines = raw.lines().map(str::trim).filter(|s| !s.is_empty());
        let Some(port_str) = lines.next() else {
            continue;
        };
        let Ok(port) = port_str.parse::<u16>() else {
            continue;
        };
        if !crate::dev::ports::is_port_listening(port) {
            continue;
        }
        let ws_path = lines.next().unwrap_or("/devtools/browser");
        let ws_url = format!("ws://127.0.0.1:{port}{ws_path}");
        return Some((port, ws_url, (*label).to_string()));
    }
    None
}

/// Helper to search local install paths, sibling build artifacts, macOS system paths, and PATH for a BrowserEngine
fn find_binary_for_engine(engine: BrowserEngine) -> Option<PathBuf> {
    if engine == BrowserEngine::MiniBrowse {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let sibling = dir.join("minibrowse");
                if sibling.is_file() {
                    return Some(sibling);
                }
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
        if let Some(home) = dirs::home_dir() {
            let local_bin = home.join(".local").join("bin").join("minibrowse");
            if local_bin.is_file() {
                return Some(local_bin);
            }
        }
        if let Ok(path) = which::which("minibrowse") {
            return Some(path);
        }
        return None;
    }

    if engine == BrowserEngine::Safari {
        let mac_driver = PathBuf::from("/usr/bin/safaridriver");
        if mac_driver.is_file() {
            return Some(mac_driver);
        }
        for name in ["safaridriver", "WebKitWebDriver", "webkitwebdriver"] {
            if let Ok(path) = which::which(name) {
                return Some(path);
            }
        }
        return None;
    }

    let candidates: &[&str] = match engine {
        BrowserEngine::MiniBrowse => &["minibrowse"],
        BrowserEngine::Safari => &["safaridriver"],
        BrowserEngine::Obscura => &["obscura"],
        BrowserEngine::Firefox => &[
            "geckodriver",
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

    #[cfg(target_os = "macos")]
    {
        let mac_app_bins: &[&str] = match engine {
            BrowserEngine::Chrome => &[
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
                "/Applications/Chromium.app/Contents/MacOS/Chromium",
            ],
            BrowserEngine::Firefox => &["/Applications/Firefox.app/Contents/MacOS/firefox"],
            _ => &[],
        };
        for &p in mac_app_bins {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
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
        assert_eq!(
            parse_engine_name("minibrowser"),
            Some(BrowserEngine::MiniBrowse)
        );
    }

    #[test]
    fn test_build_launch_args_safari_and_webkitwebdriver() {
        let safari_cfg = EngineConfig::new(
            BrowserEngine::Safari,
            BrowserMode::Gui,
            9230,
            PathBuf::from("/usr/bin/safaridriver"),
            PathBuf::from("/tmp/safari_profile"),
        );
        assert_eq!(
            BrowserManager::build_launch_args(&safari_cfg),
            vec!["-p", "9230"]
        );

        let wk_cfg = EngineConfig::new(
            BrowserEngine::Safari,
            BrowserMode::Headless,
            9231,
            PathBuf::from("/usr/bin/WebKitWebDriver"),
            PathBuf::from("/tmp/wk_profile"),
        );
        assert_eq!(
            BrowserManager::build_launch_args(&wk_cfg),
            vec!["--port=9231"]
        );
    }

    #[test]
    fn test_debug_purpose_always_routes_to_minibrowse_even_when_general_configured_to_firefox_or_chrome(
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::Config::default();
        cfg.browser.default_engine = Some("firefox".to_string());
        cfg.save_workspace(tmp.path()).unwrap();

        // General browsing honors user's configured `firefox` primary (with MiniBrowse fallback)
        let general_chain = BrowserManager::resolve_priority_chain(
            BrowserPurpose::General,
            BrowserMode::Gui,
            None,
            tmp.path(),
        );
        assert_eq!(general_chain[0], BrowserEngine::Firefox);
        assert!(
            general_chain.contains(&BrowserEngine::MiniBrowse)
                && general_chain.contains(&BrowserEngine::Chrome),
            "Forced default_engine must preserve full fallback chain (MiniBrowse + Chrome), got {:?}",
            general_chain
        );

        let mut pool = EnginePool::default();
        assert!(!pool.is_in_failure_cooldown(BrowserEngine::Firefox, BrowserMode::Headless));
        pool.mark_engine_failed(BrowserEngine::Firefox, BrowserMode::Headless);
        assert!(pool.is_in_failure_cooldown(BrowserEngine::Firefox, BrowserMode::Headless));
        assert!(!pool.is_in_failure_cooldown(BrowserEngine::Chrome, BrowserMode::Headless));
        pool.clear_engine_failure(BrowserEngine::Firefox, BrowserMode::Headless);
        assert!(!pool.is_in_failure_cooldown(BrowserEngine::Firefox, BrowserMode::Headless));

        // Explicit per-call `engine="chrome"` or `engine="safari"` overrides general config
        let explicit_chrome = BrowserManager::resolve_priority_chain(
            BrowserPurpose::General,
            BrowserMode::Gui,
            Some(BrowserEngine::Chrome),
            tmp.path(),
        );
        assert_eq!(explicit_chrome, vec![BrowserEngine::Chrome]);

        let explicit_safari = BrowserManager::resolve_priority_chain(
            BrowserPurpose::General,
            BrowserMode::Gui,
            Some(BrowserEngine::Safari),
            tmp.path(),
        );
        assert_eq!(explicit_safari, vec![BrowserEngine::Safari]);

        // Debug ALWAYS pins `MiniBrowse` first even though general browser is configured to Firefox!
        let debug_chain = BrowserManager::resolve_priority_chain(
            BrowserPurpose::Debug,
            BrowserMode::Headless,
            None,
            tmp.path(),
        );
        assert_eq!(debug_chain[0], BrowserEngine::MiniBrowse);
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
