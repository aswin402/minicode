//! Adaptive Two-Gear Browser Engine (`Gear 0: Pure-Rust Arena DOM` -> `Gear 1: WebKit2GTK + Cairo`)
//! with Ephemeral Multi-Tab / `BrowserContext` Isolation, Resource Profiles, and Auto-Hibernation.
//!
//! - Starts headless sessions in Gear 0 (`~7.3 MB` VmRSS, `0.0%` idle CPU) and handles static/SSR
//!   `browser_research` without ever loading `libwebkit2gtk`.
//! - Automatically escalates to Gear 1 (`webkit2gtk-offscreen`) when a page requires JS/SPA/Shadow DOM
//!   hydration or visual/layout/interactive CDP operations, and hibernates Gear 1 after idle timeout.
//! - Supports isolated tabs & ephemeral `BrowserContext`s inside a single daemon process.

use crate::fallback::{encode_solid_aura_png, MockRoute, PureRustPageState};
use crate::ffi::{
    base64_encode, trim_process_heap, tune_allocator_env, GError, WebKitGtkApi,
    CAIRO_FORMAT_ARGB32, CAIRO_STATUS_SUCCESS, GTK_WINDOW_TOPLEVEL, G_PRIORITY_DEFAULT_IDLE,
    WEBKIT_CACHE_MODEL_DOCUMENT_VIEWER, WEBKIT_HARDWARE_ACCELERATION_POLICY_NEVER,
    WEBKIT_HARDWARE_ACCELERATION_POLICY_ON_DEMAND, WEBKIT_LOAD_FINISHED,
    WEBKIT_SNAPSHOT_OPTIONS_NONE, WEBKIT_SNAPSHOT_REGION_VISIBLE,
    WEBKIT_USER_CONTENT_INJECTED_ALL_FRAMES, WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,
};
use crate::net_policy::{ResourceProfile, CLEAN_BROWSER_USER_AGENT};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, oneshot};

const BRIDGE_JS: &str = include_str!("bridge.js");

#[derive(Debug, Clone)]
pub enum BrowserBackendEvent {
    Console {
        level: String,
        text: String,
    },
    NetworkError {
        method: String,
        url: String,
        status: u16,
        error: String,
    },
    Lifecycle(String),
}

pub enum BrowserBackendCmd {
    Navigate {
        url: String,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    Reload {
        ignore_cache: bool,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    EvaluateJs {
        expression: String,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    AddInitScript {
        source: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetViewport {
        width: u32,
        height: u32,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetResourceProfile {
        profile: ResourceProfile,
        reply: oneshot::Sender<Result<(), String>>,
    },
    WaitForIdle {
        quiet_ms: u64,
        timeout_ms: u64,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    CaptureScreenshotPng {
        reply: oneshot::Sender<Result<String, String>>,
    },
    DispatchMouse {
        event_type: String,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        reply: oneshot::Sender<Result<(), String>>,
    },
    DispatchKey {
        event_type: String,
        key: String,
        code: String,
        modifiers: u32,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetMockRoutes {
        routes: Vec<MockRoute>,
        reply: oneshot::Sender<Result<usize, String>>,
    },
    SetBlockedPatterns {
        patterns: Vec<String>,
        reply: oneshot::Sender<Result<usize, String>>,
    },
    SetAdBlock {
        enabled: bool,
        reply: oneshot::Sender<Result<bool, String>>,
    },
    ResearchExtract {
        query: Option<String>,
        max_chars: usize,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    WebDebugAudit {
        reply: oneshot::Sender<Result<Value, String>>,
    },
    UiDiff {
        query: Option<String>,
        max_chars: usize,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    NetworkWaterfall {
        reply: oneshot::Sender<Result<Value, String>>,
    },
    StorageInspect {
        reply: oneshot::Sender<Result<Value, String>>,
    },
    CreateBrowserContext {
        reply: oneshot::Sender<String>,
    },
    DisposeBrowserContext {
        context_id: String,
        reply: oneshot::Sender<Result<usize, String>>,
    },
    ListBrowserContexts {
        reply: oneshot::Sender<Vec<String>>,
    },
    CreateTarget {
        url: Option<String>,
        context_id: Option<String>,
        reply: oneshot::Sender<Result<String, String>>,
    },
    ActivateTarget {
        target_id: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    CloseTarget {
        target_id: String,
        reply: oneshot::Sender<Result<bool, String>>,
    },
    ListTargets {
        reply: oneshot::Sender<Vec<Value>>,
    },
    SetCookies {
        cookies: Vec<(String, String)>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    GetCookies {
        reply: oneshot::Sender<Result<Vec<Value>, String>>,
    },
    GetStatus {
        reply: oneshot::Sender<Value>,
    },
    #[allow(dead_code)]
    Shutdown,
}

#[derive(Clone)]
pub struct BrowserEngineHandle {
    cmd_tx: tokio::sync::mpsc::UnboundedSender<BrowserBackendCmd>,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
    backend_kind: Arc<Mutex<String>>,
}

impl BrowserEngineHandle {
    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<BrowserBackendEvent> {
        self.events_tx.subscribe()
    }

    #[must_use]
    pub fn backend_name(&self) -> String {
        self.backend_kind
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "minibrowse".to_string())
    }

    pub fn send(&self, cmd: BrowserBackendCmd) -> Result<(), String> {
        self.cmd_tx
            .send(cmd)
            .map_err(|_| "MiniBrowse supervisor task terminated".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub headless: bool,
    pub width: u32,
    pub height: u32,
    pub initial_url: Option<String>,
    pub adblock: bool,
}

pub fn spawn_browser_backend(opts: LaunchOptions) -> Result<BrowserEngineHandle, String> {
    let (events_tx, _) = broadcast::channel(256);
    let force_fallback = std::env::var("MINIBROWSE_FORCE_FALLBACK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let has_display = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty())
        || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    let can_use_webkit = !force_fallback && has_display;

    let initial_kind = if !opts.headless && can_use_webkit {
        "webkit2gtk-gui"
    } else {
        "pure-rust-dom"
    };
    let backend_kind = Arc::new(Mutex::new(initial_kind.to_string()));
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();

    tokio::spawn(run_adaptive_supervisor(
        opts,
        can_use_webkit,
        cmd_rx,
        events_tx.clone(),
        Arc::clone(&backend_kind),
    ));

    Ok(BrowserEngineHandle {
        cmd_tx,
        events_tx,
        backend_kind,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
//  Adaptive Two-Gear & Multi-Tab Supervisor
// ─────────────────────────────────────────────────────────────────────────────

struct Gear0Tab {
    context_id: Option<String>,
    page: PureRustPageState,
    webkit_synced_url: Option<String>,
}

struct WebKitThreadHandle {
    cmd_tx: mpsc::Sender<WebKitActorCmd>,
    wakeup_fn: unsafe extern "C" fn(
        c_int,
        crate::ffi::GSourceFunc,
        *mut c_void,
        Option<crate::ffi::GDestroyNotify>,
    ) -> u32,
    state_ptr: usize,
}

impl WebKitThreadHandle {
    fn send(&self, cmd: WebKitActorCmd) -> Result<(), String> {
        self.cmd_tx
            .send(cmd)
            .map_err(|_| "WebKit actor thread terminated".to_string())?;
        unsafe {
            (self.wakeup_fn)(
                G_PRIORITY_DEFAULT_IDLE,
                drain_webkit_commands_idle,
                self.state_ptr as *mut c_void,
                None,
            );
        }
        Ok(())
    }
}

enum WebKitActorCmd {
    EnsureTab {
        target_id: String,
        ephemeral: bool,
        reply: oneshot::Sender<Result<(), String>>,
    },
    ActivateTab {
        target_id: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    CloseTab {
        target_id: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Navigate {
        url: String,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    Reload {
        ignore_cache: bool,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    EvaluateJs {
        expression: String,
        unwrap_val: bool,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    AddInitScript {
        source: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetViewport {
        width: u32,
        height: u32,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetResourceProfile {
        profile: ResourceProfile,
        reply: oneshot::Sender<Result<(), String>>,
    },
    CaptureScreenshotPng {
        reply: oneshot::Sender<Result<String, String>>,
    },
    SetPolicies {
        adblock: bool,
        mock_routes: Vec<MockRoute>,
        blocked_patterns: Vec<String>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    GetActiveMeta {
        reply: oneshot::Sender<(String, String)>,
    },
    Shutdown,
}

struct SupervisorState {
    opts: LaunchOptions,
    can_use_webkit: bool,
    webkit: Option<WebKitThreadHandle>,
    backend_kind: Arc<Mutex<String>>,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
    tabs: BTreeMap<String, Gear0Tab>,
    contexts: BTreeSet<String>,
    active_target_id: String,
    next_tab_seq: u64,
    next_ctx_seq: u64,
    viewport_width: u32,
    viewport_height: u32,
    adblock: bool,
    resource_profile: ResourceProfile,
    mock_routes: Vec<MockRoute>,
    blocked_patterns: Vec<String>,
    init_scripts: Vec<String>,
}

impl SupervisorState {
    fn set_kind_label(&self, label: &str) {
        if let Ok(mut g) = self.backend_kind.lock() {
            *g = label.to_string();
        }
    }

    fn active_tab_mut(&mut self) -> &mut Gear0Tab {
        let active_id = self.active_target_id.clone();
        if !self.tabs.contains_key(&active_id) {
            let fallback_id = self
                .tabs
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "main".to_string());
            if !self.tabs.contains_key(&fallback_id) {
                self.tabs.insert(
                    "main".to_string(),
                    Gear0Tab {
                        context_id: None,
                        page: PureRustPageState::new(
                            self.viewport_width,
                            self.viewport_height,
                            self.adblock,
                        ),
                        webkit_synced_url: None,
                    },
                );
                self.active_target_id = "main".to_string();
            } else {
                self.active_target_id = fallback_id;
            }
        }
        let id = self.active_target_id.clone();
        self.tabs.get_mut(&id).unwrap_or_else(|| unreachable!())
    }

    async fn ensure_gear1(&mut self) -> Result<bool, String> {
        if !self.can_use_webkit {
            return Ok(false);
        }
        if self.webkit.is_none() {
            let mut launch_opts = self.opts.clone();
            launch_opts.width = self.viewport_width;
            launch_opts.height = self.viewport_height;
            launch_opts.adblock = self.adblock;
            launch_opts.initial_url = None;

            match spawn_webkit_thread(launch_opts, self.events_tx.clone()) {
                Ok(wk) => {
                    self.set_kind_label(if self.opts.headless {
                        "webkit2gtk-offscreen"
                    } else {
                        "webkit2gtk-gui"
                    });
                    // Replay init scripts, resource profile, and network policies into fresh Gear 1
                    for src in &self.init_scripts {
                        let (tx, rx) = oneshot::channel();
                        let _ = wk.send(WebKitActorCmd::AddInitScript {
                            source: src.clone(),
                            reply: tx,
                        });
                        let _ = rx.await;
                    }
                    let (ptx, prx) = oneshot::channel();
                    let _ = wk.send(WebKitActorCmd::SetPolicies {
                        adblock: self.adblock,
                        mock_routes: self.mock_routes.clone(),
                        blocked_patterns: self.blocked_patterns.clone(),
                        reply: ptx,
                    });
                    let _ = prx.await;

                    let (rtx, rrx) = oneshot::channel();
                    let _ = wk.send(WebKitActorCmd::SetResourceProfile {
                        profile: self.resource_profile,
                        reply: rtx,
                    });
                    let _ = rrx.await;

                    self.webkit = Some(wk);
                }
                Err(_) => {
                    self.can_use_webkit = false;
                    return Ok(false);
                }
            }
        }

        // Ensure the active tab exists & is active in WebKit, and sync URL if Gear 0 navigated while Gear 1 was asleep
        let target_id = self.active_target_id.clone();
        let (ephemeral, desired_url, already_synced) = {
            let tab = self.active_tab_mut();
            let cur = tab.page.current_url.clone();
            let synced = tab.webkit_synced_url.as_deref() == Some(cur.as_str());
            (tab.context_id.is_some(), cur, synced)
        };

        if let Some(ref wk) = self.webkit {
            let (etx, erx) = oneshot::channel();
            wk.send(WebKitActorCmd::EnsureTab {
                target_id: target_id.clone(),
                ephemeral,
                reply: etx,
            })?;
            let _ = erx.await;

            let (atx, arx) = oneshot::channel();
            wk.send(WebKitActorCmd::ActivateTab {
                target_id,
                reply: atx,
            })?;
            let _ = arx.await;

            if !already_synced && desired_url != "about:blank" && !desired_url.is_empty() {
                let (ntx, nrx) = oneshot::channel();
                wk.send(WebKitActorCmd::Navigate {
                    url: desired_url.clone(),
                    reply: ntx,
                })?;
                let _ = tokio::time::timeout(Duration::from_secs(15), nrx).await;
                self.active_tab_mut().webkit_synced_url = Some(desired_url);
            }
        }
        Ok(true)
    }

    fn hibernate_gear1(&mut self) {
        if !self.opts.headless {
            return;
        }
        if let Some(wk) = self.webkit.take() {
            let _ = wk.send(WebKitActorCmd::Shutdown);
            for tab in self.tabs.values_mut() {
                tab.webkit_synced_url = None;
            }
            self.set_kind_label("pure-rust-dom");
            trim_process_heap();
        }
    }

    async fn eval_webkit_unwrapped(&self, expr: String) -> Result<Value, String> {
        let Some(ref wk) = self.webkit else {
            return Err("WebKit gear is not active".to_string());
        };
        let (tx, rx) = oneshot::channel();
        wk.send(WebKitActorCmd::EvaluateJs {
            expression: expr,
            unwrap_val: true,
            reply: tx,
        })?;
        tokio::time::timeout(Duration::from_secs(12), rx)
            .await
            .map_err(|_| "Timed out waiting for WebKit bridge evaluation".to_string())?
            .map_err(|_| "WebKit evaluation channel closed".to_string())?
    }
}

fn is_lightweight_probe_expr(expr: &str) -> bool {
    let t = expr.trim();
    matches!(
        t,
        "1 + 1"
            | "true"
            | "false"
            | "window.location.href"
            | "location.href"
            | "document.URL"
            | "document.title"
    ) || t.contains("scrollRestoration = 'manual'")
        || t.contains("const root = document.querySelector('#root, #app, #__next')")
}

async fn run_adaptive_supervisor(
    opts: LaunchOptions,
    can_use_webkit: bool,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<BrowserBackendCmd>,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
    backend_kind: Arc<Mutex<String>>,
) {
    let hibernate_secs = std::env::var("MINIBROWSE_HIBERNATE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(90)
        .max(10);

    let mut tabs = BTreeMap::new();
    tabs.insert(
        "main".to_string(),
        Gear0Tab {
            context_id: None,
            page: PureRustPageState::new(opts.width, opts.height, opts.adblock),
            webkit_synced_url: None,
        },
    );

    let mut sup = SupervisorState {
        viewport_width: opts.width.max(320),
        viewport_height: opts.height.max(240),
        adblock: opts.adblock,
        opts: opts.clone(),
        can_use_webkit,
        webkit: None,
        backend_kind,
        events_tx,
        tabs,
        contexts: BTreeSet::new(),
        active_target_id: "main".to_string(),
        next_tab_seq: 2,
        next_ctx_seq: 1,
        resource_profile: ResourceProfile::Full,
        mock_routes: Vec::new(),
        blocked_patterns: Vec::new(),
        init_scripts: Vec::new(),
    };

    // GUI mode opens the GTK window immediately; headless mode stays in ~7.3MB Gear 0 until needed!
    if !opts.headless && sup.can_use_webkit {
        let _ = sup.ensure_gear1().await;
    }

    if let Some(initial) = opts.initial_url.clone() {
        let (tx, _rx) = oneshot::channel();
        handle_supervisor_cmd(
            &mut sup,
            BrowserBackendCmd::Navigate {
                url: initial,
                reply: tx,
            },
        )
        .await;
    }

    loop {
        if sup.opts.headless && sup.webkit.is_some() {
            tokio::select! {
                maybe_cmd = cmd_rx.recv() => {
                    let Some(cmd) = maybe_cmd else { break; };
                    if matches!(cmd, BrowserBackendCmd::Shutdown) {
                        if let Some(wk) = sup.webkit.take() {
                            let _ = wk.send(WebKitActorCmd::Shutdown);
                        }
                        break;
                    }
                    handle_supervisor_cmd(&mut sup, cmd).await;
                }
                () = tokio::time::sleep(Duration::from_secs(hibernate_secs)) => {
                    sup.hibernate_gear1();
                }
            }
        } else {
            let Some(cmd) = cmd_rx.recv().await else {
                break;
            };
            if matches!(cmd, BrowserBackendCmd::Shutdown) {
                if let Some(wk) = sup.webkit.take() {
                    let _ = wk.send(WebKitActorCmd::Shutdown);
                }
                break;
            }
            handle_supervisor_cmd(&mut sup, cmd).await;
        }
    }
}

async fn handle_supervisor_cmd(sup: &mut SupervisorState, cmd: BrowserBackendCmd) {
    match cmd {
        BrowserBackendCmd::Navigate { url, reply } => {
            // If GUI mode, load directly in Gear 1
            if !sup.opts.headless && sup.can_use_webkit && sup.ensure_gear1().await.unwrap_or(false)
            {
                {
                    let tab = sup.active_tab_mut();
                    tab.page.current_url = url.clone();
                    tab.webkit_synced_url = Some(url.clone());
                }
                if let Some(ref wk) = sup.webkit {
                    let _ = wk.send(WebKitActorCmd::Navigate { url, reply });
                    return;
                }
            }

            // Always run fast Gear-0 fetch + Arena parse first (~5ms)
            let nav_res = {
                let routes = sup.mock_routes.clone();
                let blocked = sup.blocked_patterns.clone();
                let adblock = sup.adblock;
                let profile = sup.resource_profile;
                let tab = sup.active_tab_mut();
                tab.webkit_synced_url = None;
                tab.page.mock_routes = routes;
                tab.page.blocked_patterns = blocked;
                tab.page.adblock_enabled = adblock;
                tab.page.resource_profile = profile;
                tab.page.navigate(&url).await
            };

            match nav_res {
                Ok(net_opt) => {
                    let needs_hydration = sup.active_tab_mut().page.needs_js_hydration();
                    if sup.can_use_webkit
                        && (sup.webkit.is_some() || needs_hydration)
                        && sup.ensure_gear1().await.unwrap_or(false)
                    {
                        let final_url = sup.active_tab_mut().page.current_url.clone();
                        let already_loaded = sup.active_tab_mut().webkit_synced_url.as_deref()
                            == Some(final_url.as_str());
                        if already_loaded {
                            let _ = reply.send(Ok(json!({ "frameId": "main", "gear": 1 })));
                            return;
                        }
                        sup.active_tab_mut().webkit_synced_url = Some(final_url.clone());
                        if let Some(ref wk) = sup.webkit {
                            let _ = wk.send(WebKitActorCmd::Navigate {
                                url: final_url,
                                reply,
                            });
                            return;
                        }
                    }

                    if let Some(net) = net_opt {
                        if !net.ok || net.status >= 400 {
                            let _ = sup.events_tx.send(BrowserBackendEvent::NetworkError {
                                method: net.method,
                                url: net.url,
                                status: net.status,
                                error: net.error.unwrap_or_else(|| "HTTP error".to_string()),
                            });
                        }
                    }
                    let _ = sup.events_tx.send(BrowserBackendEvent::Lifecycle(
                        "DOMContentLoaded".to_string(),
                    ));
                    let _ = sup
                        .events_tx
                        .send(BrowserBackendEvent::Lifecycle("load".to_string()));
                    let _ = sup
                        .events_tx
                        .send(BrowserBackendEvent::Lifecycle("networkIdle".to_string()));
                    let _ = reply.send(Ok(json!({ "frameId": "main", "gear": 0 })));
                }
                Err(e) => {
                    let _ = sup.events_tx.send(BrowserBackendEvent::NetworkError {
                        method: "GET".to_string(),
                        url,
                        status: 0,
                        error: e.clone(),
                    });
                    let _ = reply.send(Err(e));
                }
            }
        }

        BrowserBackendCmd::Reload {
            ignore_cache,
            reply,
        } => {
            if sup.webkit.is_some() {
                if let Some(ref wk) = sup.webkit {
                    let _ = wk.send(WebKitActorCmd::Reload {
                        ignore_cache,
                        reply,
                    });
                    return;
                }
            }
            let cur = sup.active_tab_mut().page.current_url.clone();
            let res = sup
                .active_tab_mut()
                .page
                .navigate(&cur)
                .await
                .map(|_| json!({ "frameId": "main" }));
            let _ = sup
                .events_tx
                .send(BrowserBackendEvent::Lifecycle("networkIdle".to_string()));
            let _ = reply.send(res);
        }

        BrowserBackendCmd::EvaluateJs { expression, reply } => {
            // Keep lightweight probes ("1 + 1", "document.title", "location.href") in Gear 0 when WebKit is asleep!
            if sup.webkit.is_none()
                && (!sup.can_use_webkit || is_lightweight_probe_expr(&expression))
            {
                let val = sup.active_tab_mut().page.evaluate_expression(&expression);
                let rtype = match &val {
                    Value::String(_) => "string",
                    Value::Number(_) => "number",
                    Value::Bool(_) => "boolean",
                    Value::Null => "undefined",
                    _ => "object",
                };
                let _ = reply.send(Ok(json!({
                    "result": { "type": rtype, "value": val }
                })));
                return;
            }

            if sup.ensure_gear1().await.unwrap_or(false) {
                if let Some(ref wk) = sup.webkit {
                    let _ = wk.send(WebKitActorCmd::EvaluateJs {
                        expression,
                        unwrap_val: false,
                        reply,
                    });
                    return;
                }
            }

            let val = sup.active_tab_mut().page.evaluate_expression(&expression);
            let rtype = match &val {
                Value::String(_) => "string",
                Value::Number(_) => "number",
                Value::Bool(_) => "boolean",
                Value::Null => "undefined",
                _ => "object",
            };
            let _ = reply.send(Ok(json!({
                "result": { "type": rtype, "value": val }
            })));
        }

        BrowserBackendCmd::AddInitScript { source, reply } => {
            sup.init_scripts.push(source.clone());
            if let Some(ref wk) = sup.webkit {
                let _ = wk.send(WebKitActorCmd::AddInitScript { source, reply });
            } else {
                let _ = reply.send(Ok(()));
            }
        }

        BrowserBackendCmd::SetViewport {
            width,
            height,
            reply,
        } => {
            sup.viewport_width = width.max(320);
            sup.viewport_height = height.max(240);
            for tab in sup.tabs.values_mut() {
                tab.page.viewport_width = sup.viewport_width;
                tab.page.viewport_height = sup.viewport_height;
            }
            if let Some(ref wk) = sup.webkit {
                let _ = wk.send(WebKitActorCmd::SetViewport {
                    width: sup.viewport_width,
                    height: sup.viewport_height,
                    reply,
                });
            } else {
                let _ = reply.send(Ok(()));
            }
        }

        BrowserBackendCmd::SetResourceProfile { profile, reply } => {
            sup.resource_profile = profile;
            for tab in sup.tabs.values_mut() {
                tab.page.resource_profile = profile;
            }
            if let Some(ref wk) = sup.webkit {
                let _ = wk.send(WebKitActorCmd::SetResourceProfile { profile, reply });
            } else {
                let _ = reply.send(Ok(()));
            }
        }

        BrowserBackendCmd::WaitForIdle {
            quiet_ms,
            timeout_ms,
            reply,
        } => {
            if sup.webkit.is_some() {
                let expr = format!(
                    "window.__minibrowse ? window.__minibrowse.waitForIdle({quiet_ms}, {timeout_ms}) : {{ idle: true, inflight: 0, waited_ms: 0 }}"
                );
                let res = sup.eval_webkit_unwrapped(expr).await;
                let _ = reply.send(res);
            } else {
                let _ = reply.send(Ok(json!({
                    "idle": true,
                    "inflight": 0,
                    "waited_ms": 0,
                    "gear": 0
                })));
            }
        }

        BrowserBackendCmd::CaptureScreenshotPng { reply } => {
            if sup.ensure_gear1().await.unwrap_or(false) {
                if let Some(ref wk) = sup.webkit {
                    // Ensure images are enabled when capturing a visual screenshot
                    let (rtx, rrx) = oneshot::channel();
                    let _ = wk.send(WebKitActorCmd::SetResourceProfile {
                        profile: ResourceProfile::Full,
                        reply: rtx,
                    });
                    let _ = rrx.await;
                    let _ = wk.send(WebKitActorCmd::CaptureScreenshotPng { reply });
                    return;
                }
            }
            let png = sup.active_tab_mut().page.render_synthetic_png();
            let _ = reply.send(Ok(base64_encode(&png)));
        }

        BrowserBackendCmd::DispatchMouse {
            event_type,
            x,
            y,
            delta_x,
            delta_y,
            reply,
        } => {
            if sup.ensure_gear1().await.unwrap_or(false) {
                let ev_json = serde_json::to_string(&event_type)
                    .unwrap_or_else(|_| "\"mouseMoved\"".to_string());
                let expr = format!(
                    "window.__minibrowse ? window.__minibrowse.dispatchPointer({ev_json}, {x}, {y}, {delta_x}, {delta_y}) : false"
                );
                let _ = sup.eval_webkit_unwrapped(expr).await;
            }
            let _ = reply.send(Ok(()));
        }

        BrowserBackendCmd::DispatchKey {
            event_type,
            key,
            code,
            modifiers,
            reply,
        } => {
            if sup.ensure_gear1().await.unwrap_or(false) {
                let ev_json = serde_json::to_string(&event_type)
                    .unwrap_or_else(|_| "\"keyDown\"".to_string());
                let key_json = serde_json::to_string(&key).unwrap_or_else(|_| "\"\"".to_string());
                let code_json = serde_json::to_string(&code).unwrap_or_else(|_| "\"\"".to_string());
                let expr = format!(
                    "window.__minibrowse ? window.__minibrowse.dispatchKeyboard({ev_json}, {key_json}, {code_json}, {modifiers}) : false"
                );
                let _ = sup.eval_webkit_unwrapped(expr).await;
            }
            let _ = reply.send(Ok(()));
        }

        BrowserBackendCmd::SetMockRoutes { routes, reply } => {
            let count = routes.len();
            sup.mock_routes = routes.clone();
            for tab in sup.tabs.values_mut() {
                tab.page.mock_routes = routes.clone();
            }
            if let Some(ref wk) = sup.webkit {
                let (ptx, prx) = oneshot::channel();
                let _ = wk.send(WebKitActorCmd::SetPolicies {
                    adblock: sup.adblock,
                    mock_routes: sup.mock_routes.clone(),
                    blocked_patterns: sup.blocked_patterns.clone(),
                    reply: ptx,
                });
                let _ = prx.await;
            }
            let _ = reply.send(Ok(count));
        }

        BrowserBackendCmd::SetBlockedPatterns { patterns, reply } => {
            let count = patterns.len();
            sup.blocked_patterns = patterns.clone();
            for tab in sup.tabs.values_mut() {
                tab.page.blocked_patterns = patterns.clone();
            }
            if let Some(ref wk) = sup.webkit {
                let (ptx, prx) = oneshot::channel();
                let _ = wk.send(WebKitActorCmd::SetPolicies {
                    adblock: sup.adblock,
                    mock_routes: sup.mock_routes.clone(),
                    blocked_patterns: sup.blocked_patterns.clone(),
                    reply: ptx,
                });
                let _ = prx.await;
            }
            let _ = reply.send(Ok(count));
        }

        BrowserBackendCmd::SetAdBlock { enabled, reply } => {
            sup.adblock = enabled;
            for tab in sup.tabs.values_mut() {
                tab.page.adblock_enabled = enabled;
            }
            if let Some(ref wk) = sup.webkit {
                let (ptx, prx) = oneshot::channel();
                let _ = wk.send(WebKitActorCmd::SetPolicies {
                    adblock: sup.adblock,
                    mock_routes: sup.mock_routes.clone(),
                    blocked_patterns: sup.blocked_patterns.clone(),
                    reply: ptx,
                });
                let _ = prx.await;
            }
            let _ = reply.send(Ok(enabled));
        }

        BrowserBackendCmd::ResearchExtract {
            query,
            max_chars,
            reply,
        } => {
            // Stay in ~8MB Gear 0 unless Gear 1 is already synced on this page or page needs JS hydration!
            let needs_js = sup.active_tab_mut().page.needs_js_hydration();
            let use_webkit = if sup.webkit.is_some() {
                let cur = sup.active_tab_mut().page.current_url.clone();
                sup.active_tab_mut().webkit_synced_url.as_deref() == Some(cur.as_str()) || needs_js
            } else {
                sup.can_use_webkit && needs_js
            };

            if use_webkit && sup.ensure_gear1().await.unwrap_or(false) {
                let q_json = serde_json::to_string(&query.unwrap_or_default())
                    .unwrap_or_else(|_| "\"\"".to_string());
                let expr = format!("window.__minibrowse.extractResearch({q_json}, {max_chars})");
                let _ = reply.send(sup.eval_webkit_unwrapped(expr).await);
                return;
            }

            let res = sup
                .active_tab_mut()
                .page
                .extract_research(query.as_deref(), max_chars);
            let _ = reply.send(Ok(res));
        }

        BrowserBackendCmd::WebDebugAudit { reply } => {
            if sup.ensure_gear1().await.unwrap_or(false) {
                let res = sup
                    .eval_webkit_unwrapped("window.__minibrowse.auditWebDebug()".to_string())
                    .await;
                let _ = reply.send(res);
                return;
            }
            let _ = reply.send(Ok(sup.active_tab_mut().page.audit_web_debug()));
        }

        BrowserBackendCmd::UiDiff {
            query,
            max_chars,
            reply,
        } => {
            if sup.ensure_gear1().await.unwrap_or(false) {
                let q_json = serde_json::to_string(&query.unwrap_or_default())
                    .unwrap_or_else(|_| "\"\"".to_string());
                let expr = format!("window.__minibrowse.captureUiDiff({q_json}, {max_chars})");
                let _ = reply.send(sup.eval_webkit_unwrapped(expr).await);
                return;
            }
            let _ = reply.send(Ok(sup
                .active_tab_mut()
                .page
                .capture_ui_diff(query.as_deref(), max_chars)));
        }

        BrowserBackendCmd::NetworkWaterfall { reply } => {
            if sup.webkit.is_some() {
                let res = sup
                    .eval_webkit_unwrapped("window.__minibrowse.networkWaterfall()".to_string())
                    .await;
                let _ = reply.send(res);
                return;
            }
            let _ = reply.send(Ok(sup.active_tab_mut().page.network_waterfall()));
        }

        BrowserBackendCmd::StorageInspect { reply } => {
            if sup.webkit.is_some() {
                let res = sup
                    .eval_webkit_unwrapped("window.__minibrowse.inspectStorage()".to_string())
                    .await;
                let _ = reply.send(res);
                return;
            }
            let _ = reply.send(Ok(sup.active_tab_mut().page.inspect_storage()));
        }

        BrowserBackendCmd::CreateBrowserContext { reply } => {
            let cid = format!("mb-ctx-{}", sup.next_ctx_seq);
            sup.next_ctx_seq += 1;
            sup.contexts.insert(cid.clone());
            let _ = reply.send(cid);
        }

        BrowserBackendCmd::DisposeBrowserContext { context_id, reply } => {
            sup.contexts.remove(&context_id);
            let doomed: Vec<String> = sup
                .tabs
                .iter()
                .filter(|(_, tab)| tab.context_id.as_deref() == Some(context_id.as_str()))
                .map(|(tid, _)| tid.clone())
                .collect();
            for tid in &doomed {
                sup.tabs.remove(tid);
                if let Some(ref wk) = sup.webkit {
                    let (ctx, crx) = oneshot::channel();
                    let _ = wk.send(WebKitActorCmd::CloseTab {
                        target_id: tid.clone(),
                        reply: ctx,
                    });
                    let _ = crx.await;
                }
            }
            let _ = sup.active_tab_mut();
            trim_process_heap();
            let _ = reply.send(Ok(doomed.len()));
        }

        BrowserBackendCmd::ListBrowserContexts { reply } => {
            let _ = reply.send(sup.contexts.iter().cloned().collect());
        }

        BrowserBackendCmd::CreateTarget {
            url,
            context_id,
            reply,
        } => {
            let tid = format!("tab-{}", sup.next_tab_seq);
            sup.next_tab_seq += 1;
            let mut page =
                PureRustPageState::new(sup.viewport_width, sup.viewport_height, sup.adblock);
            page.mock_routes = sup.mock_routes.clone();
            page.blocked_patterns = sup.blocked_patterns.clone();
            page.resource_profile = sup.resource_profile;
            if let Some(ref cid) = context_id {
                sup.contexts.insert(cid.clone());
            }
            sup.tabs.insert(
                tid.clone(),
                Gear0Tab {
                    context_id,
                    page,
                    webkit_synced_url: None,
                },
            );
            sup.active_target_id = tid.clone();

            if let Some(u) = url.filter(|s| !s.trim().is_empty() && s.trim() != "about:blank") {
                let (ntx, nrx) = oneshot::channel();
                Box::pin(handle_supervisor_cmd(
                    sup,
                    BrowserBackendCmd::Navigate { url: u, reply: ntx },
                ))
                .await;
                let _ = nrx.await;
            }
            let _ = reply.send(Ok(tid));
        }

        BrowserBackendCmd::ActivateTarget { target_id, reply } => {
            if !sup.tabs.contains_key(&target_id) {
                let _ = reply.send(Err(format!("Unknown targetId '{target_id}'")));
                return;
            }
            sup.active_target_id = target_id.clone();
            if sup.webkit.is_some() {
                let _ = sup.ensure_gear1().await;
            }
            let _ = reply.send(Ok(()));
        }

        BrowserBackendCmd::CloseTarget { target_id, reply } => {
            if sup.tabs.len() <= 1 && sup.tabs.contains_key(&target_id) {
                // Keep at least one tab alive; reset it to about:blank
                let tab = sup.active_tab_mut();
                let _ = tab.page.navigate("about:blank").await;
                tab.webkit_synced_url = None;
                trim_process_heap();
                let _ = reply.send(Ok(true));
                return;
            }
            let removed = sup.tabs.remove(&target_id).is_some();
            if removed {
                if let Some(ref wk) = sup.webkit {
                    let (ctx, crx) = oneshot::channel();
                    let _ = wk.send(WebKitActorCmd::CloseTab {
                        target_id,
                        reply: ctx,
                    });
                    let _ = crx.await;
                }
                let _ = sup.active_tab_mut();
                trim_process_heap();
            }
            let _ = reply.send(Ok(removed));
        }

        BrowserBackendCmd::ListTargets { reply } => {
            let active = sup.active_target_id.clone();
            let list: Vec<Value> = sup
                .tabs
                .iter()
                .map(|(tid, tab)| {
                    json!({
                        "targetId": tid,
                        "type": "page",
                        "title": tab.page.title,
                        "url": tab.page.current_url,
                        "attached": *tid == active,
                        "browserContextId": tab.context_id,
                    })
                })
                .collect();
            let _ = reply.send(list);
        }

        BrowserBackendCmd::SetCookies { cookies, reply } => {
            {
                let tab = sup.active_tab_mut();
                for (k, v) in &cookies {
                    tab.page.cookies.insert(k.clone(), v.clone());
                }
            }
            if sup.webkit.is_some() {
                for (k, v) in cookies {
                    let assign = format!("{k}={v}; path=/");
                    if let Ok(assign_json) = serde_json::to_string(&assign) {
                        let _ = sup
                            .eval_webkit_unwrapped(format!("document.cookie = {assign_json}"))
                            .await;
                    }
                }
            }
            let _ = reply.send(Ok(()));
        }

        BrowserBackendCmd::GetCookies { reply } => {
            if sup.webkit.is_some() {
                let raw = sup
                    .eval_webkit_unwrapped("document.cookie".to_string())
                    .await
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default();
                let parsed: Vec<Value> = raw
                    .split(';')
                    .filter_map(|pair| {
                        let (k, v) = pair.trim().split_once('=')?;
                        (!k.is_empty()).then(|| json!({ "name": k.trim(), "value": v.trim() }))
                    })
                    .collect();
                let _ = reply.send(Ok(parsed));
                return;
            }
            let cookies: Vec<Value> = sup
                .active_tab_mut()
                .page
                .cookies
                .iter()
                .map(|(k, v)| json!({ "name": k, "value": v }))
                .collect();
            let _ = reply.send(Ok(cookies));
        }

        BrowserBackendCmd::GetStatus { reply } => {
            let gear = usize::from(sup.webkit.is_some());
            let engine_name = if sup.webkit.is_some() {
                if sup.opts.headless {
                    "webkit2gtk-offscreen"
                } else {
                    "webkit2gtk-gui"
                }
            } else {
                "pure-rust-dom"
            };

            let (url, title) = if let Some(ref wk) = sup.webkit {
                let (mtx, mrx) = oneshot::channel();
                if wk
                    .send(WebKitActorCmd::GetActiveMeta { reply: mtx })
                    .is_ok()
                {
                    if let Ok((u, t)) = tokio::time::timeout(Duration::from_secs(2), mrx)
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .ok_or(())
                    {
                        (u, t)
                    } else {
                        let tab = sup.active_tab_mut();
                        (tab.page.current_url.clone(), tab.page.title.clone())
                    }
                } else {
                    let tab = sup.active_tab_mut();
                    (tab.page.current_url.clone(), tab.page.title.clone())
                }
            } else {
                let tab = sup.active_tab_mut();
                (tab.page.current_url.clone(), tab.page.title.clone())
            };

            let _ = reply.send(json!({
                "engine": engine_name,
                "gear": gear,
                "active_target": sup.active_target_id,
                "open_tabs": sup.tabs.len(),
                "browser_contexts": sup.contexts.len(),
                "url": url,
                "title": title,
                "viewport": { "width": sup.viewport_width, "height": sup.viewport_height },
                "rss_mb": current_rss_mb(),
            }));
        }

        BrowserBackendCmd::Shutdown => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tier-1 Native WebKit2GTK + Cairo Multi-Tab Actor Thread
// ─────────────────────────────────────────────────────────────────────────────

struct WebKitTabSlot {
    window: *mut c_void,
    webview: *mut c_void,
    ucm: *mut c_void,
}

struct WebKitActorState {
    api: WebKitGtkApi,
    tabs: HashMap<String, WebKitTabSlot>,
    active_target_id: String,
    headless: bool,
    width: u32,
    height: u32,
    adblock: bool,
    resource_profile: ResourceProfile,
    mock_routes: Vec<MockRoute>,
    blocked_patterns: Vec<String>,
    init_scripts: Vec<String>,
    cmd_rx: mpsc::Receiver<WebKitActorCmd>,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
    pending_loads: Vec<oneshot::Sender<Result<Value, String>>>,
    next_eval_token: u64,
    pending_evals: HashMap<u64, (bool, oneshot::Sender<Result<Value, String>>)>,
}

impl WebKitActorState {
    fn active_slot(&self) -> Option<&WebKitTabSlot> {
        self.tabs
            .get(&self.active_target_id)
            .or_else(|| self.tabs.values().next())
    }
}

fn spawn_webkit_thread(
    opts: LaunchOptions,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
) -> Result<WebKitThreadHandle, String> {
    let (init_tx, init_rx) = mpsc::sync_channel::<
        Result<
            (
                usize,
                unsafe extern "C" fn(
                    c_int,
                    crate::ffi::GSourceFunc,
                    *mut c_void,
                    Option<crate::ffi::GDestroyNotify>,
                ) -> u32,
            ),
            String,
        >,
    >(1);
    let (cmd_tx, cmd_rx) = mpsc::channel::<WebKitActorCmd>();

    std::thread::Builder::new()
        .name("minibrowse-gtk".to_string())
        .spawn(move || {
            run_webkit_thread(opts, cmd_rx, events_tx, init_tx);
        })
        .map_err(|e| format!("Failed spawning minibrowse GTK thread: {e}"))?;

    let (state_ptr, wakeup_fn) = init_rx
        .recv()
        .map_err(|_| "WebKit initialization channel closed".to_string())??;

    Ok(WebKitThreadHandle {
        cmd_tx,
        wakeup_fn,
        state_ptr,
    })
}

#[allow(clippy::type_complexity)]
fn run_webkit_thread(
    opts: LaunchOptions,
    cmd_rx: mpsc::Receiver<WebKitActorCmd>,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
    init_tx: mpsc::SyncSender<
        Result<
            (
                usize,
                unsafe extern "C" fn(
                    c_int,
                    crate::ffi::GSourceFunc,
                    *mut c_void,
                    Option<crate::ffi::GDestroyNotify>,
                ) -> u32,
            ),
            String,
        >,
    >,
) {
    tune_allocator_env(opts.headless);

    let api = match WebKitGtkApi::try_load() {
        Ok(a) => a,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };

    unsafe {
        if (api.gtk_init_check)(ptr::null_mut(), ptr::null_mut()) == 0 {
            let _ = init_tx.send(Err("No X11/Wayland display available for GTK".to_string()));
            return;
        }

        // 1. Tighter 96 MiB MemoryPressureSettings (conservative 25%, strict 45% purge)
        let mp = (api.webkit_memory_pressure_settings_new)();
        if !mp.is_null() {
            (api.webkit_memory_pressure_settings_set_memory_limit)(mp, 96);
            (api.webkit_memory_pressure_settings_set_conservative_threshold)(mp, 0.25);
            (api.webkit_memory_pressure_settings_set_strict_threshold)(mp, 0.45);
            (api.webkit_memory_pressure_settings_set_kill_threshold)(mp, 0.85);
            (api.webkit_memory_pressure_settings_set_poll_interval)(mp, 1.5);
            (api.webkit_website_data_manager_set_memory_pressure_settings)(mp);
            (api.webkit_memory_pressure_settings_free)(mp);
        }

        // 2. Single-process DocumentViewer cache diet (zero BFCache / decoded-page retention)
        let web_ctx = (api.webkit_web_context_get_default)();
        if !web_ctx.is_null() {
            (api.webkit_web_context_set_cache_model)(web_ctx, WEBKIT_CACHE_MODEL_DOCUMENT_VIEWER);
            (api.webkit_web_context_set_web_process_count_limit)(web_ctx, 1);
        }

        let wakeup_fn = api.g_idle_add_full;
        let boxed_state = Box::new(WebKitActorState {
            api,
            tabs: HashMap::new(),
            active_target_id: "main".to_string(),
            headless: opts.headless,
            width: opts.width.max(320),
            height: opts.height.max(240),
            adblock: opts.adblock,
            resource_profile: ResourceProfile::Full,
            mock_routes: Vec::new(),
            blocked_patterns: Vec::new(),
            init_scripts: Vec::new(),
            cmd_rx,
            events_tx,
            pending_loads: Vec::new(),
            next_eval_token: 1,
            pending_evals: HashMap::new(),
        });
        let state_ptr = Box::into_raw(boxed_state);

        // Create default "main" tab
        create_webkit_tab_slot(&mut *state_ptr, "main", false);

        let _ = init_tx.send(Ok((state_ptr as usize, wakeup_fn)));
        ((*state_ptr).api.gtk_main)();

        let state_box = Box::from_raw(state_ptr);
        for slot in state_box.tabs.values() {
            (state_box.api.gtk_widget_destroy)(slot.window);
        }
        trim_process_heap();
    }
}

unsafe fn create_webkit_tab_slot(state: &mut WebKitActorState, target_id: &str, ephemeral: bool) {
    if state.tabs.contains_key(target_id) {
        return;
    }

    let ucm = (state.api.webkit_user_content_manager_new)();
    if let Ok(handler_name) = CString::new("minibrowseBridge") {
        let _ = (state
            .api
            .webkit_user_content_manager_register_script_message_handler)(
            ucm,
            handler_name.as_ptr(),
        );
    }
    inject_user_script(&state.api, ucm, BRIDGE_JS);
    for script_src in &state.init_scripts {
        inject_user_script(&state.api, ucm, script_src);
    }

    // Create an ephemeral data-manager WebKitWebView when isolated BrowserContext is requested
    let mut webview: *mut c_void = ptr::null_mut();
    if ephemeral {
        if let (Some(new_ephem_dm), Some(new_ctx_with_dm), Some(get_wv_type), Some(g_obj_new)) = (
            state.api.webkit_website_data_manager_new_ephemeral,
            state.api.webkit_web_context_new_with_website_data_manager,
            state.api.webkit_web_view_get_type,
            state.api.g_object_new,
        ) {
            let dm = new_ephem_dm();
            if !dm.is_null() {
                let ctx = new_ctx_with_dm(dm);
                (state.api.g_object_unref)(dm);
                if !ctx.is_null() {
                    (state.api.webkit_web_context_set_cache_model)(
                        ctx,
                        WEBKIT_CACHE_MODEL_DOCUMENT_VIEWER,
                    );
                    if let (Ok(p_ctx), Ok(p_ucm)) = (
                        CString::new("web-context"),
                        CString::new("user-content-manager"),
                    ) {
                        webview = g_obj_new(
                            get_wv_type(),
                            p_ctx.as_ptr(),
                            ctx,
                            p_ucm.as_ptr(),
                            ucm,
                            ptr::null::<c_char>(),
                        );
                    }
                    (state.api.g_object_unref)(ctx);
                }
            }
        }
    }
    if webview.is_null() {
        webview = (state.api.webkit_web_view_new_with_user_content_manager)(ucm);
    }

    apply_webkit_diet_settings(&state.api, webview, state.headless, state.resource_profile);

    let w_i32 = state.width as c_int;
    let h_i32 = state.height as c_int;
    let window = if state.headless {
        (state.api.gtk_offscreen_window_new)()
    } else {
        let win = (state.api.gtk_window_new)(GTK_WINDOW_TOPLEVEL);
        if let Ok(title) = CString::new("minibrowse — Agentic Browser") {
            (state.api.gtk_window_set_title)(win, title.as_ptr());
        }
        win
    };
    (state.api.gtk_window_set_default_size)(window, w_i32, h_i32);
    (state.api.gtk_widget_set_size_request)(webview, w_i32, h_i32);
    (state.api.gtk_container_add)(window, webview);
    (state.api.gtk_widget_show_all)(window);

    let state_ptr = state as *mut WebKitActorState;
    if let Ok(sig) = CString::new("load-changed") {
        let _ = (state.api.g_signal_connect_data)(
            webview,
            sig.as_ptr(),
            on_webkit_load_changed as *const c_void,
            state_ptr as *mut c_void,
            ptr::null(),
            0,
        );
    }
    if let Ok(sig) = CString::new("script-message-received::minibrowseBridge") {
        let _ = (state.api.g_signal_connect_data)(
            ucm,
            sig.as_ptr(),
            on_script_message_received as *const c_void,
            state_ptr as *mut c_void,
            ptr::null(),
            0,
        );
    }
    if !state.headless && target_id == "main" {
        if let Ok(sig) = CString::new("destroy") {
            let _ = (state.api.g_signal_connect_data)(
                window,
                sig.as_ptr(),
                on_window_destroy as *const c_void,
                state_ptr as *mut c_void,
                ptr::null(),
                0,
            );
        }
    }

    if let Ok(c_blank) = CString::new("about:blank") {
        (state.api.webkit_web_view_load_uri)(webview, c_blank.as_ptr());
    }

    state.tabs.insert(
        target_id.to_string(),
        WebKitTabSlot {
            window,
            webview,
            ucm,
        },
    );
}

unsafe fn apply_webkit_diet_settings(
    api: &WebKitGtkApi,
    webview: *mut c_void,
    headless: bool,
    profile: ResourceProfile,
) {
    let settings = (api.webkit_web_view_get_settings)(webview);
    if settings.is_null() {
        return;
    }
    (api.webkit_settings_set_enable_page_cache)(settings, 0);
    (api.webkit_settings_set_enable_smooth_scrolling)(settings, 0);
    let hw_policy = if headless {
        WEBKIT_HARDWARE_ACCELERATION_POLICY_NEVER
    } else {
        WEBKIT_HARDWARE_ACCELERATION_POLICY_ON_DEMAND
    };
    (api.webkit_settings_set_hardware_acceleration_policy)(settings, hw_policy);

    if let Some(set_images) = api.webkit_settings_set_auto_load_images {
        let load_imgs = c_int::from(profile == ResourceProfile::Full);
        set_images(settings, load_imgs);
    }
    if headless {
        if let Some(set_webgl) = api.webkit_settings_set_enable_webgl {
            set_webgl(settings, 0);
        }
    }
    if let Some(set_webaudio) = api.webkit_settings_set_enable_webaudio {
        set_webaudio(settings, 0);
    }
    if let Some(set_mediastream) = api.webkit_settings_set_enable_media_stream {
        set_mediastream(settings, 0);
    }
    if let Some(set_mediasource) = api.webkit_settings_set_enable_mediasource {
        set_mediasource(settings, 0);
    }
    if let Some(set_ua) = api.webkit_settings_set_user_agent {
        if let Ok(c_ua) = CString::new(CLEAN_BROWSER_USER_AGENT) {
            set_ua(settings, c_ua.as_ptr());
        }
    }
}

unsafe fn inject_user_script(api: &WebKitGtkApi, ucm: *mut c_void, source: &str) {
    if let Ok(c_src) = CString::new(source) {
        let script = (api.webkit_user_script_new)(
            c_src.as_ptr(),
            WEBKIT_USER_CONTENT_INJECTED_ALL_FRAMES,
            WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,
            ptr::null(),
            ptr::null(),
        );
        if !script.is_null() {
            (api.webkit_user_content_manager_add_script)(ucm, script);
            (api.webkit_user_script_unref)(script);
        }
    }
}

unsafe extern "C" fn on_window_destroy(_widget: *mut c_void, user_data: *mut c_void) {
    if !user_data.is_null() {
        let state = &*(user_data as *const WebKitActorState);
        (state.api.gtk_main_quit)();
    }
}

unsafe extern "C" fn on_webkit_load_changed(
    webview: *mut c_void,
    load_event: c_int,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let state = &mut *(user_data as *mut WebKitActorState);
    if load_event == WEBKIT_LOAD_FINISHED {
        sync_policies_into_webview(state, webview);

        for waiter in state.pending_loads.drain(..) {
            let _ = waiter.send(Ok(json!({ "frameId": "main", "gear": 1 })));
        }
        let _ = state.events_tx.send(BrowserBackendEvent::Lifecycle(
            "DOMContentLoaded".to_string(),
        ));
        let _ = state
            .events_tx
            .send(BrowserBackendEvent::Lifecycle("load".to_string()));
        let _ = state
            .events_tx
            .send(BrowserBackendEvent::Lifecycle("networkIdle".to_string()));
        trim_process_heap();
    }
}

unsafe fn sync_policies_into_webview(state: &WebKitActorState, webview: *mut c_void) {
    if webview.is_null() {
        return;
    }
    let routes_json =
        serde_json::to_string(&state.mock_routes).unwrap_or_else(|_| "[]".to_string());
    let blocked_json =
        serde_json::to_string(&state.blocked_patterns).unwrap_or_else(|_| "[]".to_string());
    let adblock_lit = if state.adblock { "true" } else { "false" };
    let sync_js = format!(
        "if (window.__minibrowse) {{ window.__minibrowse.setAdBlock({adblock_lit}); window.__minibrowse.setMockRoutes({routes_json}); window.__minibrowse.setBlockedPatterns({blocked_json}); }}"
    );
    if let Ok(c_js) = CString::new(sync_js) {
        (state.api.webkit_web_view_evaluate_javascript)(
            webview,
            c_js.as_ptr(),
            -1,
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            None,
            ptr::null_mut(),
        );
    }
}

unsafe extern "C" fn on_script_message_received(
    _ucm: *mut c_void,
    js_result: *mut c_void,
    user_data: *mut c_void,
) {
    if user_data.is_null() || js_result.is_null() {
        return;
    }
    let state = &mut *(user_data as *mut WebKitActorState);
    let jsc_val = (state.api.webkit_javascript_result_get_js_value)(js_result);
    if jsc_val.is_null() {
        return;
    }
    let raw_str = (state.api.jsc_value_to_string)(jsc_val);
    if raw_str.is_null() {
        return;
    }
    let msg_text = CStr::from_ptr(raw_str).to_string_lossy().into_owned();
    (state.api.g_free)(raw_str as *mut c_void);

    let Ok(parsed) = serde_json::from_str::<Value>(&msg_text) else {
        return;
    };
    let kind = parsed.get("kind").and_then(Value::as_str).unwrap_or("");
    let payload = parsed.get("payload").cloned().unwrap_or(Value::Null);

    match kind {
        "eval_done" => {
            if let Some(token) = payload.get("token").and_then(Value::as_u64) {
                if let Some((unwrap_val, reply)) = state.pending_evals.remove(&token) {
                    let ok = payload.get("ok").and_then(Value::as_bool).unwrap_or(false);
                    if ok {
                        if unwrap_val {
                            let _ = reply
                                .send(Ok(payload.get("value").cloned().unwrap_or(Value::Null)));
                            return;
                        }
                        let is_undef = payload
                            .get("isUndefined")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let rtype = payload
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("undefined");
                        let cdp_res = if is_undef {
                            json!({
                                "result": {
                                    "type": "undefined",
                                    "description": "undefined"
                                }
                            })
                        } else {
                            json!({
                                "result": {
                                    "type": rtype,
                                    "value": payload.get("value").cloned().unwrap_or(Value::Null)
                                }
                            })
                        };
                        let _ = reply.send(Ok(cdp_res));
                    } else {
                        let err_msg = payload
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("JavaScript execution error");
                        if unwrap_val {
                            let _ = reply.send(Err(err_msg.to_string()));
                        } else {
                            let _ = reply.send(Ok(json!({
                                "result": { "type": "object", "subtype": "error", "description": err_msg },
                                "exceptionDetails": {
                                    "text": err_msg,
                                    "exception": { "type": "object", "subtype": "error", "description": err_msg }
                                }
                            })));
                        }
                    }
                }
            }
        }
        "console" => {
            let level = payload
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("log")
                .to_string();
            let text = payload
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let _ = state
                .events_tx
                .send(BrowserBackendEvent::Console { level, text });
        }
        "network_error" => {
            let method = payload
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_string();
            let url = payload
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let status = payload.get("status").and_then(Value::as_u64).unwrap_or(0) as u16;
            let error = payload
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Network error")
                .to_string();
            let _ = state.events_tx.send(BrowserBackendEvent::NetworkError {
                method,
                url,
                status,
                error,
            });
        }
        _ => {}
    }
}

unsafe extern "C" fn drain_webkit_commands_idle(user_data: *mut c_void) -> c_int {
    if user_data.is_null() {
        return 0;
    }
    let state = &mut *(user_data as *mut WebKitActorState);
    while let Ok(cmd) = state.cmd_rx.try_recv() {
        handle_webkit_command(state, cmd);
    }
    0 // G_SOURCE_REMOVE (one-shot wakeup)
}

unsafe fn handle_webkit_command(state: &mut WebKitActorState, cmd: WebKitActorCmd) {
    match cmd {
        WebKitActorCmd::EnsureTab {
            target_id,
            ephemeral,
            reply,
        } => {
            create_webkit_tab_slot(state, &target_id, ephemeral);
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::ActivateTab { target_id, reply } => {
            if state.tabs.contains_key(&target_id) {
                state.active_target_id = target_id;
                let _ = reply.send(Ok(()));
            } else {
                let _ = reply.send(Err("Tab not found in WebKit".to_string()));
            }
        }
        WebKitActorCmd::CloseTab { target_id, reply } => {
            if let Some(slot) = state.tabs.remove(&target_id) {
                (state.api.gtk_widget_destroy)(slot.window);
                let default_ctx = (state.api.webkit_web_context_get_default)();
                if !default_ctx.is_null() {
                    if let Some(clear_fn) = state.api.webkit_web_context_clear_cache {
                        clear_fn(default_ctx);
                    }
                }
                trim_process_heap();
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::Navigate { url, reply } => {
            let Some(webview) = state.active_slot().map(|s| s.webview) else {
                let _ = reply.send(Err("No active WebKit tab".to_string()));
                return;
            };
            if let Some(rest) = url.strip_prefix("data:text/html,") {
                let decoded = percent_decode_simple(rest);
                if let (Ok(c_html), Ok(c_base)) =
                    (CString::new(decoded), CString::new("http://localhost/"))
                {
                    state.pending_loads.push(reply);
                    (state.api.webkit_web_view_load_html)(
                        webview,
                        c_html.as_ptr(),
                        c_base.as_ptr(),
                    );
                } else {
                    let _ = reply.send(Err("Invalid NUL byte in data URL".to_string()));
                }
            } else if let Ok(c_url) = CString::new(url) {
                state.pending_loads.push(reply);
                (state.api.webkit_web_view_load_uri)(webview, c_url.as_ptr());
            } else {
                let _ = reply.send(Err("Invalid URL containing NUL byte".to_string()));
            }
        }
        WebKitActorCmd::Reload {
            ignore_cache,
            reply,
        } => {
            if let Some(webview) = state.active_slot().map(|s| s.webview) {
                state.pending_loads.push(reply);
                if ignore_cache {
                    (state.api.webkit_web_view_reload_bypass_cache)(webview);
                } else {
                    (state.api.webkit_web_view_reload)(webview);
                }
            } else {
                let _ = reply.send(Err("No active WebKit tab".to_string()));
            }
        }
        WebKitActorCmd::EvaluateJs {
            expression,
            unwrap_val,
            reply,
        } => {
            start_webkit_js_eval_inner(state, &expression, unwrap_val, reply);
        }
        WebKitActorCmd::AddInitScript { source, reply } => {
            state.init_scripts.push(source.clone());
            for slot in state.tabs.values() {
                inject_user_script(&state.api, slot.ucm, &source);
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::SetViewport {
            width,
            height,
            reply,
        } => {
            state.width = width.max(320);
            state.height = height.max(240);
            for slot in state.tabs.values() {
                (state.api.gtk_widget_set_size_request)(
                    slot.webview,
                    state.width as c_int,
                    state.height as c_int,
                );
                (state.api.gtk_window_resize)(
                    slot.window,
                    state.width as c_int,
                    state.height as c_int,
                );
                (state.api.gtk_widget_queue_resize)(slot.webview);
                (state.api.gtk_widget_queue_draw)(slot.webview);
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::SetResourceProfile { profile, reply } => {
            state.resource_profile = profile;
            if let Some(set_images) = state.api.webkit_settings_set_auto_load_images {
                let load_imgs = c_int::from(profile == ResourceProfile::Full);
                for slot in state.tabs.values() {
                    let settings = (state.api.webkit_web_view_get_settings)(slot.webview);
                    if !settings.is_null() {
                        set_images(settings, load_imgs);
                    }
                }
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::CaptureScreenshotPng { reply } => {
            start_webkit_snapshot(state, reply);
        }
        WebKitActorCmd::SetPolicies {
            adblock,
            mock_routes,
            blocked_patterns,
            reply,
        } => {
            state.adblock = adblock;
            state.mock_routes = mock_routes;
            state.blocked_patterns = blocked_patterns;
            let webviews: Vec<*mut c_void> = state.tabs.values().map(|s| s.webview).collect();
            for wv in webviews {
                sync_policies_into_webview(state, wv);
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::GetActiveMeta { reply } => {
            if let Some(webview) = state.active_slot().map(|s| s.webview) {
                let uri_ptr = (state.api.webkit_web_view_get_uri)(webview);
                let uri = if uri_ptr.is_null() {
                    "about:blank".to_string()
                } else {
                    CStr::from_ptr(uri_ptr).to_string_lossy().into_owned()
                };
                let title_ptr = (state.api.webkit_web_view_get_title)(webview);
                let title = if title_ptr.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(title_ptr).to_string_lossy().into_owned()
                };
                let _ = reply.send((uri, title));
            } else {
                let _ = reply.send(("about:blank".to_string(), String::new()));
            }
        }
        WebKitActorCmd::Shutdown => {
            (state.api.gtk_main_quit)();
        }
    }
}

struct EvalFinishCtx {
    state_ptr: *mut WebKitActorState,
    token: u64,
}

unsafe fn start_webkit_js_eval_inner(
    state: &mut WebKitActorState,
    expression: &str,
    unwrap_val: bool,
    reply: oneshot::Sender<Result<Value, String>>,
) {
    let Some(webview) = state.active_slot().map(|s| s.webview) else {
        let _ = reply.send(Err("No active WebKit tab".to_string()));
        return;
    };
    let token = state.next_eval_token;
    state.next_eval_token = state.next_eval_token.wrapping_add(1);

    let wrapped = format!(
        r#"(function(__mb_tok) {{
  var __post = function(ok, rtype, val, isUndef, errText) {{
    if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.minibrowseBridge) {{
      window.webkit.messageHandlers.minibrowseBridge.postMessage(JSON.stringify({{
        kind: "eval_done",
        payload: ok ? {{ token: __mb_tok, ok: true, type: rtype, value: val, isUndefined: isUndef }}
                    : {{ token: __mb_tok, ok: false, error: errText }}
      }}));
      return true;
    }}
    return false;
  }};
  try {{
    var __raw = (0, eval)({expr_json});
    Promise.resolve(__raw).then(function(__v) {{
      __post(true, typeof __v, __v === undefined ? null : __v, __v === undefined, "");
    }}, function(__e) {{
      __post(false, "error", null, false, String((__e && (__e.stack || __e.message)) || __e));
    }});
    return "__mb_async__";
  }} catch (__syncErr) {{
    var __msg = String((__syncErr && (__syncErr.stack || __syncErr.message)) || __syncErr);
    if (!__post(false, "error", null, false, __msg)) {{
      throw __syncErr;
    }}
    return "__mb_async__";
  }}
}})({token})"#,
        expr_json = serde_json::to_string(expression).unwrap_or_else(|_| "\"\"".to_string()),
        token = token
    );

    let Ok(c_script) = CString::new(wrapped) else {
        let _ = reply.send(Err("Expression contains embedded NUL byte".to_string()));
        return;
    };

    state.pending_evals.insert(token, (unwrap_val, reply));
    let ctx_ptr = Box::into_raw(Box::new(EvalFinishCtx {
        state_ptr: state as *mut WebKitActorState,
        token,
    }));

    (state.api.webkit_web_view_evaluate_javascript)(
        webview,
        c_script.as_ptr(),
        -1,
        ptr::null(),
        ptr::null(),
        ptr::null_mut(),
        Some(on_webkit_eval_finished),
        ctx_ptr as *mut c_void,
    );
}

unsafe extern "C" fn on_webkit_eval_finished(
    source: *mut c_void,
    res: *mut c_void,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let ctx = Box::from_raw(user_data as *mut EvalFinishCtx);
    let state = &mut *ctx.state_ptr;
    let mut err: *mut GError = ptr::null_mut();
    let jsc_val = (state.api.webkit_web_view_evaluate_javascript_finish)(source, res, &mut err);

    if !err.is_null() {
        let msg = if (*err).message.is_null() {
            "JavaScript syntax or evaluation error".to_string()
        } else {
            CStr::from_ptr((*err).message)
                .to_string_lossy()
                .into_owned()
        };
        (state.api.g_error_free)(err);
        if let Some((unwrap_val, reply)) = state.pending_evals.remove(&ctx.token) {
            if unwrap_val {
                let _ = reply.send(Err(msg));
            } else {
                let _ = reply.send(Ok(json!({
                    "result": { "type": "object", "subtype": "error", "description": msg },
                    "exceptionDetails": {
                        "text": msg,
                        "exception": { "type": "object", "subtype": "error", "description": msg }
                    }
                })));
            }
        }
        return;
    }

    if !jsc_val.is_null() {
        let raw_c = (state.api.jsc_value_to_string)(jsc_val);
        let s = if raw_c.is_null() {
            String::new()
        } else {
            let text = CStr::from_ptr(raw_c).to_string_lossy().into_owned();
            (state.api.g_free)(raw_c as *mut c_void);
            text
        };
        (state.api.g_object_unref)(jsc_val);

        if s != "__mb_async__" {
            if let Some((unwrap_val, reply)) = state.pending_evals.remove(&ctx.token) {
                if unwrap_val {
                    let _ = reply.send(Ok(Value::String(s)));
                } else {
                    let _ = reply.send(Ok(json!({
                        "result": { "type": "string", "value": s }
                    })));
                }
            }
        }
    }
}

struct SnapshotFinishCtx {
    state_ptr: *mut WebKitActorState,
    webview: *mut c_void,
    reply: Option<oneshot::Sender<Result<String, String>>>,
}

unsafe fn start_webkit_snapshot(
    state: &mut WebKitActorState,
    reply: oneshot::Sender<Result<String, String>>,
) {
    let Some(webview) = state.active_slot().map(|s| s.webview) else {
        let _ = reply.send(Ok(base64_encode(&encode_solid_aura_png(64, 36))));
        return;
    };
    (state.api.gtk_widget_queue_draw)(webview);
    while (state.api.gtk_main_iteration_do)(0) != 0 {}

    let ctx_ptr = Box::into_raw(Box::new(SnapshotFinishCtx {
        state_ptr: state as *mut WebKitActorState,
        webview,
        reply: Some(reply),
    }));

    (state.api.webkit_web_view_get_snapshot)(
        webview,
        WEBKIT_SNAPSHOT_REGION_VISIBLE,
        WEBKIT_SNAPSHOT_OPTIONS_NONE,
        ptr::null_mut(),
        Some(on_webkit_snapshot_ready),
        ctx_ptr as *mut c_void,
    );
}

unsafe extern "C" fn cairo_vec_writer(closure: *mut c_void, data: *const u8, length: u32) -> c_int {
    if closure.is_null() || data.is_null() {
        return 11; // CAIRO_STATUS_WRITE_ERROR
    }
    let buf = &mut *(closure as *mut Vec<u8>);
    let slice = std::slice::from_raw_parts(data, length as usize);
    buf.extend_from_slice(slice);
    CAIRO_STATUS_SUCCESS
}

unsafe extern "C" fn on_webkit_snapshot_ready(
    source: *mut c_void,
    res: *mut c_void,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let mut ctx = Box::from_raw(user_data as *mut SnapshotFinishCtx);
    let state = &mut *ctx.state_ptr;
    let Some(reply) = ctx.reply.take() else {
        return;
    };

    let mut err: *mut GError = ptr::null_mut();
    let surface = (state.api.webkit_web_view_get_snapshot_finish)(source, res, &mut err);
    if !err.is_null() {
        (state.api.g_error_free)(err);
    }

    let mut png_bytes = Vec::with_capacity(32 * 1024);
    if !surface.is_null() {
        (state.api.cairo_surface_flush)(surface);
        let _ = (state.api.cairo_surface_write_to_png_stream)(
            surface,
            cairo_vec_writer,
            (&mut png_bytes as *mut Vec<u8>).cast::<c_void>(),
        );
        (state.api.cairo_surface_destroy)(surface);
    }

    if png_bytes.is_empty() && !ctx.webview.is_null() {
        let w = (state.api.gtk_widget_get_allocated_width)(ctx.webview).max(state.width as c_int);
        let h = (state.api.gtk_widget_get_allocated_height)(ctx.webview).max(state.height as c_int);
        let fallback_surf = (state.api.cairo_image_surface_create)(CAIRO_FORMAT_ARGB32, w, h);
        if !fallback_surf.is_null() {
            let cr = (state.api.cairo_create)(fallback_surf);
            if !cr.is_null() {
                (state.api.gtk_widget_draw)(ctx.webview, cr);
                (state.api.cairo_destroy)(cr);
            }
            (state.api.cairo_surface_flush)(fallback_surf);
            let _ = (state.api.cairo_surface_write_to_png_stream)(
                fallback_surf,
                cairo_vec_writer,
                (&mut png_bytes as *mut Vec<u8>).cast::<c_void>(),
            );
            (state.api.cairo_surface_destroy)(fallback_surf);
        }
    }

    if png_bytes.is_empty() {
        png_bytes = encode_solid_aura_png(64, 36);
    }

    trim_process_heap();
    let _ = reply.send(Ok(base64_encode(&png_bytes)));
}

fn percent_decode_simple(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reads current process Resident Set Size (RSS) in MiB from `/proc/self/statm` on Linux.
#[must_use]
pub fn current_rss_mb() -> f64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(rss_pages_str) = statm.split_whitespace().nth(1) {
                if let Ok(pages) = rss_pages_str.parse::<u64>() {
                    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
                    return ((pages * page_size) as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0;
                }
            }
        }
    }
    0.0
}
