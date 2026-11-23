//! Adaptive Two-Gear Browser Engine (`Gear 0: Pure-Rust Arena DOM` -> `Gear 1: WebKit2GTK + Cairo`)
//! with Ephemeral Multi-Tab / `BrowserContext` Isolation, Resource Profiles, and Auto-Hibernation.
//!
//! - Starts headless sessions in Gear 0 (`~7.3 MB` VmRSS, `0.0%` idle CPU) and handles static/SSR
//!   `browser_research` without ever loading `libwebkit2gtk`.
//! - Automatically escalates to Gear 1 (`webkit2gtk-offscreen`) when a page requires JS/SPA/Shadow DOM
//!   hydration or visual/layout/interactive CDP operations, and hibernates Gear 1 after idle timeout.
//! - Supports isolated tabs & ephemeral `BrowserContext`s inside a single daemon process.

use crate::fallback::{encode_minimal_pdf, encode_solid_aura_png, MockRoute, PureRustPageState};
use crate::ffi::{
    base64_encode, configure_runtime_webgl_env, trim_process_heap, tune_allocator_env_with_webgl,
    GError, WebKitGtkApi, CAIRO_FORMAT_ARGB32, CAIRO_STATUS_SUCCESS, GDK_BUTTON_PRESS_MASK,
    GDK_KEY_PRESS_MASK, GDK_POINTER_MOTION_MASK, GDK_SCROLL_MASK, GDK_WINDOW_STATE_ICONIFIED,
    GDK_WINDOW_STATE_WITHDRAWN, GTK_ALIGN_CENTER, GTK_ALIGN_FILL, GTK_WINDOW_TOPLEVEL,
    G_PRIORITY_DEFAULT_IDLE, WEBKIT_CACHE_MODEL_DOCUMENT_VIEWER,
    WEBKIT_HARDWARE_ACCELERATION_POLICY_NEVER, WEBKIT_HARDWARE_ACCELERATION_POLICY_ON_DEMAND,
    WEBKIT_LOAD_FINISHED, WEBKIT_NETWORK_PROXY_MODE_CUSTOM, WEBKIT_SNAPSHOT_OPTIONS_NONE,
    WEBKIT_SNAPSHOT_REGION_VISIBLE, WEBKIT_USER_CONTENT_INJECTED_ALL_FRAMES,
    WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,
};
use crate::net_policy::{
    build_webkit_content_blocker_json, ResourceProfile, CLEAN_BROWSER_USER_AGENT,
};
use crate::vision::{
    compare_frames, draw_annotations_bgra, draw_pointer_indicator_bgra, encode_bgra_frame_png,
    hamming_distance64, pack_bgra_frame, AnnotationBox, RawFrame,
};
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
    SetDocumentContent {
        html: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    EvaluateJs {
        expression: String,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    AddInitScript {
        source: String,
        reply: oneshot::Sender<Result<String, String>>,
    },
    RemoveInitScript {
        identifier: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetEmulatedMedia {
        media: Option<String>,
        features: Vec<(String, String)>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetViewport {
        width: u32,
        height: u32,
        overridden: bool,
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
    WaitForSelector {
        selector: String,
        state_mode: String,
        timeout_ms: u64,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    CaptureScreenshotPng {
        reply: oneshot::Sender<Result<String, String>>,
    },
    CaptureScreenshotEx {
        if_changed: bool,
        threshold_bits: u32,
        annotate: bool,
        boxes: Vec<AnnotationBox>,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    VisualDiff {
        baseline_name: String,
        save_only: bool,
        reply: oneshot::Sender<Result<Value, String>>,
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
        format: Option<String>,
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
    ApiTraffic {
        filter: Option<String>,
        only_errors: bool,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    StorageInspect {
        reply: oneshot::Sender<Result<Value, String>>,
    },
    ExtractTables {
        selector: Option<String>,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    SetWindowMode {
        gui: bool,
        steal_focus: bool,
        title: Option<String>,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    CreateBrowserContext {
        proxy_server: Option<String>,
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
        background: bool,
        reply: oneshot::Sender<Result<String, String>>,
    },
    OpenOrReuseTab {
        url: String,
        context_id: Option<String>,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    SuspendIdleTabs {
        idle_secs: u64,
        reply: oneshot::Sender<Value>,
    },
    SetMediaDiet {
        enabled: Option<bool>,
        audio_only: Option<bool>,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    SetWebGl {
        enabled: bool,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    FreezeGui {
        freeze: bool,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    TrimMemory {
        hard: bool,
        reply: oneshot::Sender<Value>,
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
    PrintToPdf {
        landscape: bool,
        reply: oneshot::Sender<Result<String, String>>,
    },
    GetHistory {
        reply: oneshot::Sender<Value>,
    },
    NavigateHistoryDelta {
        delta: isize,
        reply: oneshot::Sender<Result<bool, String>>,
    },
    ConfigureDialog {
        accept: bool,
        prompt_text: Option<String>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetFileInputFiles {
        selector: Option<String>,
        specs: Vec<Value>,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    ScopedTarget {
        target_id: String,
        cmd: Box<BrowserBackendCmd>,
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
    pub webgl: bool,
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

#[derive(Debug, Clone, Default)]
struct EmulatedMediaState {
    media: Option<String>,
    features: Vec<(String, String)>,
}

impl EmulatedMediaState {
    fn is_empty(&self) -> bool {
        self.media.as_deref().unwrap_or("").trim().is_empty() && self.features.is_empty()
    }
}

fn wrap_init_script_iife(source: &str) -> String {
    format!("(function(){{\n{source}\n}})();")
}

fn build_emulated_media_js(em: &EmulatedMediaState) -> String {
    let media_lit = serde_json::to_string(em.media.as_deref().unwrap_or(""))
        .unwrap_or_else(|_| "\"\"".to_string());
    let feats_val: Vec<Value> = em
        .features
        .iter()
        .map(|(k, v)| json!({ "name": k, "value": v }))
        .collect();
    let feats_lit = serde_json::to_string(&feats_val).unwrap_or_else(|_| "[]".to_string());
    format!(
        "window.__minibrowse && window.__minibrowse.setEmulatedMedia({media_lit}, {feats_lit});"
    )
}

struct Gear0Tab {
    context_id: Option<String>,
    page: PureRustPageState,
    webkit_synced_url: Option<String>,
    last_webkit_origin: Option<String>,
    last_screenshot_dhash: Option<u64>,
    last_active: std::time::Instant,
    suspended: bool,
    init_scripts: Vec<(String, String)>,
    emulated_media: EmulatedMediaState,
}

#[derive(Debug, Default)]
struct EngineTelemetry {
    nav_count: std::sync::atomic::AtomicU64,
    last_nav_ms: std::sync::atomic::AtomicU64,
    total_nav_ms: std::sync::atomic::AtomicU64,
    eval_count: std::sync::atomic::AtomicU64,
    last_eval_ms: std::sync::atomic::AtomicU64,
    snapshot_count: std::sync::atomic::AtomicU64,
    last_snapshot_ms: std::sync::atomic::AtomicU64,
    cross_origin_arena_resets: std::sync::atomic::AtomicU64,
    gui_freeze_hibernations: std::sync::atomic::AtomicU64,
    gui_frozen: std::sync::atomic::AtomicBool,
}

fn spawn_timed_nav_reply(
    telem: Arc<EngineTelemetry>,
    started: std::time::Instant,
    rx: oneshot::Receiver<Result<Value, String>>,
    reply: oneshot::Sender<Result<Value, String>>,
) {
    tokio::spawn(async move {
        if let Ok(res) = rx.await {
            let ms = started.elapsed().as_millis() as u64;
            telem
                .nav_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            telem
                .last_nav_ms
                .store(ms, std::sync::atomic::Ordering::Relaxed);
            telem
                .total_nav_ms
                .fetch_add(ms, std::sync::atomic::Ordering::Relaxed);
            let _ = reply.send(res);
        }
    });
}

fn spawn_timed_eval_reply(
    telem: Arc<EngineTelemetry>,
    started: std::time::Instant,
    rx: oneshot::Receiver<Result<Value, String>>,
    reply: oneshot::Sender<Result<Value, String>>,
) {
    tokio::spawn(async move {
        if let Ok(res) = rx.await {
            let ms = started.elapsed().as_millis() as u64;
            telem
                .eval_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            telem
                .last_eval_ms
                .store(ms, std::sync::atomic::Ordering::Relaxed);
            let _ = reply.send(res);
        }
    });
}

fn spawn_timed_snap_reply(
    telem: Arc<EngineTelemetry>,
    started: std::time::Instant,
    rx: oneshot::Receiver<Result<String, String>>,
    reply: oneshot::Sender<Result<String, String>>,
) {
    tokio::spawn(async move {
        if let Ok(res) = rx.await {
            let ms = started.elapsed().as_millis() as u64;
            telem
                .snapshot_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            telem
                .last_snapshot_ms
                .store(ms, std::sync::atomic::Ordering::Relaxed);
            let _ = reply.send(res);
        }
    });
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
        proxy_server: Option<String>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetWindowMode {
        gui: bool,
        steal_focus: bool,
        title: Option<String>,
        reply: oneshot::Sender<Result<bool, String>>,
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
    SetDocumentContent {
        html: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    EvaluateJs {
        expression: String,
        unwrap_val: bool,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    AddInitScript {
        target_id: String,
        id: String,
        source: String,
        reply: oneshot::Sender<Result<String, String>>,
    },
    RemoveInitScript {
        target_id: String,
        identifier: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetEmulatedMedia {
        target_id: String,
        media: Option<String>,
        features: Vec<(String, String)>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetViewport {
        width: u32,
        height: u32,
        overridden: bool,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetResourceProfile {
        profile: ResourceProfile,
        reply: oneshot::Sender<Result<(), String>>,
    },
    CaptureScreenshotPng {
        reply: oneshot::Sender<Result<String, String>>,
    },
    CaptureFrameAndPng {
        boxes: Vec<AnnotationBox>,
        pointer: Option<(i32, i32, bool)>,
        reply: oneshot::Sender<Result<(RawFrame, Vec<u8>), String>>,
    },
    RealizeActiveTab {
        reply: oneshot::Sender<Result<(), String>>,
    },
    PrintToPdf {
        landscape: bool,
        reply: oneshot::Sender<Result<String, String>>,
    },
    NavigateHistoryDelta {
        delta: isize,
        reply: oneshot::Sender<Result<bool, String>>,
    },
    SetPolicies {
        adblock: bool,
        mock_routes: Vec<MockRoute>,
        blocked_patterns: Vec<String>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    GetActiveMeta {
        reply: oneshot::Sender<(String, String, bool)>,
    },
    IsTabPlayingAudio {
        target_id: String,
        reply: oneshot::Sender<bool>,
    },
    SetWebGl {
        enabled: bool,
        reply: oneshot::Sender<usize>,
    },
    FreezeGui {
        freeze: bool,
        idle_min_secs: Option<u64>,
        reply: oneshot::Sender<bool>,
    },
    RecycleActiveRenderer {
        reply: oneshot::Sender<bool>,
    },
    TrimMemory {
        recycle_idle_renderer: bool,
        reply: oneshot::Sender<usize>,
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
    context_proxies: HashMap<String, String>,
    active_target_id: String,
    main_target_claimed: bool,
    next_tab_seq: u64,
    next_ctx_seq: u64,
    viewport_width: u32,
    viewport_height: u32,
    viewport_overridden: bool,
    adblock: bool,
    resource_profile: ResourceProfile,
    mock_routes: Vec<MockRoute>,
    blocked_patterns: Vec<String>,
    next_init_script_id: u64,
    visual_baselines: HashMap<String, RawFrame>,
    last_pointer: Option<(i32, i32, bool)>,
    telemetry: Arc<EngineTelemetry>,
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
                        last_webkit_origin: None,
                        last_screenshot_dhash: None,
                        last_active: std::time::Instant::now(),
                        suspended: false,
                        init_scripts: Vec::new(),
                        emulated_media: EmulatedMediaState::default(),
                    },
                );
                self.active_target_id = "main".to_string();
            } else {
                self.active_target_id = fallback_id;
            }
        }
        let id = self.active_target_id.clone();
        let tab = self.tabs.get_mut(&id).unwrap_or_else(|| unreachable!());
        tab.last_active = std::time::Instant::now();
        tab.suspended = false;
        tab
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
                    // Replay per-tab init scripts & emulated media, plus resource profile and network policies into fresh Gear 1
                    for (tid, tab) in &self.tabs {
                        if !tab.emulated_media.is_empty() {
                            let (tx, rx) = oneshot::channel();
                            let _ = wk.send(WebKitActorCmd::SetEmulatedMedia {
                                target_id: tid.clone(),
                                media: tab.emulated_media.media.clone(),
                                features: tab.emulated_media.features.clone(),
                                reply: tx,
                            });
                            let _ = rx.await;
                        }
                        for (sid, src) in &tab.init_scripts {
                            let (tx, rx) = oneshot::channel();
                            let _ = wk.send(WebKitActorCmd::AddInitScript {
                                target_id: tid.clone(),
                                id: sid.clone(),
                                source: src.clone(),
                                reply: tx,
                            });
                            let _ = rx.await;
                        }
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

                    if self.viewport_overridden {
                        let (vtx, vrx) = oneshot::channel();
                        let _ = wk.send(WebKitActorCmd::SetViewport {
                            width: self.viewport_width,
                            height: self.viewport_height,
                            overridden: true,
                            reply: vtx,
                        });
                        let _ = vrx.await;
                    }

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
        let (ephemeral, proxy_server, desired_url, already_synced) = {
            let tab = self.active_tab_mut();
            let cur = tab.page.current_url.clone();
            let synced = tab.webkit_synced_url.as_deref() == Some(cur.as_str());
            let prx = tab.page.proxy_server.clone();
            (tab.context_id.is_some() || prx.is_some(), prx, cur, synced)
        };

        if let Some(ref wk) = self.webkit {
            let (etx, erx) = oneshot::channel();
            wk.send(WebKitActorCmd::EnsureTab {
                target_id: target_id.clone(),
                ephemeral,
                proxy_server,
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

    /// Reclaims idle headless WebKitWebProcess RAM WITHOUT tearing down the single-threaded
    /// GTK3 main loop (re-initializing GTK/WebKit on a fresh OS thread violates GTK thread
    /// affinity and causes CDP channel drops / `Page.navigate` hangs on wake).
    async fn hibernate_gear1(&mut self) {
        if !self.opts.headless {
            return;
        }
        if let Some(ref wk) = self.webkit {
            let (tx, rx) = oneshot::channel();
            if wk
                .send(WebKitActorCmd::TrimMemory {
                    recycle_idle_renderer: true,
                    reply: tx,
                })
                .is_ok()
            {
                let _ = tokio::time::timeout(Duration::from_secs(3), rx).await;
            }
            for tab in self.tabs.values_mut() {
                tab.webkit_synced_url = None;
                tab.suspended = true;
            }
            trim_process_heap();
        }
    }

    /// Suspends idle background Space tabs (`tid != active_target_id` and `tid != "main"`) to 0% CPU
    /// and frees their `WebKitWebView` + Cairo backing surfaces (`agweb` / Chrome Memory-Saver pattern),
    /// while NEVER suspending a background tab that is actively playing audio/video!
    /// Transparently rehydrates on next command via `ensure_gear1()`.
    async fn suspend_idle_background_tabs(&mut self, max_idle: Duration) -> usize {
        let Some(ref wk) = self.webkit else {
            return 0;
        };
        let active_id = self.active_target_id.clone();
        let now = std::time::Instant::now();
        let candidates: Vec<String> = self
            .tabs
            .iter()
            .filter(|(tid, tab)| {
                *tid != "main"
                    && **tid != active_id
                    && !tab.suspended
                    && tab.webkit_synced_url.is_some()
                    && now.duration_since(tab.last_active) >= max_idle
            })
            .map(|(tid, _)| tid.clone())
            .collect();

        let mut suspended_count = 0usize;
        for tid in candidates {
            // Never suspend a tab that is actively playing audio/media for the user!
            let (ptx, prx) = oneshot::channel();
            if wk
                .send(WebKitActorCmd::IsTabPlayingAudio {
                    target_id: tid.clone(),
                    reply: ptx,
                })
                .is_ok()
                && prx.await.unwrap_or(false)
            {
                continue;
            }
            let (ctx, crx) = oneshot::channel();
            if wk
                .send(WebKitActorCmd::CloseTab {
                    target_id: tid.clone(),
                    reply: ctx,
                })
                .is_ok()
            {
                let _ = crx.await;
                if let Some(tab) = self.tabs.get_mut(&tid) {
                    tab.webkit_synced_url = None;
                    tab.suspended = true;
                    suspended_count += 1;
                }
            }
        }
        if suspended_count > 0 {
            trim_process_heap();
        }
        suspended_count
    }

    async fn is_any_tab_playing_audio(&self) -> bool {
        let Some(ref wk) = self.webkit else {
            return false;
        };
        for tid in self.tabs.keys() {
            let (ptx, prx) = oneshot::channel();
            if wk
                .send(WebKitActorCmd::IsTabPlayingAudio {
                    target_id: tid.clone(),
                    reply: ptx,
                })
                .is_ok()
                && prx.await.unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    /// Two-stage process-tree memory governor (`tree_pss_mb` across worker + network + renderer):
    /// - Stage 1 (soft >= 72% of `hard_limit_mb`): clears WebKit caches, prunes `#cinematics` ambient canvases,
    ///   suspends non-playing idle background tabs, and calls `malloc_trim(0)`.
    /// - Stage 2 (hard >= 100% of `hard_limit_mb`): if no tab is actively playing audio and active tab is idle
    ///   (or `force_hard == true`), recycles `WebKitWebProcess` cleanly while preserving tab URLs & cookies.
    async fn enforce_memory_budget(&mut self, hard_limit_mb: f64, force_hard: bool) -> Value {
        let before = sample_process_tree_memory();
        let soft_limit_mb = hard_limit_mb * 0.72;
        if !force_hard && before.tree_pss_mb < soft_limit_mb {
            return json!({
                "trimmed": false,
                "recycled_renderer": false,
                "before_pss_mb": before.tree_pss_mb,
                "after_pss_mb": before.tree_pss_mb,
                "hard_limit_mb": hard_limit_mb,
            });
        }

        let suspended = self
            .suspend_idle_background_tabs(Duration::from_secs(45))
            .await;
        if let Some(ref wk) = self.webkit {
            let (tx, rx) = oneshot::channel();
            let _ = wk.send(WebKitActorCmd::TrimMemory {
                recycle_idle_renderer: false,
                reply: tx,
            });
            let _ = tokio::time::timeout(Duration::from_secs(3), rx).await;
        }
        trim_process_heap();

        let mid = sample_process_tree_memory();
        let mut recycled = false;
        if force_hard || mid.tree_pss_mb > hard_limit_mb {
            let playing_audio = self.is_any_tab_playing_audio().await;
            let active_idle_ok = self
                .tabs
                .get(&self.active_target_id)
                .is_none_or(|t| t.last_active.elapsed() >= Duration::from_secs(20));
            if !playing_audio && (force_hard || active_idle_ok) {
                if let Some(ref wk) = self.webkit {
                    let (tx, rx) = oneshot::channel();
                    if wk
                        .send(WebKitActorCmd::TrimMemory {
                            recycle_idle_renderer: true,
                            reply: tx,
                        })
                        .is_ok()
                    {
                        let count = tokio::time::timeout(Duration::from_secs(3), rx)
                            .await
                            .ok()
                            .and_then(Result::ok)
                            .unwrap_or(0);
                        if count > 0 {
                            recycled = true;
                            for tab in self.tabs.values_mut() {
                                tab.webkit_synced_url = None;
                            }
                        }
                    }
                }
            }
        }
        let after = sample_process_tree_memory();
        json!({
            "trimmed": true,
            "suspended_tabs": suspended,
            "recycled_renderer": recycled,
            "before_pss_mb": before.tree_pss_mb,
            "after_pss_mb": after.tree_pss_mb,
            "after_tree_rss_mb": after.tree_rss_mb,
            "hard_limit_mb": hard_limit_mb,
        })
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

    async fn ensure_active_visual_layout(&self) {
        if let Some(ref wk) = self.webkit {
            let (tx, rx) = oneshot::channel();
            if wk
                .send(WebKitActorCmd::RealizeActiveTab { reply: tx })
                .is_ok()
            {
                let _ = tokio::time::timeout(Duration::from_secs(3), rx).await;
            }
        }
    }

    async fn capture_current_frame(
        &mut self,
        annotate: bool,
        extra_boxes: Vec<AnnotationBox>,
    ) -> Result<(RawFrame, Vec<u8>), String> {
        if self.ensure_gear1().await.unwrap_or(false) {
            let mut boxes = extra_boxes.clone();
            self.ensure_active_visual_layout().await;
            if annotate && boxes.is_empty() {
                if let Ok(val) = self
                    .eval_webkit_unwrapped(
                        "window.__minibrowse ? window.__minibrowse.interactiveBoxes(50) : []"
                            .to_string(),
                    )
                    .await
                {
                    boxes = serde_json::from_value::<Vec<AnnotationBox>>(val).unwrap_or_default();
                }
            }
            if let Some(ref wk) = self.webkit {
                let (rtx, rrx) = oneshot::channel();
                let _ = wk.send(WebKitActorCmd::SetResourceProfile {
                    profile: ResourceProfile::Full,
                    reply: rtx,
                });
                let _ = rrx.await;

                let ptr_overlay = if annotate { self.last_pointer } else { None };
                let (ftx, frx) = oneshot::channel();
                wk.send(WebKitActorCmd::CaptureFrameAndPng {
                    boxes,
                    pointer: ptr_overlay,
                    reply: ftx,
                })?;
                if let Ok(Ok(res)) = tokio::time::timeout(Duration::from_secs(10), frx).await {
                    return res;
                }
            }
        }
        let frame = self
            .active_tab_mut()
            .page
            .render_gear0_frame(annotate, &extra_boxes);
        let png = encode_bgra_frame_png(&frame);
        Ok((frame, png))
    }
}

fn extract_origin_key(raw_url: &str) -> Option<String> {
    let parsed = url::Url::parse(raw_url).ok()?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = parsed.host_str()?;
    let port = parsed.port_or_known_default().unwrap_or(0);
    Some(format!("{scheme}://{host}:{port}"))
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
            | "(window.__minicode_console || []).length"
    ) || t.contains("scrollRestoration = 'manual'")
        || t.contains("const root = document.querySelector('#root, #app, #__next')")
        || t.contains("document.documentElement ? document.documentElement.outerHTML")
        || t.contains("typeof window.__minicode_page_agent")
        || (t.starts_with("(function()") && t.contains("scanInteractables: function()"))
        || t.contains("minicode-simulator-container")
        || t.contains("document.fonts ? document.fonts.ready")
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
        .unwrap_or(300)
        .max(10);
    let gui_freeze_secs = std::env::var("MINIBROWSE_GUI_FREEZE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(45)
        .max(10);

    let mut tabs = BTreeMap::new();
    tabs.insert(
        "main".to_string(),
        Gear0Tab {
            context_id: None,
            page: PureRustPageState::new(opts.width, opts.height, opts.adblock),
            webkit_synced_url: None,
            last_webkit_origin: None,
            last_screenshot_dhash: None,
            last_active: std::time::Instant::now(),
            suspended: false,
            init_scripts: Vec::new(),
            emulated_media: EmulatedMediaState::default(),
        },
    );

    let mut sup = SupervisorState {
        viewport_width: opts.width.max(320),
        viewport_height: opts.height.max(240),
        viewport_overridden: false,
        adblock: opts.adblock,
        opts: opts.clone(),
        can_use_webkit,
        webkit: None,
        backend_kind,
        events_tx,
        tabs,
        contexts: BTreeSet::new(),
        context_proxies: HashMap::new(),
        active_target_id: "main".to_string(),
        main_target_claimed: opts.initial_url.is_some(),
        next_tab_seq: 2,
        next_ctx_seq: 1,
        resource_profile: ResourceProfile::Full,
        mock_routes: Vec::new(),
        blocked_patterns: Vec::new(),
        next_init_script_id: 1,
        visual_baselines: HashMap::new(),
        last_pointer: None,
        telemetry: Arc::new(EngineTelemetry::default()),
    };

    // GUI mode opens the GTK window immediately; headless mode stays in ~4.8MB PSS Gear 0 until needed!
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

    let configured_max_mb = std::env::var("MINIBROWSE_MAX_MEMORY_MB")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|&v| v >= 96.0);

    loop {
        let hard_limit_mb =
            configured_max_mb.unwrap_or(if sup.opts.headless { 320.0 } else { 640.0 });
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
            () = tokio::time::sleep(Duration::from_secs(15)), if sup.webkit.is_some() => {
                if sup.opts.headless {
                    let has_unsuspended = sup.tabs.values().any(|t| !t.suspended);
                    let all_idle = sup
                        .tabs
                        .values()
                        .all(|t| t.last_active.elapsed() >= Duration::from_secs(hibernate_secs));
                    if has_unsuspended && all_idle && !sup.is_any_tab_playing_audio().await {
                        sup.hibernate_gear1().await;
                        continue;
                    }
                } else {
                    // Freeze-Frame Live Snapshot Hibernation for idle GUI windows:
                    // keeps the desktop GtkWindow open & pixel-identical while freeing WebKitWebProcess!
                    let all_agent_idle = sup
                        .tabs
                        .values()
                        .all(|t| t.last_active.elapsed() >= Duration::from_secs(gui_freeze_secs));
                    if all_agent_idle
                        && !sup
                            .telemetry
                            .gui_frozen
                            .load(std::sync::atomic::Ordering::Relaxed)
                        && !sup.is_any_tab_playing_audio().await
                    {
                        if let Some(ref wk) = sup.webkit {
                            let (ftx, frx) = oneshot::channel();
                            if wk
                                .send(WebKitActorCmd::FreezeGui {
                                    freeze: true,
                                    idle_min_secs: Some(gui_freeze_secs),
                                    reply: ftx,
                                })
                                .is_ok()
                                && frx.await.unwrap_or(false)
                            {
                                sup.telemetry
                                    .gui_frozen
                                    .store(true, std::sync::atomic::Ordering::Relaxed);
                                sup.telemetry
                                    .gui_freeze_hibernations
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                for tab in sup.tabs.values_mut() {
                                    if !sup.opts.headless {
                                        tab.webkit_synced_url = None;
                                    }
                                }
                                trim_process_heap();
                            }
                        }
                    }
                }
                let _ = sup.suspend_idle_background_tabs(Duration::from_secs(60)).await;
                let _ = sup.enforce_memory_budget(hard_limit_mb, false).await;
            }
        }
    }
}

async fn handle_supervisor_cmd(sup: &mut SupervisorState, cmd: BrowserBackendCmd) {
    match cmd {
        BrowserBackendCmd::Navigate { url, reply } => {
            let nav_started = std::time::Instant::now();
            sup.telemetry
                .gui_frozen
                .store(false, std::sync::atomic::Ordering::Relaxed);

            // Lightpanda-inspired Cross-Origin Renderer Arena Reset:
            // when crossing origins with a bloated renderer heap (and no audio playing),
            // recycle WebKitWebProcess right before loading the new site so C++ heap fragmentation never accumulates!
            let next_origin = extract_origin_key(&url);
            let prev_origin = sup.active_tab_mut().last_webkit_origin.clone();
            if let (Some(prev), Some(next)) = (&prev_origin, &next_origin) {
                if prev != next && sup.webkit.is_some() {
                    let arena_cap_mb = std::env::var("MINIBROWSE_ARENA_RESET_MB")
                        .ok()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(140.0);
                    let mem = sample_process_tree_memory();
                    if mem.renderer_rss_mb >= arena_cap_mb && !sup.is_any_tab_playing_audio().await
                    {
                        if let Some(ref wk) = sup.webkit {
                            let (rtx, rrx) = oneshot::channel();
                            if wk
                                .send(WebKitActorCmd::RecycleActiveRenderer { reply: rtx })
                                .is_ok()
                                && rrx.await.unwrap_or(false)
                            {
                                sup.telemetry
                                    .cross_origin_arena_resets
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                }
            }

            // If GUI mode, load directly in Gear 1 without blocking the supervisor command loop!
            if !sup.opts.headless && sup.can_use_webkit && sup.ensure_gear1().await.unwrap_or(false)
            {
                {
                    let tab = sup.active_tab_mut();
                    tab.page.current_url = url.clone();
                    tab.webkit_synced_url = Some(url.clone());
                    tab.last_webkit_origin = next_origin;
                }
                if let Some(ref wk) = sup.webkit {
                    let (ntx, nrx) = oneshot::channel();
                    if wk
                        .send(WebKitActorCmd::Navigate {
                            url: url.clone(),
                            reply: ntx,
                        })
                        .is_ok()
                    {
                        spawn_timed_nav_reply(Arc::clone(&sup.telemetry), nav_started, nrx, reply);
                        return;
                    }
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
                        let final_origin = extract_origin_key(&final_url);
                        let already_loaded = sup.active_tab_mut().webkit_synced_url.as_deref()
                            == Some(final_url.as_str());
                        sup.active_tab_mut().last_webkit_origin = final_origin;
                        if already_loaded {
                            let elapsed = nav_started.elapsed().as_millis() as u64;
                            sup.telemetry
                                .nav_count
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            sup.telemetry
                                .last_nav_ms
                                .store(elapsed, std::sync::atomic::Ordering::Relaxed);
                            sup.telemetry
                                .total_nav_ms
                                .fetch_add(elapsed, std::sync::atomic::Ordering::Relaxed);
                            let _ = reply.send(Ok(json!({ "frameId": "main", "gear": 1 })));
                            return;
                        }
                        sup.active_tab_mut().webkit_synced_url = Some(final_url.clone());
                        if let Some(ref wk) = sup.webkit {
                            let (ntx, nrx) = oneshot::channel();
                            if wk
                                .send(WebKitActorCmd::Navigate {
                                    url: final_url,
                                    reply: ntx,
                                })
                                .is_ok()
                            {
                                spawn_timed_nav_reply(
                                    Arc::clone(&sup.telemetry),
                                    nav_started,
                                    nrx,
                                    reply,
                                );
                                return;
                            }
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
                    let elapsed = nav_started.elapsed().as_millis() as u64;
                    sup.telemetry
                        .nav_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    sup.telemetry
                        .last_nav_ms
                        .store(elapsed, std::sync::atomic::Ordering::Relaxed);
                    sup.telemetry
                        .total_nav_ms
                        .fetch_add(elapsed, std::sync::atomic::Ordering::Relaxed);
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
            sup.telemetry
                .gui_frozen
                .store(false, std::sync::atomic::Ordering::Relaxed);
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

        BrowserBackendCmd::SetDocumentContent { html, reply } => {
            sup.telemetry
                .gui_frozen
                .store(false, std::sync::atomic::Ordering::Relaxed);
            let needs_js = {
                let tab = sup.active_tab_mut();
                tab.page.set_document_content(&html);
                tab.webkit_synced_url = None;
                tab.page.needs_js_hydration()
            };
            if sup.can_use_webkit
                && (!sup.opts.headless || sup.webkit.is_some() || needs_js)
                && sup.ensure_gear1().await.unwrap_or(false)
            {
                if let Some(ref wk) = sup.webkit {
                    let _ = wk.send(WebKitActorCmd::SetDocumentContent { html, reply });
                    return;
                }
            }
            let _ = sup.events_tx.send(BrowserBackendEvent::Lifecycle(
                "DOMContentLoaded".to_string(),
            ));
            let _ = sup
                .events_tx
                .send(BrowserBackendEvent::Lifecycle("load".to_string()));
            let _ = reply.send(Ok(()));
        }

        BrowserBackendCmd::EvaluateJs { expression, reply } => {
            let eval_started = std::time::Instant::now();
            // Keep lightweight probes ("1 + 1", "document.title", "outerHTML", PageAgent read probes)
            // in Gear 0 when WebKit is asleep, AND answer pure liveness probes directly in Gear 0
            // when the active tab is hibernated (`suspended`) so pool health-checks never trigger
            // a WebKit reload that times out the 2-second liveness probe.
            let is_liveness_probe = matches!(expression.trim(), "1 + 1" | "true" | "false");
            let active_suspended = sup.active_tab_mut().suspended;
            if (sup.webkit.is_none()
                && (!sup.can_use_webkit || is_lightweight_probe_expr(&expression)))
                || (active_suspended && is_liveness_probe)
            {
                let val = sup.active_tab_mut().page.evaluate_expression(&expression);
                let rtype = match &val {
                    Value::String(_) => "string",
                    Value::Number(_) => "number",
                    Value::Bool(_) => "boolean",
                    Value::Null => "undefined",
                    _ => "object",
                };
                let ms = eval_started.elapsed().as_millis() as u64;
                sup.telemetry
                    .eval_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                sup.telemetry
                    .last_eval_ms
                    .store(ms, std::sync::atomic::Ordering::Relaxed);
                let _ = reply.send(Ok(json!({
                    "result": { "type": rtype, "value": val }
                })));
                return;
            }

            sup.telemetry
                .gui_frozen
                .store(false, std::sync::atomic::Ordering::Relaxed);
            if sup.ensure_gear1().await.unwrap_or(false) {
                if let Some(ref wk) = sup.webkit {
                    let (etx, erx) = oneshot::channel();
                    if wk
                        .send(WebKitActorCmd::EvaluateJs {
                            expression: expression.clone(),
                            unwrap_val: false,
                            reply: etx,
                        })
                        .is_ok()
                    {
                        spawn_timed_eval_reply(
                            Arc::clone(&sup.telemetry),
                            eval_started,
                            erx,
                            reply,
                        );
                        return;
                    }
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
            let ms = eval_started.elapsed().as_millis() as u64;
            sup.telemetry
                .eval_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            sup.telemetry
                .last_eval_ms
                .store(ms, std::sync::atomic::Ordering::Relaxed);
            let _ = reply.send(Ok(json!({
                "result": { "type": rtype, "value": val }
            })));
        }

        BrowserBackendCmd::AddInitScript { source, reply } => {
            let id = sup.next_init_script_id.to_string();
            sup.next_init_script_id += 1;
            let target_id = sup.active_target_id.clone();
            sup.active_tab_mut()
                .init_scripts
                .push((id.clone(), source.clone()));
            if let Some(ref wk) = sup.webkit {
                let _ = wk.send(WebKitActorCmd::AddInitScript {
                    target_id,
                    id,
                    source,
                    reply,
                });
            } else {
                let _ = reply.send(Ok(id));
            }
        }

        BrowserBackendCmd::RemoveInitScript { identifier, reply } => {
            let target_id = sup.active_target_id.clone();
            sup.active_tab_mut()
                .init_scripts
                .retain(|(id, _)| id != &identifier);
            if let Some(ref wk) = sup.webkit {
                let _ = wk.send(WebKitActorCmd::RemoveInitScript {
                    target_id,
                    identifier,
                    reply,
                });
            } else {
                let _ = reply.send(Ok(()));
            }
        }

        BrowserBackendCmd::SetEmulatedMedia {
            media,
            features,
            reply,
        } => {
            let target_id = sup.active_target_id.clone();
            sup.active_tab_mut().emulated_media = EmulatedMediaState {
                media: media.clone(),
                features: features.clone(),
            };
            if let Some(ref wk) = sup.webkit {
                let _ = wk.send(WebKitActorCmd::SetEmulatedMedia {
                    target_id,
                    media,
                    features,
                    reply,
                });
            } else {
                let _ = reply.send(Ok(()));
            }
        }

        BrowserBackendCmd::SetViewport {
            width,
            height,
            overridden,
            reply,
        } => {
            sup.viewport_width = width.max(320);
            sup.viewport_height = height.max(240);
            sup.viewport_overridden = overridden;
            for tab in sup.tabs.values_mut() {
                tab.page.viewport_width = sup.viewport_width;
                tab.page.viewport_height = sup.viewport_height;
            }
            if let Some(ref wk) = sup.webkit {
                let _ = wk.send(WebKitActorCmd::SetViewport {
                    width: sup.viewport_width,
                    height: sup.viewport_height,
                    overridden,
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

        BrowserBackendCmd::WaitForSelector {
            selector,
            state_mode,
            timeout_ms,
            reply,
        } => {
            if sup.webkit.is_some() {
                let sel_json =
                    serde_json::to_string(&selector).unwrap_or_else(|_| "\"\"".to_string());
                let mode_json = serde_json::to_string(&state_mode)
                    .unwrap_or_else(|_| "\"attached\"".to_string());
                let expr = format!(
                    "window.__minibrowse ? window.__minibrowse.waitForSelector({sel_json}, {mode_json}, {timeout_ms}) : {{ matched: false }}"
                );
                let _ = reply.send(sup.eval_webkit_unwrapped(expr).await);
            } else {
                let res = sup
                    .active_tab_mut()
                    .page
                    .wait_for_selector(&selector, &state_mode);
                let _ = reply.send(Ok(res));
            }
        }

        BrowserBackendCmd::CaptureScreenshotPng { reply } => {
            let snap_started = std::time::Instant::now();
            if sup.ensure_gear1().await.unwrap_or(false) {
                sup.ensure_active_visual_layout().await;
                if let Some(ref wk) = sup.webkit {
                    // Ensure images are enabled when capturing a visual screenshot
                    let (rtx, rrx) = oneshot::channel();
                    let _ = wk.send(WebKitActorCmd::SetResourceProfile {
                        profile: ResourceProfile::Full,
                        reply: rtx,
                    });
                    let _ = rrx.await;
                    let (stx, srx) = oneshot::channel();
                    if wk
                        .send(WebKitActorCmd::CaptureScreenshotPng { reply: stx })
                        .is_ok()
                    {
                        spawn_timed_snap_reply(
                            Arc::clone(&sup.telemetry),
                            snap_started,
                            srx,
                            reply,
                        );
                        return;
                    }
                }
            }
            let png = sup.active_tab_mut().page.render_synthetic_png();
            let ms = snap_started.elapsed().as_millis() as u64;
            sup.telemetry
                .snapshot_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            sup.telemetry
                .last_snapshot_ms
                .store(ms, std::sync::atomic::Ordering::Relaxed);
            let _ = reply.send(Ok(base64_encode(&png)));
        }

        BrowserBackendCmd::CaptureScreenshotEx {
            if_changed,
            threshold_bits,
            annotate,
            boxes,
            reply,
        } => match sup.capture_current_frame(annotate, boxes).await {
            Ok((frame, png_bytes)) => {
                let prev_hash = sup.active_tab_mut().last_screenshot_dhash;
                let dist = prev_hash.map(|p| hamming_distance64(p, frame.dhash));
                if if_changed {
                    if let Some(d) = dist {
                        if d <= threshold_bits {
                            let _ = reply.send(Ok(json!({
                                "unchanged": true,
                                "dhash": format!("{:016x}", frame.dhash),
                                "hamming_distance": d,
                            })));
                            return;
                        }
                    }
                }
                sup.active_tab_mut().last_screenshot_dhash = Some(frame.dhash);
                let _ = reply.send(Ok(json!({
                    "unchanged": false,
                    "annotated": annotate,
                    "width": frame.width,
                    "height": frame.height,
                    "dhash": format!("{:016x}", frame.dhash),
                    "hamming_distance": dist,
                    "data": base64_encode(&png_bytes),
                })));
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        },

        BrowserBackendCmd::VisualDiff {
            baseline_name,
            save_only,
            reply,
        } => match sup.capture_current_frame(false, Vec::new()).await {
            Ok((curr_frame, _png)) => {
                let key = if baseline_name.trim().is_empty() {
                    "default".to_string()
                } else {
                    baseline_name
                };
                if save_only {
                    let w = curr_frame.width;
                    let h = curr_frame.height;
                    let dh = curr_frame.dhash;
                    sup.visual_baselines.insert(key.clone(), curr_frame);
                    let _ = reply.send(Ok(json!({
                        "saved": true,
                        "baseline": key,
                        "width": w,
                        "height": h,
                        "dhash": format!("{dh:016x}"),
                    })));
                    return;
                }
                let Some(base_frame) = sup.visual_baselines.get(&key).cloned() else {
                    let w = curr_frame.width;
                    let h = curr_frame.height;
                    let dh = curr_frame.dhash;
                    sup.visual_baselines.insert(key.clone(), curr_frame);
                    let _ = reply.send(Ok(json!({
                        "saved_initial_baseline": true,
                        "baseline": key,
                        "width": w,
                        "height": h,
                        "dhash": format!("{dh:016x}"),
                        "mismatch_pixels": 0,
                        "mismatch_pct": 0.0,
                        "changed_regions": [],
                    })));
                    return;
                };
                let outcome = compare_frames(&base_frame, &curr_frame);
                let _ = reply.send(Ok(json!({
                    "baseline": key,
                    "width": outcome.width,
                    "height": outcome.height,
                    "mismatch_pixels": outcome.mismatch_pixels,
                    "total_pixels": outcome.total_pixels,
                    "mismatch_pct": outcome.mismatch_pct,
                    "dhash_distance": outcome.dhash_distance,
                    "changed_regions": outcome.changed_regions,
                    "diff_png_base64": base64_encode(&outcome.diff_png),
                })));
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        },

        BrowserBackendCmd::DispatchMouse {
            event_type,
            x,
            y,
            delta_x,
            delta_y,
            reply,
        } => {
            let is_click = event_type == "mousePressed" || event_type == "mouseReleased";
            sup.last_pointer = Some((x.round() as i32, y.round() as i32, is_click));
            if sup.ensure_gear1().await.unwrap_or(false) {
                sup.ensure_active_visual_layout().await;
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
            format,
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
                let fmt_json =
                    serde_json::to_string(&format.unwrap_or_else(|| "markdown".to_string()))
                        .unwrap_or_else(|_| "\"markdown\"".to_string());
                let expr = format!(
                    "window.__minibrowse.extractResearch({q_json}, {max_chars}, {fmt_json})"
                );
                let _ = reply.send(sup.eval_webkit_unwrapped(expr).await);
                return;
            }

            let res = sup.active_tab_mut().page.extract_research_with_format(
                query.as_deref(),
                max_chars,
                format.as_deref(),
            );
            let _ = reply.send(Ok(res));
        }

        BrowserBackendCmd::WebDebugAudit { reply } => {
            if sup.ensure_gear1().await.unwrap_or(false) {
                sup.ensure_active_visual_layout().await;
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
                sup.ensure_active_visual_layout().await;
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

        BrowserBackendCmd::ApiTraffic {
            filter,
            only_errors,
            reply,
        } => {
            if sup.webkit.is_some() {
                let f_json = match filter {
                    Some(ref s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
                    None => "null".to_string(),
                };
                let err_lit = if only_errors { "true" } else { "false" };
                let expr = format!(
                    "window.__minibrowse ? window.__minibrowse.apiTraffic({f_json}, {err_lit}) : {{ entries: [] }}"
                );
                let _ = reply.send(sup.eval_webkit_unwrapped(expr).await);
                return;
            }
            let _ = reply.send(Ok(sup
                .active_tab_mut()
                .page
                .api_traffic(filter.as_deref(), only_errors)));
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

        BrowserBackendCmd::ExtractTables { selector, reply } => {
            if sup.webkit.is_some() {
                let sel_json = match selector {
                    Some(ref s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
                    None => "null".to_string(),
                };
                let expr = format!(
                    "window.__minibrowse ? window.__minibrowse.extractTables({sel_json}) : {{ table_count: 0, tables: [] }}"
                );
                let _ = reply.send(sup.eval_webkit_unwrapped(expr).await);
                return;
            }
            let _ = reply.send(Ok(sup
                .active_tab_mut()
                .page
                .extract_tables(selector.as_deref())));
        }

        BrowserBackendCmd::SetWindowMode {
            gui,
            steal_focus,
            title,
            reply,
        } => {
            if gui && !sup.can_use_webkit {
                let _ = reply.send(Ok(json!({
                    "ok": false,
                    "mode": "headless",
                    "gear": 0,
                    "reason": "No X11/Wayland display or WebKit2GTK available for GUI promotion"
                })));
                return;
            }
            sup.opts.headless = !gui;
            if gui {
                let _ = sup.ensure_gear1().await;
            }
            if let Some(ref wk) = sup.webkit {
                let (wtx, wrx) = oneshot::channel();
                if wk
                    .send(WebKitActorCmd::SetWindowMode {
                        gui,
                        steal_focus,
                        title,
                        reply: wtx,
                    })
                    .is_ok()
                {
                    if let Ok(Ok(any_gui_open)) = wrx.await {
                        sup.opts.headless = !any_gui_open;
                        sup.set_kind_label(if any_gui_open {
                            "webkit2gtk-gui"
                        } else {
                            "webkit2gtk-offscreen"
                        });
                        let _ = reply.send(Ok(json!({
                            "ok": true,
                            "mode": if gui { "gui" } else { "headless" },
                            "gear": 1,
                            "any_gui_open": any_gui_open,
                        })));
                        return;
                    }
                }
            }
            let _ = reply.send(Ok(json!({
                "ok": !gui,
                "mode": "headless",
                "gear": usize::from(sup.webkit.is_some()),
            })));
        }

        BrowserBackendCmd::CreateBrowserContext {
            proxy_server,
            reply,
        } => {
            let cid = format!("mb-ctx-{}", sup.next_ctx_seq);
            sup.next_ctx_seq += 1;
            sup.contexts.insert(cid.clone());
            if let Some(p) = proxy_server.filter(|s| !s.trim().is_empty()) {
                sup.context_proxies.insert(cid.clone(), p);
            }
            let _ = reply.send(cid);
        }

        BrowserBackendCmd::DisposeBrowserContext { context_id, reply } => {
            sup.contexts.remove(&context_id);
            sup.context_proxies.remove(&context_id);
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
            background,
            reply,
        } => {
            let can_claim_virgin_main = !sup.main_target_claimed
                && context_id.is_none()
                && sup.tabs.len() == 1
                && sup.tabs.get("main").is_some_and(|t| {
                    t.context_id.is_none()
                        && t.page.current_url == "about:blank"
                        && t.webkit_synced_url.is_none()
                });
            if can_claim_virgin_main {
                sup.main_target_claimed = true;
                sup.active_target_id = "main".to_string();
                if let Some(u) = url.filter(|s| !s.trim().is_empty() && s.trim() != "about:blank") {
                    let (ntx, nrx) = oneshot::channel();
                    Box::pin(handle_supervisor_cmd(
                        sup,
                        BrowserBackendCmd::Navigate { url: u, reply: ntx },
                    ))
                    .await;
                    let _ = nrx.await;
                }
                let _ = reply.send(Ok("main".to_string()));
                return;
            }

            let prev_active = sup.active_target_id.clone();
            let tid = format!("tab-{}", sup.next_tab_seq);
            sup.next_tab_seq += 1;
            let mut page =
                PureRustPageState::new(sup.viewport_width, sup.viewport_height, sup.adblock);
            page.mock_routes = sup.mock_routes.clone();
            page.blocked_patterns = sup.blocked_patterns.clone();
            page.resource_profile = sup.resource_profile;
            if let Some(ref cid) = context_id {
                sup.contexts.insert(cid.clone());
                if let Some(prx) = sup.context_proxies.get(cid).cloned() {
                    page.proxy_server = Some(prx);
                }
            }
            sup.tabs.insert(
                tid.clone(),
                Gear0Tab {
                    context_id,
                    page,
                    webkit_synced_url: None,
                    last_webkit_origin: None,
                    last_screenshot_dhash: None,
                    last_active: std::time::Instant::now(),
                    suspended: false,
                    init_scripts: Vec::new(),
                    emulated_media: EmulatedMediaState::default(),
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
            if background && sup.tabs.contains_key(&prev_active) {
                sup.active_target_id = prev_active.clone();
                if let Some(ref wk) = sup.webkit {
                    let (atx, arx) = oneshot::channel();
                    let _ = wk.send(WebKitActorCmd::ActivateTab {
                        target_id: prev_active,
                        reply: atx,
                    });
                    let _ = arx.await;
                }
            }
            let _ = reply.send(Ok(tid));
        }

        BrowserBackendCmd::OpenOrReuseTab {
            url,
            context_id,
            reply,
        } => {
            let norm = url.trim().trim_end_matches('/');
            let matched_tid = sup
                .tabs
                .iter()
                .find(|(_, tab)| {
                    tab.context_id == context_id
                        && tab.page.current_url.trim().trim_end_matches('/') == norm
                })
                .map(|(tid, _)| (tid.clone(), true))
                .or_else(|| {
                    sup.tabs
                        .iter()
                        .find(|(_, tab)| {
                            tab.context_id == context_id && tab.page.current_url == "about:blank"
                        })
                        .map(|(tid, _)| (tid.clone(), false))
                });

            if let Some((tid, exact_match)) = matched_tid {
                sup.active_target_id = tid.clone();
                if sup.webkit.is_some() {
                    let _ = sup.ensure_gear1().await;
                }
                if !exact_match && !norm.is_empty() && norm != "about:blank" {
                    let (ntx, nrx) = oneshot::channel();
                    Box::pin(handle_supervisor_cmd(
                        sup,
                        BrowserBackendCmd::Navigate {
                            url: url.clone(),
                            reply: ntx,
                        },
                    ))
                    .await;
                    let _ = nrx.await;
                }
                let _ = reply.send(Ok(json!({
                    "targetId": tid,
                    "reused": true,
                    "alreadyAtUrl": exact_match,
                    "url": url,
                })));
                return;
            }

            let is_bg = context_id.is_some();
            let (ctx, crx) = oneshot::channel();
            Box::pin(handle_supervisor_cmd(
                sup,
                BrowserBackendCmd::CreateTarget {
                    url: Some(url.clone()),
                    context_id,
                    background: is_bg,
                    reply: ctx,
                },
            ))
            .await;
            match crx.await {
                Ok(Ok(tid)) => {
                    let _ = reply.send(Ok(json!({
                        "targetId": tid,
                        "reused": false,
                        "alreadyAtUrl": false,
                        "url": url,
                    })));
                }
                Ok(Err(e)) => {
                    let _ = reply.send(Err(e));
                }
                Err(_) => {
                    let _ = reply.send(Err("Failed creating target".to_string()));
                }
            }
        }

        BrowserBackendCmd::SuspendIdleTabs { idle_secs, reply } => {
            let count = sup
                .suspend_idle_background_tabs(Duration::from_secs(idle_secs))
                .await;
            let total_suspended = sup.tabs.values().filter(|t| t.suspended).count();
            let _ = reply.send(json!({
                "suspended_now": count,
                "total_suspended": total_suspended,
                "open_tabs": sup.tabs.len(),
            }));
        }

        BrowserBackendCmd::SetMediaDiet {
            enabled,
            audio_only,
            reply,
        } => {
            if sup.webkit.is_some() {
                let mut opts_obj = serde_json::Map::new();
                if let Some(en) = enabled {
                    opts_obj.insert("enabled".to_string(), json!(en));
                }
                if let Some(ao) = audio_only {
                    opts_obj.insert("audioOnly".to_string(), json!(ao));
                }
                let opts_json = Value::Object(opts_obj).to_string();
                let expr = format!(
                    "window.__minibrowse ? window.__minibrowse.setMediaDiet({opts_json}) : {{ ok: true }}"
                );
                let _ = reply.send(sup.eval_webkit_unwrapped(expr).await);
                return;
            }
            let _ = reply.send(Ok(json!({
                "enabled": enabled.unwrap_or(true),
                "audio_only": audio_only.unwrap_or(false),
                "gear": 0,
            })));
        }

        BrowserBackendCmd::SetWebGl { enabled, reply } => {
            sup.opts.webgl = enabled;
            configure_runtime_webgl_env(enabled);
            let mut recycled = 0usize;
            if let Some(ref wk) = sup.webkit {
                let (tx, rx) = oneshot::channel();
                if wk
                    .send(WebKitActorCmd::SetWebGl { enabled, reply: tx })
                    .is_ok()
                {
                    recycled = tokio::time::timeout(Duration::from_secs(4), rx)
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .unwrap_or(0);
                }
                for tab in sup.tabs.values_mut() {
                    tab.webkit_synced_url = None;
                }
            }
            let _ = reply.send(Ok(json!({
                "webgl_enabled": enabled,
                "render_pipeline": if enabled { "webgl-hw" } else { "2d-zero-llvm" },
                "recycled_renderers": recycled,
            })));
        }

        BrowserBackendCmd::FreezeGui { freeze, reply } => {
            let before = sample_process_tree_memory();
            let mut frozen_now = false;
            if let Some(ref wk) = sup.webkit {
                if freeze && sup.is_any_tab_playing_audio().await {
                    let _ = reply.send(Ok(json!({
                        "frozen": false,
                        "skipped_reason": "audio_playing",
                        "before_pss_mb": before.tree_pss_mb,
                        "after_pss_mb": before.tree_pss_mb,
                    })));
                    return;
                }
                let (ftx, frx) = oneshot::channel();
                if wk
                    .send(WebKitActorCmd::FreezeGui {
                        freeze,
                        idle_min_secs: None,
                        reply: ftx,
                    })
                    .is_ok()
                {
                    frozen_now = tokio::time::timeout(Duration::from_secs(4), frx)
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .unwrap_or(false);
                }
            }
            if freeze && frozen_now {
                sup.telemetry
                    .gui_frozen
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                sup.telemetry
                    .gui_freeze_hibernations
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                for tab in sup.tabs.values_mut() {
                    tab.webkit_synced_url = None;
                }
            } else if !freeze {
                sup.telemetry
                    .gui_frozen
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            trim_process_heap();
            tokio::time::sleep(Duration::from_millis(80)).await;
            let after = sample_process_tree_memory();
            let cur_frozen = sup
                .telemetry
                .gui_frozen
                .load(std::sync::atomic::Ordering::Relaxed);
            let _ = reply.send(Ok(json!({
                "frozen": cur_frozen,
                "changed": frozen_now,
                "before_pss_mb": before.tree_pss_mb,
                "after_pss_mb": after.tree_pss_mb,
                "after_rss_mb": after.tree_rss_mb,
                "process_count": after.process_count,
            })));
        }

        BrowserBackendCmd::TrimMemory { hard, reply } => {
            let default_cap = if sup.opts.headless { 320.0 } else { 640.0 };
            let limit = std::env::var("MINIBROWSE_MAX_MEMORY_MB")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(default_cap);
            let res = sup.enforce_memory_budget(limit, hard).await;
            let _ = reply.send(res);
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
                let is_main = target_id == "main";
                let tab = sup.active_tab_mut();
                let _ = tab.page.navigate("about:blank").await;
                tab.webkit_synced_url = None;
                if is_main {
                    sup.main_target_claimed = false;
                }
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

        BrowserBackendCmd::ScopedTarget { target_id, cmd } => {
            if !sup.tabs.contains_key(&target_id) {
                Box::pin(handle_supervisor_cmd(sup, *cmd)).await;
                return;
            }
            let prev_active = sup.active_target_id.clone();
            let switched = target_id != prev_active;
            let preserve_switch = matches!(
                cmd.as_ref(),
                BrowserBackendCmd::ActivateTarget { .. } | BrowserBackendCmd::CloseTarget { .. }
            );
            if switched {
                sup.active_target_id = target_id;
                if sup.webkit.is_some() {
                    let _ = sup.ensure_gear1().await;
                }
            }
            Box::pin(handle_supervisor_cmd(sup, *cmd)).await;
            if switched && !preserve_switch && sup.tabs.contains_key(&prev_active) {
                sup.active_target_id = prev_active;
                if sup.webkit.is_some() {
                    let _ = sup.ensure_gear1().await;
                }
            }
        }

        BrowserBackendCmd::PrintToPdf { landscape, reply } => {
            if sup.ensure_gear1().await.unwrap_or(false) {
                if let Some(ref wk) = sup.webkit {
                    let (rtx, rrx) = oneshot::channel();
                    let _ = wk.send(WebKitActorCmd::SetResourceProfile {
                        profile: ResourceProfile::Full,
                        reply: rtx,
                    });
                    let _ = rrx.await;
                    let _ = wk.send(WebKitActorCmd::PrintToPdf { landscape, reply });
                    return;
                }
            }
            let pdf_bytes = sup.active_tab_mut().page.render_minimal_pdf(landscape);
            let _ = reply.send(Ok(base64_encode(&pdf_bytes)));
        }

        BrowserBackendCmd::GetHistory { reply } => {
            let _ = reply.send(sup.active_tab_mut().page.navigation_history_value());
        }

        BrowserBackendCmd::NavigateHistoryDelta { delta, reply } => {
            let gear0_moved = sup
                .active_tab_mut()
                .page
                .navigate_history_delta(delta)
                .await
                .unwrap_or(false);
            if let Some(ref wk) = sup.webkit {
                let (htx, hrx) = oneshot::channel();
                let _ = wk.send(WebKitActorCmd::NavigateHistoryDelta { delta, reply: htx });
                if let Ok(Ok(Ok(wk_moved))) =
                    tokio::time::timeout(Duration::from_secs(5), hrx).await
                {
                    let _ = reply.send(Ok(wk_moved || gear0_moved));
                    return;
                }
            }
            let _ = reply.send(Ok(gear0_moved));
        }

        BrowserBackendCmd::ConfigureDialog {
            accept,
            prompt_text,
            reply,
        } => {
            if sup.webkit.is_some() {
                let accept_lit = if accept { "true" } else { "false" };
                let prompt_lit = match prompt_text {
                    Some(t) => serde_json::to_string(&t).unwrap_or_else(|_| "null".to_string()),
                    None => "null".to_string(),
                };
                let _ = sup
                    .eval_webkit_unwrapped(format!(
                        "window.__minibrowse ? window.__minibrowse.configureDialog({accept_lit}, {prompt_lit}) : true"
                    ))
                    .await;
            }
            let _ = reply.send(Ok(()));
        }

        BrowserBackendCmd::SetFileInputFiles {
            selector,
            specs,
            reply,
        } => {
            if sup.ensure_gear1().await.unwrap_or(false) {
                let sel_json = match selector {
                    Some(s) => serde_json::to_string(&s).unwrap_or_else(|_| "null".to_string()),
                    None => "null".to_string(),
                };
                let specs_json = serde_json::to_string(&specs).unwrap_or_else(|_| "[]".to_string());
                let expr = format!(
                    "window.__minibrowse ? window.__minibrowse.setFileInputFiles({sel_json}, {specs_json}) : {{ ok: false }}"
                );
                let _ = reply.send(sup.eval_webkit_unwrapped(expr).await);
                return;
            }
            let _ = reply.send(Ok(json!({ "ok": true, "count": specs.len(), "gear": 0 })));
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

            let fallback_frz = sup
                .telemetry
                .gui_frozen
                .load(std::sync::atomic::Ordering::Relaxed);
            let (url, title, actor_frozen) = if let Some(ref wk) = sup.webkit {
                let (mtx, mrx) = oneshot::channel();
                if wk
                    .send(WebKitActorCmd::GetActiveMeta { reply: mtx })
                    .is_ok()
                {
                    if let Ok((u, t, frz)) = tokio::time::timeout(Duration::from_secs(2), mrx)
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .ok_or(())
                    {
                        (u, t, frz)
                    } else {
                        let tab = sup.active_tab_mut();
                        (
                            tab.page.current_url.clone(),
                            tab.page.title.clone(),
                            fallback_frz,
                        )
                    }
                } else {
                    let tab = sup.active_tab_mut();
                    (
                        tab.page.current_url.clone(),
                        tab.page.title.clone(),
                        fallback_frz,
                    )
                }
            } else {
                let tab = sup.active_tab_mut();
                (tab.page.current_url.clone(), tab.page.title.clone(), false)
            };
            sup.telemetry
                .gui_frozen
                .store(actor_frozen, std::sync::atomic::Ordering::Relaxed);

            let render_pipeline = if sup.webkit.is_none() {
                "gear0-pure-rust"
            } else if !sup.opts.headless && actor_frozen {
                "gui-freeze-frame"
            } else if !sup.opts.headless && sup.opts.webgl {
                "gui-webgl-hw"
            } else if !sup.opts.headless {
                "gui-2d-zero-llvm"
            } else if sup.opts.webgl {
                "headless-webgl"
            } else {
                "headless-2d-zero-llvm"
            };

            let blocked_requests: usize = sup
                .tabs
                .values()
                .map(|t| t.page.network_log.iter().filter(|n| n.blocked).count())
                .sum();
            let est_saved_mb = ((blocked_requests as f64) * 0.18 * 10.0).round() / 10.0;
            let nav_cnt = sup
                .telemetry
                .nav_count
                .load(std::sync::atomic::Ordering::Relaxed);
            let avg_nav_ms = sup
                .telemetry
                .total_nav_ms
                .load(std::sync::atomic::Ordering::Relaxed)
                .checked_div(nav_cnt)
                .unwrap_or(0);

            let mem = sample_process_tree_memory();
            let _ = reply.send(json!({
                "engine": engine_name,
                "gear": gear,
                "render_pipeline": render_pipeline,
                "webgl_enabled": sup.opts.webgl,
                "gui_frozen": actor_frozen,
                "active_target": sup.active_target_id,
                "open_tabs": sup.tabs.len(),
                "browser_contexts": sup.contexts.len(),
                "url": url,
                "title": title,
                "viewport": { "width": sup.viewport_width, "height": sup.viewport_height },
                "rss_mb": mem.tree_rss_mb,
                "pss_mb": mem.tree_pss_mb,
                "worker_rss_mb": mem.worker_rss_mb,
                "renderer_rss_mb": mem.renderer_rss_mb,
                "network_rss_mb": mem.network_rss_mb,
                "tree_threads": mem.tree_threads,
                "process_count": mem.process_count,
                "scopes": {
                    "nav_count": nav_cnt,
                    "last_nav_ms": sup.telemetry.last_nav_ms.load(std::sync::atomic::Ordering::Relaxed),
                    "avg_nav_ms": avg_nav_ms,
                    "eval_count": sup.telemetry.eval_count.load(std::sync::atomic::Ordering::Relaxed),
                    "last_eval_ms": sup.telemetry.last_eval_ms.load(std::sync::atomic::Ordering::Relaxed),
                    "snapshot_count": sup.telemetry.snapshot_count.load(std::sync::atomic::Ordering::Relaxed),
                    "last_snapshot_ms": sup.telemetry.last_snapshot_ms.load(std::sync::atomic::Ordering::Relaxed),
                    "cross_origin_arena_resets": sup.telemetry.cross_origin_arena_resets.load(std::sync::atomic::Ordering::Relaxed),
                    "gui_freeze_hibernations": sup.telemetry.gui_freeze_hibernations.load(std::sync::atomic::Ordering::Relaxed),
                },
                "adblock_stats": {
                    "enabled": sup.adblock,
                    "blocked_requests": blocked_requests,
                    "estimated_saved_mb": est_saved_mb,
                },
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
    visual_realized: bool,
    is_gui_window: bool,
    frozen_drawing_area: *mut c_void,
    frozen_surface: *mut c_void,
    frozen_url: String,
    frozen_title: String,
    init_scripts: Vec<(String, String)>,
    emulated_media: EmulatedMediaState,
}

struct WebKitActorState {
    api: WebKitGtkApi,
    tabs: HashMap<String, WebKitTabSlot>,
    active_target_id: String,
    filter_store: *mut c_void,
    filter_store_dir: Option<std::path::PathBuf>,
    compiled_filter: *mut c_void,
    headless: bool,
    webgl: bool,
    width: u32,
    height: u32,
    viewport_overridden: bool,
    adblock: bool,
    resource_profile: ResourceProfile,
    mock_routes: Vec<MockRoute>,
    blocked_patterns: Vec<String>,
    last_user_input: std::time::Instant,
    cmd_rx: mpsc::Receiver<WebKitActorCmd>,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
    pending_loads: Vec<(*mut c_void, oneshot::Sender<Result<Value, String>>)>,
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
    tune_allocator_env_with_webgl(opts.headless, opts.webgl);

    let api = match WebKitGtkApi::try_load() {
        Ok(a) => a,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };

    unsafe {
        // Scope `__EGL_VENDOR_LIBRARY_FILENAMES=/dev/null` strictly around `gtk_init_check` when `!opts.webgl`
        // so GTK3's UI process never `dlopen`s Mesa `libgallium` + `libLLVM.so` (~36 MB PSS / ~68 MB RSS saved!),
        // then immediately clear it before spawning `WebKitWebProcess` so WebKit's `PlatformDisplay` has valid EGL!
        let restore_egl_env =
            !opts.webgl && std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_none();
        if restore_egl_env {
            std::env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", "/dev/null");
        }
        let gtk_ok = (api.gtk_init_check)(ptr::null_mut(), ptr::null_mut());
        if restore_egl_env {
            std::env::remove_var("__EGL_VENDOR_LIBRARY_FILENAMES");
        }
        if gtk_ok == 0 {
            let _ = init_tx.send(Err("No X11/Wayland display available for GTK".to_string()));
            return;
        }
        let respect_host_dpi = !opts.headless
            && std::env::var("MINIBROWSE_RESPECT_HOST_DPI")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
        if !respect_host_dpi {
            // Lock GTK Xft DPI to exact 96 DPI (96 * 1024 = 98304) in both headless QA and GUI
            // preview so host GNOME/Wayland fractional text-scaling-factor (e.g. 1.1x) never
            // shrinks CSS viewports (preventing 1440x900 -> 1309x818 and 390x844 -> 354x767).
            if let (Some(get_settings), Some(set_long)) = (
                api.gtk_settings_get_default,
                api.gtk_settings_set_long_property,
            ) {
                let gtk_settings = get_settings();
                if !gtk_settings.is_null() {
                    if let (Ok(prop), Ok(origin)) =
                        (CString::new("gtk-xft-dpi"), CString::new("minibrowse"))
                    {
                        set_long(gtk_settings, prop.as_ptr(), 96 * 1024, origin.as_ptr());
                    }
                }
            }
        }
        if !opts.headless {
            install_inline_aura_window_icon(&api);
        }

        // 1. Adaptive MemoryPressureSettings (128 MiB headless; 512 MiB GUI so media/SPAs never GC-thrash)
        let mp = (api.webkit_memory_pressure_settings_new)();
        if !mp.is_null() {
            if opts.headless {
                (api.webkit_memory_pressure_settings_set_memory_limit)(mp, 128);
                (api.webkit_memory_pressure_settings_set_conservative_threshold)(mp, 0.25);
                (api.webkit_memory_pressure_settings_set_strict_threshold)(mp, 0.45);
                (api.webkit_memory_pressure_settings_set_kill_threshold)(mp, 0.90);
            } else {
                (api.webkit_memory_pressure_settings_set_memory_limit)(mp, 512);
                (api.webkit_memory_pressure_settings_set_conservative_threshold)(mp, 0.35);
                (api.webkit_memory_pressure_settings_set_strict_threshold)(mp, 0.60);
                (api.webkit_memory_pressure_settings_set_kill_threshold)(mp, 0.95);
            }
            (api.webkit_memory_pressure_settings_set_poll_interval)(mp, 2.0);
            (api.webkit_website_data_manager_set_memory_pressure_settings)(mp);
            (api.webkit_memory_pressure_settings_free)(mp);
        }

        // 2. DocumentViewer cache diet (zero BFCache / decoded-page retention)
        let web_ctx = (api.webkit_web_context_get_default)();
        if !web_ctx.is_null() {
            (api.webkit_web_context_set_cache_model)(web_ctx, WEBKIT_CACHE_MODEL_DOCUMENT_VIEWER);
        }

        // Sweep orphaned `/tmp/minibrowse-ucf-<dead_pid>` dirs left behind by killed sessions
        cleanup_stale_ucf_dirs();

        let (filter_store, filter_store_dir) =
            if let Some(new_store) = api.webkit_user_content_filter_store_new {
                let store_dir =
                    std::env::temp_dir().join(format!("minibrowse-ucf-{}", std::process::id()));
                let _ = std::fs::create_dir_all(&store_dir);
                let ptr = CString::new(store_dir.to_string_lossy().as_bytes())
                    .ok()
                    .map_or(ptr::null_mut(), |p| new_store(p.as_ptr()));
                (ptr, Some(store_dir))
            } else {
                (ptr::null_mut(), None)
            };

        let wakeup_fn = api.g_idle_add_full;
        let boxed_state = Box::new(WebKitActorState {
            api,
            tabs: HashMap::new(),
            active_target_id: "main".to_string(),
            filter_store,
            filter_store_dir,
            compiled_filter: ptr::null_mut(),
            headless: opts.headless,
            webgl: opts.webgl,
            width: opts.width.max(320),
            height: opts.height.max(240),
            viewport_overridden: false,
            adblock: opts.adblock,
            resource_profile: ResourceProfile::Full,
            mock_routes: Vec::new(),
            blocked_patterns: Vec::new(),
            last_user_input: std::time::Instant::now(),
            cmd_rx,
            events_tx,
            pending_loads: Vec::new(),
            next_eval_token: 1,
            pending_evals: HashMap::new(),
        });
        let state_ptr = Box::into_raw(boxed_state);

        // Create default "main" tab and compile native C++ WebKit content-blocker bytecode
        create_webkit_tab_slot(&mut *state_ptr, "main", false, None);
        refresh_native_content_filters(&mut *state_ptr);

        let _ = init_tx.send(Ok((state_ptr as usize, wakeup_fn)));
        ((*state_ptr).api.gtk_main)();

        let state_box = Box::from_raw(state_ptr);
        for slot in state_box.tabs.values() {
            if !slot.frozen_drawing_area.is_null() && !slot.webview.is_null() {
                (state_box.api.g_object_unref)(slot.webview);
            }
            if !slot.frozen_surface.is_null() {
                (state_box.api.cairo_surface_destroy)(slot.frozen_surface);
            }
            (state_box.api.gtk_widget_destroy)(slot.window);
        }
        if !state_box.compiled_filter.is_null() {
            if let Some(unref_f) = state_box.api.webkit_user_content_filter_unref {
                unref_f(state_box.compiled_filter);
            }
        }
        if !state_box.filter_store.is_null() {
            (state_box.api.g_object_unref)(state_box.filter_store);
        }
        if let Some(ref store_dir) = state_box.filter_store_dir {
            let _ = std::fs::remove_dir_all(store_dir);
        }
        trim_process_heap();
    }
}

/// Removes orphaned `minibrowse-ucf-<pid>` temp directories whose owner process no longer exists.
fn cleanup_stale_ucf_dirs() {
    let tmp = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&tmp) else {
        return;
    };
    let my_pid = std::process::id();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        let Some(pid_str) = name_str.strip_prefix("minibrowse-ucf-") else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if pid == my_pid {
            continue;
        }
        #[cfg(target_os = "linux")]
        let alive = std::path::Path::new(&format!("/proc/{pid}")).exists();
        #[cfg(not(target_os = "linux"))]
        let alive = false;
        if !alive {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Sets a static 16x16 RGBA Aura-purple pixel window icon so GTK never searches desktop SVG themes.
unsafe fn install_inline_aura_window_icon(api: &WebKitGtkApi) {
    static ICON_RGBA: std::sync::OnceLock<[u8; 16 * 16 * 4]> = std::sync::OnceLock::new();
    let pixels = ICON_RGBA.get_or_init(|| {
        let mut buf = [0u8; 16 * 16 * 4];
        for y in 0..16usize {
            for x in 0..16usize {
                let off = (y * 16 + x) * 4;
                let border = x == 0 || x == 15 || y == 0 || y == 15;
                let accent = y < 4 || ((4..=11).contains(&x) && (6..=9).contains(&y));
                let (r, g, b) = if border || accent {
                    (0xA2, 0x77, 0xFF)
                } else {
                    (0x15, 0x14, 0x1B)
                };
                buf[off] = r;
                buf[off + 1] = g;
                buf[off + 2] = b;
                buf[off + 3] = 0xFF;
            }
        }
        buf
    });
    if let (Some(new_pb), Some(set_icon)) = (
        api.gdk_pixbuf_new_from_data,
        api.gtk_window_set_default_icon,
    ) {
        // GDK_COLORSPACE_RGB = 0, has_alpha = 1, bits_per_sample = 8, width = 16, height = 16, rowstride = 64
        let pb = new_pb(
            pixels.as_ptr(),
            0,
            1,
            8,
            16,
            16,
            64,
            ptr::null(),
            ptr::null_mut(),
        );
        if !pb.is_null() {
            set_icon(pb);
            (api.g_object_unref)(pb);
        }
    }
}

unsafe fn build_slot_webview_and_ucm(
    state: &mut WebKitActorState,
    ephemeral: bool,
    proxy_server: Option<&str>,
) -> (*mut c_void, *mut c_void) {
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
    if !state.compiled_filter.is_null() {
        if let Some(add_filter) = state.api.webkit_user_content_manager_add_filter {
            add_filter(ucm, state.compiled_filter);
        }
    }

    let mut webview: *mut c_void = ptr::null_mut();
    if ephemeral || proxy_server.is_some() {
        if let (Some(new_ephem_dm), Some(new_ctx_with_dm), Some(get_wv_type), Some(g_obj_new)) = (
            state.api.webkit_website_data_manager_new_ephemeral,
            state.api.webkit_web_context_new_with_website_data_manager,
            state.api.webkit_web_view_get_type,
            state.api.g_object_new,
        ) {
            let dm = new_ephem_dm();
            if !dm.is_null() {
                if let Some(proxy_uri) = proxy_server.filter(|s| !s.trim().is_empty()) {
                    if let (Some(new_ps), Some(free_ps), Some(set_ps), Ok(c_proxy)) = (
                        state.api.webkit_network_proxy_settings_new,
                        state.api.webkit_network_proxy_settings_free,
                        state
                            .api
                            .webkit_website_data_manager_set_network_proxy_settings,
                        CString::new(proxy_uri.trim()),
                    ) {
                        let ps = new_ps(c_proxy.as_ptr(), ptr::null());
                        if !ps.is_null() {
                            set_ps(dm, WEBKIT_NETWORK_PROXY_MODE_CUSTOM, ps);
                            free_ps(ps);
                        }
                    }
                }
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

    apply_webkit_diet_settings(
        &state.api,
        webview,
        state.headless,
        state.webgl,
        state.resource_profile,
    );

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
    if let Ok(sig) = CString::new("script-dialog") {
        let _ = (state.api.g_signal_connect_data)(
            webview,
            sig.as_ptr(),
            on_webkit_script_dialog as *const c_void,
            state_ptr as *mut c_void,
            ptr::null(),
            0,
        );
    }
    (webview, ucm)
}

unsafe fn attach_gui_window_signals(
    state_ptr: *mut WebKitActorState,
    api: &WebKitGtkApi,
    window: *mut c_void,
    is_main: bool,
) {
    if let Some(add_events) = api.gtk_widget_add_events {
        add_events(
            window,
            GDK_POINTER_MOTION_MASK | GDK_BUTTON_PRESS_MASK | GDK_KEY_PRESS_MASK | GDK_SCROLL_MASK,
        );
    }
    if is_main {
        if let Ok(sig) = CString::new("destroy") {
            let _ = (api.g_signal_connect_data)(
                window,
                sig.as_ptr(),
                on_window_destroy as *const c_void,
                state_ptr as *mut c_void,
                ptr::null(),
                0,
            );
        }
    }
    if let Ok(sig) = CString::new("window-state-event") {
        let _ = (api.g_signal_connect_data)(
            window,
            sig.as_ptr(),
            on_window_state_event as *const c_void,
            state_ptr as *mut c_void,
            ptr::null(),
            0,
        );
    }
    for ev_name in [
        "motion-notify-event",
        "button-press-event",
        "scroll-event",
        "key-press-event",
    ] {
        if let Ok(sig) = CString::new(ev_name) {
            let _ = (api.g_signal_connect_data)(
                window,
                sig.as_ptr(),
                on_window_user_activity as *const c_void,
                state_ptr as *mut c_void,
                ptr::null(),
                0,
            );
        }
    }
}

unsafe fn create_webkit_tab_slot(
    state: &mut WebKitActorState,
    target_id: &str,
    ephemeral: bool,
    proxy_server: Option<&str>,
) {
    if state.tabs.contains_key(target_id) {
        return;
    }

    let w_i32 = state.width as c_int;
    let h_i32 = state.height as c_int;
    // Background isolated Space tabs (`ephemeral == true`) stay in offscreen containers by default
    // so concurrent agent Spaces never pop up intrusive desktop windows or steal user focus!
    let start_gui_win = !state.headless && !ephemeral;
    let defer_pixels = !start_gui_win && state.resource_profile == ResourceProfile::ResearchLean;

    // Map `GtkWindow` with `__EGL_VENDOR_LIBRARY_FILENAMES=/dev/null` first so GTK/GDK's UI process
    // locks onto pure 2D Wayland/X11 shm without loading Mesa `libLLVM.so` (~30 MB PSS saved in UI proc),
    // then clear it before `build_slot_webview_and_ucm` spawns `WebKitWebProcess`!
    let scoped_no_egl =
        !state.webgl && std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_none();
    if scoped_no_egl {
        std::env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", "/dev/null");
    }
    let window = if !start_gui_win {
        (state.api.gtk_offscreen_window_new)()
    } else {
        let win = (state.api.gtk_window_new)(GTK_WINDOW_TOPLEVEL);
        if let Ok(title) = CString::new("minibrowse — Agentic Browser") {
            (state.api.gtk_window_set_title)(win, title.as_ptr());
        }
        win
    };
    if defer_pixels {
        (state.api.gtk_window_set_default_size)(window, 1, 1);
    } else {
        (state.api.gtk_window_set_default_size)(window, w_i32, h_i32);
        (state.api.gtk_widget_show_all)(window);
    }
    if scoped_no_egl {
        std::env::remove_var("__EGL_VENDOR_LIBRARY_FILENAMES");
    }

    let (webview, ucm) = build_slot_webview_and_ucm(state, ephemeral, proxy_server);
    if defer_pixels {
        (state.api.gtk_widget_set_size_request)(webview, 1, 1);
        (state.api.gtk_container_add)(window, webview);
    } else {
        (state.api.gtk_widget_set_size_request)(webview, w_i32, h_i32);
        (state.api.gtk_container_add)(window, webview);
        (state.api.gtk_widget_show_all)(window);
    }

    let state_ptr = state as *mut WebKitActorState;
    if start_gui_win {
        attach_gui_window_signals(state_ptr, &state.api, window, target_id == "main");
    }

    state.tabs.insert(
        target_id.to_string(),
        WebKitTabSlot {
            window,
            webview,
            ucm,
            visual_realized: !defer_pixels,
            is_gui_window: start_gui_win,
            frozen_drawing_area: ptr::null_mut(),
            frozen_surface: ptr::null_mut(),
            frozen_url: String::new(),
            frozen_title: String::new(),
            init_scripts: Vec::new(),
            emulated_media: EmulatedMediaState::default(),
        },
    );
}

/// Freezes an open GUI tab in-place: captures its exact live Cairo viewport frame into a lightweight
/// `GtkDrawingArea` inside the same `GtkWindow`, detaches `slot.webview` (`g_object_ref`), and
/// terminates `WebKitWebProcess` so the user's desktop window stays 100% pixel-identical while
/// renderer memory drops to 0 MB!
unsafe fn freeze_gui_slot(state: &mut WebKitActorState, target_id: &str) -> bool {
    let (Some(new_da), Some(container_remove), Some(g_ref), Some(term_fn)) = (
        state.api.gtk_drawing_area_new,
        state.api.gtk_container_remove,
        state.api.g_object_ref,
        state.api.webkit_web_view_terminate_web_process,
    ) else {
        return false;
    };
    let state_ptr = state as *mut WebKitActorState;
    let w_def = state.width as c_int;
    let h_def = state.height as c_int;
    let Some(slot) = state.tabs.get_mut(target_id) else {
        return false;
    };
    if !slot.is_gui_window || slot.webview.is_null() || !slot.frozen_drawing_area.is_null() {
        return false;
    }

    let uri_ptr = (state.api.webkit_web_view_get_uri)(slot.webview);
    let uri = if uri_ptr.is_null() {
        "about:blank".to_string()
    } else {
        CStr::from_ptr(uri_ptr).to_string_lossy().into_owned()
    };
    let title_ptr = (state.api.webkit_web_view_get_title)(slot.webview);
    let page_title = if title_ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(title_ptr).to_string_lossy().into_owned()
    };

    let alloc_w = (state.api.gtk_widget_get_allocated_width)(slot.webview).max(w_def);
    let alloc_h = (state.api.gtk_widget_get_allocated_height)(slot.webview).max(h_def);
    let surf = (state.api.cairo_image_surface_create)(CAIRO_FORMAT_ARGB32, alloc_w, alloc_h);
    if surf.is_null() {
        return false;
    }
    let cr = (state.api.cairo_create)(surf);
    if !cr.is_null() {
        (state.api.gtk_widget_draw)(slot.webview, cr);
        (state.api.cairo_destroy)(cr);
    }
    (state.api.cairo_surface_flush)(surf);

    let da = new_da();
    if da.is_null() {
        (state.api.cairo_surface_destroy)(surf);
        return false;
    }
    (state.api.gtk_widget_set_size_request)(da, alloc_w, alloc_h);
    if let Some(add_ev) = state.api.gtk_widget_add_events {
        add_ev(
            da,
            GDK_BUTTON_PRESS_MASK | GDK_SCROLL_MASK | GDK_KEY_PRESS_MASK,
        );
    }
    if let Some(set_focus) = state.api.gtk_widget_set_can_focus {
        set_focus(da, 1);
    }
    if let Ok(sig) = CString::new("draw") {
        let _ = (state.api.g_signal_connect_data)(
            da,
            sig.as_ptr(),
            on_freeze_drawing_area_draw as *const c_void,
            state_ptr as *mut c_void,
            ptr::null(),
            0,
        );
    }
    for wake_ev in ["button-press-event", "scroll-event", "key-press-event"] {
        if let Ok(sig) = CString::new(wake_ev) {
            let _ = (state.api.g_signal_connect_data)(
                da,
                sig.as_ptr(),
                on_freeze_user_wake_event as *const c_void,
                state_ptr as *mut c_void,
                ptr::null(),
                0,
            );
        }
    }

    // Hold `slot.webview` alive with 1 explicit ref while detached from `slot.window`,
    // and terminate `WebKitWebProcess` immediately so renderer memory drops to 0 MB!
    g_ref(slot.webview);
    term_fn(slot.webview);
    container_remove(slot.window, slot.webview);
    (state.api.gtk_container_add)(slot.window, da);
    (state.api.gtk_widget_show_all)(da);
    (state.api.gtk_widget_queue_draw)(da);

    let frozen_win_title = if page_title.is_empty() {
        "minibrowse — [Frozen Snapshot: Click or Act to Wake]".to_string()
    } else {
        format!("{page_title} — minibrowse [Frozen: Click to Wake]")
    };
    if let Ok(c_title) = CString::new(frozen_win_title) {
        (state.api.gtk_window_set_title)(slot.window, c_title.as_ptr());
    }

    slot.frozen_drawing_area = da;
    slot.frozen_surface = surf;
    slot.frozen_url = uri;
    slot.frozen_title = page_title;

    let default_ctx = (state.api.webkit_web_context_get_default)();
    if !default_ctx.is_null() {
        if let Some(clear_fn) = state.api.webkit_web_context_clear_cache {
            clear_fn(default_ctx);
        }
    }
    for _ in 0..6 {
        if (state.api.gtk_main_iteration_do)(0) == 0 {
            break;
        }
    }
    trim_process_heap();
    true
}

/// Thaws a frozen GUI tab back into its live `WebKitWebView` inside the same open `GtkWindow`.
unsafe fn ensure_slot_thawed(
    state: &mut WebKitActorState,
    target_id: &str,
    reload_previous: bool,
) -> bool {
    let Some((webview, prev_url, prev_title)) = state.tabs.get(target_id).and_then(|s| {
        (!s.frozen_drawing_area.is_null() && !s.webview.is_null())
            .then(|| (s.webview, s.frozen_url.clone(), s.frozen_title.clone()))
    }) else {
        return false;
    };
    let Some(container_remove) = state.api.gtk_container_remove else {
        return false;
    };

    let w_i32 = state.width as c_int;
    let h_i32 = state.height as c_int;

    if let Some(slot) = state.tabs.get_mut(target_id) {
        container_remove(slot.window, slot.frozen_drawing_area);
        if !slot.frozen_surface.is_null() {
            (state.api.cairo_surface_destroy)(slot.frozen_surface);
        }
        (state.api.gtk_widget_set_size_request)(webview, w_i32, h_i32);
        (state.api.gtk_container_add)(slot.window, webview);
        (state.api.g_object_unref)(webview);
        (state.api.gtk_widget_show_all)(slot.window);

        let restored_title = if prev_title.is_empty() {
            "minibrowse — Agentic Browser".to_string()
        } else {
            format!("{prev_title} — minibrowse")
        };
        if let Ok(c_title) = CString::new(restored_title) {
            (state.api.gtk_window_set_title)(slot.window, c_title.as_ptr());
        }

        slot.frozen_drawing_area = ptr::null_mut();
        slot.frozen_surface = ptr::null_mut();
        slot.visual_realized = true;
    }

    state.last_user_input = std::time::Instant::now();
    if reload_previous && !prev_url.is_empty() && prev_url != "about:blank" {
        if let Some(rest) = prev_url.strip_prefix("data:text/html,") {
            let decoded = percent_decode_simple(rest);
            if let (Ok(c_html), Ok(c_base)) =
                (CString::new(decoded), CString::new("http://localhost/"))
            {
                (state.api.webkit_web_view_load_html)(webview, c_html.as_ptr(), c_base.as_ptr());
            }
        } else if let Ok(c_url) = CString::new(prev_url) {
            (state.api.webkit_web_view_load_uri)(webview, c_url.as_ptr());
        }
    }
    for _ in 0..4 {
        if (state.api.gtk_main_iteration_do)(0) == 0 {
            break;
        }
    }
    true
}

unsafe fn ensure_active_slot_thawed(state: &mut WebKitActorState, reload_previous: bool) {
    let tid = state.active_target_id.clone();
    let _ = ensure_slot_thawed(state, &tid, reload_previous);
}

unsafe extern "C" fn on_freeze_drawing_area_draw(
    widget: *mut c_void,
    cr: *mut c_void,
    user_data: *mut c_void,
) -> c_int {
    if widget.is_null() || cr.is_null() || user_data.is_null() {
        return 0;
    }
    let state = &*(user_data as *const WebKitActorState);
    if let (Some(set_src), Some(paint)) =
        (state.api.cairo_set_source_surface, state.api.cairo_paint)
    {
        for slot in state.tabs.values() {
            if slot.frozen_drawing_area == widget && !slot.frozen_surface.is_null() {
                set_src(cr, slot.frozen_surface, 0.0, 0.0);
                paint(cr);
                return 1;
            }
        }
    }
    0
}

unsafe extern "C" fn on_freeze_user_wake_event(
    widget: *mut c_void,
    _event: *mut c_void,
    user_data: *mut c_void,
) -> c_int {
    if widget.is_null() || user_data.is_null() {
        return 0;
    }
    let state = &mut *(user_data as *mut WebKitActorState);
    let found_tid = state
        .tabs
        .iter()
        .find_map(|(tid, s)| (s.frozen_drawing_area == widget).then(|| tid.clone()));
    if let Some(tid) = found_tid {
        let _ = ensure_slot_thawed(state, &tid, true);
        return 1;
    }
    0
}

unsafe extern "C" fn on_window_user_activity(
    _widget: *mut c_void,
    _event: *mut c_void,
    user_data: *mut c_void,
) -> c_int {
    if !user_data.is_null() {
        let state = &mut *(user_data as *mut WebKitActorState);
        state.last_user_input = std::time::Instant::now();
    }
    0
}

unsafe fn ensure_active_slot_realized(state: &mut WebKitActorState) {
    ensure_active_slot_thawed(state, true);
    let tid = state.active_target_id.clone();
    let w_i32 = state.width as c_int;
    let h_i32 = state.height as c_int;
    if let Some(slot) = state.tabs.get_mut(&tid) {
        if !slot.visual_realized && !slot.webview.is_null() {
            (state.api.gtk_window_set_default_size)(slot.window, w_i32, h_i32);
            (state.api.gtk_widget_set_size_request)(slot.webview, w_i32, h_i32);
            (state.api.gtk_window_resize)(slot.window, w_i32, h_i32);
            (state.api.gtk_widget_show_all)(slot.window);
            (state.api.gtk_widget_queue_resize)(slot.webview);
            (state.api.gtk_widget_queue_draw)(slot.webview);
            for _ in 0..8 {
                if (state.api.gtk_main_iteration_do)(0) == 0 {
                    break;
                }
            }
            slot.visual_realized = true;
        }
    }
}

unsafe fn apply_webkit_diet_settings(
    api: &WebKitGtkApi,
    webview: *mut c_void,
    headless: bool,
    webgl: bool,
    profile: ResourceProfile,
) {
    if webview.is_null() {
        return;
    }
    let settings = (api.webkit_web_view_get_settings)(webview);
    if settings.is_null() {
        return;
    }
    (api.webkit_settings_set_enable_page_cache)(settings, 0);
    let enable_gui_media = c_int::from(!headless);
    let enable_gl = c_int::from(webgl);
    (api.webkit_settings_set_enable_smooth_scrolling)(settings, enable_gui_gl_or_scroll(headless));
    // Only engage GPU/EGL hardware acceleration when `--webgl` / `MiniBrowse.setWebGL(true)` is active;
    // otherwise use `POLICY_NEVER` so `WebKitWebProcess` renders pure 2D Cairo surfaces without
    // requiring an EGL display (`__EGL_VENDOR_LIBRARY_FILENAMES=/dev/null` saves ~80 MB PSS!).
    let hw_policy = if webgl {
        WEBKIT_HARDWARE_ACCELERATION_POLICY_ON_DEMAND
    } else {
        WEBKIT_HARDWARE_ACCELERATION_POLICY_NEVER
    };
    (api.webkit_settings_set_hardware_acceleration_policy)(settings, hw_policy);

    if let Some(set_images) = api.webkit_settings_set_auto_load_images {
        let load_imgs = c_int::from(profile == ResourceProfile::Full);
        set_images(settings, load_imgs);
    }
    if let Some(set_webgl) = api.webkit_settings_set_enable_webgl {
        set_webgl(settings, enable_gl);
    }
    if let Some(set_webaudio) = api.webkit_settings_set_enable_webaudio {
        set_webaudio(settings, enable_gui_media);
    }
    if let Some(set_mediastream) = api.webkit_settings_set_enable_media_stream {
        set_mediastream(settings, 0);
    }
    if let Some(set_mediasource) = api.webkit_settings_set_enable_mediasource {
        set_mediasource(settings, enable_gui_media);
    }
    if let Some(set_gesture) = api.webkit_settings_set_media_playback_requires_user_gesture {
        set_gesture(settings, 1);
    }
    if let Some(set_ua) = api.webkit_settings_set_user_agent {
        if let Ok(c_ua) = CString::new(CLEAN_BROWSER_USER_AGENT) {
            set_ua(settings, c_ua.as_ptr());
        }
    }
    if headless {
        if let Some(set_zoom) = api.webkit_web_view_set_zoom_level {
            set_zoom(webview, 1.0);
        }
    }
}

const fn enable_gui_gl_or_scroll(headless: bool) -> c_int {
    if headless {
        0
    } else {
        1
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

unsafe fn rebuild_slot_user_scripts(api: &WebKitGtkApi, slot: &WebKitTabSlot) {
    if slot.ucm.is_null() {
        return;
    }
    if let Some(remove_all) = api.webkit_user_content_manager_remove_all_scripts {
        remove_all(slot.ucm);
        inject_user_script(api, slot.ucm, BRIDGE_JS);
        if !slot.emulated_media.is_empty() {
            let media_js = build_emulated_media_js(&slot.emulated_media);
            inject_user_script(api, slot.ucm, &media_js);
        }
        for (_, script_src) in &slot.init_scripts {
            let wrapped = wrap_init_script_iife(script_src);
            inject_user_script(api, slot.ucm, &wrapped);
        }
    }
}

unsafe extern "C" fn on_window_destroy(_widget: *mut c_void, user_data: *mut c_void) {
    if !user_data.is_null() {
        let state = &*(user_data as *const WebKitActorState);
        (state.api.gtk_main_quit)();
    }
}

/// Automatically suspends `<video>` upscale painting (while keeping `<audio>` playing!) and trims
/// caches whenever the user minimizes (`ICONIFIED`) or withdraws the GUI window.
unsafe extern "C" fn on_window_state_event(
    widget: *mut c_void,
    _event: *mut c_void,
    user_data: *mut c_void,
) -> c_int {
    if widget.is_null() || user_data.is_null() {
        return 0;
    }
    let state = &mut *(user_data as *mut WebKitActorState);
    let (Some(get_gdk_win), Some(get_state)) = (
        state.api.gtk_widget_get_window,
        state.api.gdk_window_get_state,
    ) else {
        return 0;
    };
    let gdk_win = get_gdk_win(widget);
    if gdk_win.is_null() {
        return 0;
    }
    let flags = get_state(gdk_win);
    let minimized = (flags & (GDK_WINDOW_STATE_ICONIFIED | GDK_WINDOW_STATE_WITHDRAWN)) != 0;
    let lit = if minimized { "true" } else { "false" };
    if let Ok(c_js) = CString::new(format!(
        "if (window.__minibrowse) window.__minibrowse.setWindowMinimized({lit});"
    )) {
        for slot in state.tabs.values() {
            if slot.window == widget && !slot.webview.is_null() {
                (state.api.webkit_web_view_evaluate_javascript)(
                    slot.webview,
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
    }
    if minimized {
        let default_ctx = (state.api.webkit_web_context_get_default)();
        if !default_ctx.is_null() {
            if let Some(clear_fn) = state.api.webkit_web_context_clear_cache {
                clear_fn(default_ctx);
            }
        }
        trim_process_heap();
    }
    0
}

unsafe extern "C" fn on_webkit_script_dialog(
    _webview: *mut c_void,
    _dialog: *mut c_void,
    _user_data: *mut c_void,
) -> c_int {
    // Return TRUE (1) so C-level WebKit script dialogs never block headless execution
    1
}

unsafe fn refresh_native_content_filters(state: &mut WebKitActorState) {
    if state.filter_store.is_null() {
        return;
    }
    let Some(remove_all) = state.api.webkit_user_content_manager_remove_all_filters else {
        return;
    };
    for slot in state.tabs.values() {
        if !slot.ucm.is_null() {
            remove_all(slot.ucm);
        }
    }
    if !state.compiled_filter.is_null() {
        if let Some(unref_f) = state.api.webkit_user_content_filter_unref {
            unref_f(state.compiled_filter);
        }
        state.compiled_filter = ptr::null_mut();
    }

    let Some(rules_json) = build_webkit_content_blocker_json(state.adblock, state.resource_profile)
    else {
        return;
    };
    let (Some(g_bytes_new), Some(g_bytes_unref), Some(store_save)) = (
        state.api.g_bytes_new,
        state.api.g_bytes_unref,
        state.api.webkit_user_content_filter_store_save,
    ) else {
        return;
    };

    let bytes_ptr = g_bytes_new(rules_json.as_ptr().cast::<c_void>(), rules_json.len());
    if bytes_ptr.is_null() {
        return;
    }
    if let Ok(filter_id) = CString::new("minibrowse-net-policy") {
        let state_ptr = state as *mut WebKitActorState;
        store_save(
            state.filter_store,
            filter_id.as_ptr(),
            bytes_ptr,
            ptr::null_mut(),
            Some(on_content_filter_compiled),
            state_ptr.cast::<c_void>(),
        );
    }
    g_bytes_unref(bytes_ptr);
}

unsafe extern "C" fn on_content_filter_compiled(
    source: *mut c_void,
    res: *mut c_void,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let state = &mut *(user_data as *mut WebKitActorState);
    let Some(save_finish) = state.api.webkit_user_content_filter_store_save_finish else {
        return;
    };
    let mut err: *mut GError = ptr::null_mut();
    let filter = save_finish(source, res, &mut err);
    if !err.is_null() {
        (state.api.g_error_free)(err);
        return;
    }
    if filter.is_null() {
        return;
    }
    if !state.compiled_filter.is_null() {
        if let Some(unref_f) = state.api.webkit_user_content_filter_unref {
            unref_f(state.compiled_filter);
        }
    }
    state.compiled_filter = filter;
    if let Some(add_filter) = state.api.webkit_user_content_manager_add_filter {
        for slot in state.tabs.values() {
            if !slot.ucm.is_null() {
                add_filter(slot.ucm, filter);
            }
        }
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

        let mut remaining = Vec::with_capacity(state.pending_loads.len());
        for (wv, waiter) in state.pending_loads.drain(..) {
            if wv == webview {
                let _ = waiter.send(Ok(json!({ "frameId": "main", "gear": 1 })));
            } else {
                remaining.push((wv, waiter));
            }
        }
        state.pending_loads = remaining;
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
            proxy_server,
            reply,
        } => {
            create_webkit_tab_slot(state, &target_id, ephemeral, proxy_server.as_deref());
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::SetWindowMode {
            gui,
            steal_focus,
            title,
            reply,
        } => {
            ensure_active_slot_thawed(state, true);
            let state_ptr = state as *mut WebKitActorState;
            let tid = state.active_target_id.clone();
            let w_i32 = state.width as c_int;
            let h_i32 = state.height as c_int;
            if gui {
                install_inline_aura_window_icon(&state.api);
            }
            if let Some(slot) = state.tabs.get_mut(&tid) {
                if gui != slot.is_gui_window && !slot.webview.is_null() {
                    if let (Some(g_ref), Some(container_remove)) =
                        (state.api.g_object_ref, state.api.gtk_container_remove)
                    {
                        g_ref(slot.webview);
                        container_remove(slot.window, slot.webview);
                        let new_win = if gui {
                            let win = (state.api.gtk_window_new)(GTK_WINDOW_TOPLEVEL);
                            let win_title = title
                                .as_deref()
                                .unwrap_or("minibrowse — Agentic Browser (Live Session)");
                            if let Ok(c_title) = CString::new(win_title) {
                                (state.api.gtk_window_set_title)(win, c_title.as_ptr());
                            }
                            if !steal_focus {
                                if let Some(set_focus_map) = state.api.gtk_window_set_focus_on_map {
                                    set_focus_map(win, 0);
                                }
                            }
                            win
                        } else {
                            (state.api.gtk_offscreen_window_new)()
                        };
                        (state.api.gtk_window_set_default_size)(new_win, w_i32, h_i32);
                        (state.api.gtk_widget_set_size_request)(slot.webview, w_i32, h_i32);
                        (state.api.gtk_container_add)(new_win, slot.webview);
                        (state.api.g_object_unref)(slot.webview);
                        (state.api.gtk_widget_destroy)(slot.window);
                        apply_webkit_diet_settings(
                            &state.api,
                            slot.webview,
                            !gui,
                            state.webgl,
                            state.resource_profile,
                        );
                        if gui {
                            attach_gui_window_signals(
                                state_ptr,
                                &state.api,
                                new_win,
                                tid == "main",
                            );
                        }
                        (state.api.gtk_widget_show_all)(new_win);
                        if gui && steal_focus {
                            if let Some(present) = state.api.gtk_window_present {
                                present(new_win);
                            }
                        }
                        slot.window = new_win;
                        slot.is_gui_window = gui;
                        slot.visual_realized = true;
                    }
                } else if gui {
                    if let Some(c_title) = title.and_then(|t| CString::new(t).ok()) {
                        (state.api.gtk_window_set_title)(slot.window, c_title.as_ptr());
                    }
                    (state.api.gtk_widget_show_all)(slot.window);
                    if steal_focus {
                        if let Some(present) = state.api.gtk_window_present {
                            present(slot.window);
                        }
                    }
                }
                for _ in 0..4 {
                    if (state.api.gtk_main_iteration_do)(0) == 0 {
                        break;
                    }
                }
            }
            let any_gui = state.tabs.values().any(|s| s.is_gui_window);
            state.headless = !any_gui;
            let _ = reply.send(Ok(any_gui));
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
                if !slot.frozen_drawing_area.is_null() && !slot.webview.is_null() {
                    (state.api.g_object_unref)(slot.webview);
                }
                if !slot.frozen_surface.is_null() {
                    (state.api.cairo_surface_destroy)(slot.frozen_surface);
                }
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
            ensure_active_slot_thawed(state, false);
            let Some(webview) = state
                .active_slot()
                .map(|s| s.webview)
                .filter(|w| !w.is_null())
            else {
                let _ = reply.send(Err("No active WebKit tab".to_string()));
                return;
            };
            if let Some(rest) = url.strip_prefix("data:text/html,") {
                let decoded = percent_decode_simple(rest);
                if let (Ok(c_html), Ok(c_base)) =
                    (CString::new(decoded), CString::new("http://localhost/"))
                {
                    state.pending_loads.push((webview, reply));
                    (state.api.webkit_web_view_load_html)(
                        webview,
                        c_html.as_ptr(),
                        c_base.as_ptr(),
                    );
                } else {
                    let _ = reply.send(Err("Invalid NUL byte in data URL".to_string()));
                }
            } else if let Ok(c_url) = CString::new(url) {
                state.pending_loads.push((webview, reply));
                (state.api.webkit_web_view_load_uri)(webview, c_url.as_ptr());
            } else {
                let _ = reply.send(Err("Invalid URL containing NUL byte".to_string()));
            }
        }
        WebKitActorCmd::SetDocumentContent { html, reply } => {
            ensure_active_slot_thawed(state, false);
            let Some(webview) = state
                .active_slot()
                .map(|s| s.webview)
                .filter(|w| !w.is_null())
            else {
                let _ = reply.send(Err("No active WebKit tab".to_string()));
                return;
            };
            if let (Ok(c_html), Ok(c_base)) =
                (CString::new(html), CString::new("http://localhost/"))
            {
                let (ltx, _lrx) = oneshot::channel();
                state.pending_loads.push((webview, ltx));
                (state.api.webkit_web_view_load_html)(webview, c_html.as_ptr(), c_base.as_ptr());
                let _ = reply.send(Ok(()));
            } else {
                let _ = reply.send(Err("HTML contains embedded NUL byte".to_string()));
            }
        }
        WebKitActorCmd::Reload {
            ignore_cache,
            reply,
        } => {
            ensure_active_slot_thawed(state, true);
            if let Some(webview) = state
                .active_slot()
                .map(|s| s.webview)
                .filter(|w| !w.is_null())
            {
                state.pending_loads.push((webview, reply));
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
            ensure_active_slot_thawed(state, true);
            start_webkit_js_eval_inner(state, &expression, unwrap_val, reply);
        }
        WebKitActorCmd::AddInitScript {
            target_id,
            id,
            source,
            reply,
        } => {
            let tid = if state.tabs.contains_key(&target_id) {
                target_id
            } else {
                state.active_target_id.clone()
            };
            if let Some(slot) = state.tabs.get_mut(&tid) {
                slot.init_scripts.push((id.clone(), source.clone()));
                if !slot.ucm.is_null() {
                    let wrapped = wrap_init_script_iife(&source);
                    inject_user_script(&state.api, slot.ucm, &wrapped);
                }
            }
            let _ = reply.send(Ok(id));
        }
        WebKitActorCmd::RemoveInitScript {
            target_id,
            identifier,
            reply,
        } => {
            let tid = if state.tabs.contains_key(&target_id) {
                target_id
            } else {
                state.active_target_id.clone()
            };
            if let Some(slot) = state.tabs.get_mut(&tid) {
                slot.init_scripts.retain(|(sid, _)| sid != &identifier);
                rebuild_slot_user_scripts(&state.api, slot);
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::SetEmulatedMedia {
            target_id,
            media,
            features,
            reply,
        } => {
            let tid = if state.tabs.contains_key(&target_id) {
                target_id
            } else {
                state.active_target_id.clone()
            };
            let live_js = {
                if let Some(slot) = state.tabs.get_mut(&tid) {
                    slot.emulated_media = EmulatedMediaState { media, features };
                    rebuild_slot_user_scripts(&state.api, slot);
                    Some((slot.webview, build_emulated_media_js(&slot.emulated_media)))
                } else {
                    None
                }
            };
            if let Some((webview, js)) = live_js {
                if !webview.is_null() {
                    if let Ok(c_js) = CString::new(js) {
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
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::SetViewport {
            width,
            height,
            overridden,
            reply,
        } => {
            state.width = width.max(320);
            state.height = height.max(240);
            state.viewport_overridden = overridden;
            let w_i32 = state.width as c_int;
            let h_i32 = state.height as c_int;
            let gui_align = if overridden {
                GTK_ALIGN_CENTER
            } else {
                GTK_ALIGN_FILL
            };
            for slot in state.tabs.values() {
                (state.api.gtk_window_set_default_size)(slot.window, w_i32, h_i32);
                if !slot.webview.is_null() {
                    if slot.is_gui_window {
                        if let Some(set_halign) = state.api.gtk_widget_set_halign {
                            set_halign(slot.webview, gui_align);
                        }
                        if let Some(set_valign) = state.api.gtk_widget_set_valign {
                            set_valign(slot.webview, gui_align);
                        }
                    }
                    (state.api.gtk_widget_set_size_request)(slot.webview, w_i32, h_i32);
                    (state.api.gtk_widget_queue_resize)(slot.webview);
                    (state.api.gtk_widget_queue_draw)(slot.webview);
                } else if !slot.frozen_drawing_area.is_null() {
                    (state.api.gtk_widget_set_size_request)(slot.frozen_drawing_area, w_i32, h_i32);
                }
                (state.api.gtk_window_resize)(slot.window, w_i32, h_i32);
                (state.api.gtk_widget_queue_resize)(slot.window);
            }
            // Pump GTK layout allocation ticks synchronously so `window.innerWidth` / `innerHeight`
            // reflect the new viewport dimensions before returning to the CDP caller.
            for _ in 0..16 {
                if (state.api.gtk_main_iteration_do)(0) == 0 {
                    break;
                }
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::SetResourceProfile { profile, reply } => {
            let changed = state.resource_profile != profile;
            state.resource_profile = profile;
            if let Some(set_images) = state.api.webkit_settings_set_auto_load_images {
                let load_imgs = c_int::from(profile == ResourceProfile::Full);
                for slot in state.tabs.values() {
                    if !slot.webview.is_null() {
                        let settings = (state.api.webkit_web_view_get_settings)(slot.webview);
                        if !settings.is_null() {
                            set_images(settings, load_imgs);
                        }
                    }
                }
            }
            if changed {
                refresh_native_content_filters(state);
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::RealizeActiveTab { reply } => {
            ensure_active_slot_realized(state);
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::CaptureScreenshotPng { reply } => {
            ensure_active_slot_realized(state);
            start_webkit_snapshot(state, reply);
        }
        WebKitActorCmd::CaptureFrameAndPng {
            boxes,
            pointer,
            reply,
        } => {
            ensure_active_slot_realized(state);
            start_webkit_frame_capture(state, boxes, pointer, reply);
        }
        WebKitActorCmd::PrintToPdf { landscape, reply } => {
            ensure_active_slot_realized(state);
            start_webkit_pdf_export(state, landscape, reply);
        }
        WebKitActorCmd::NavigateHistoryDelta { delta, reply } => {
            ensure_active_slot_thawed(state, true);
            if let Some(webview) = state
                .active_slot()
                .map(|s| s.webview)
                .filter(|w| !w.is_null())
            {
                if delta == -1 {
                    if let (Some(can_back), Some(go_back)) = (
                        state.api.webkit_web_view_can_go_back,
                        state.api.webkit_web_view_go_back,
                    ) {
                        if can_back(webview) != 0 {
                            go_back(webview);
                            let _ = reply.send(Ok(true));
                            return;
                        }
                    }
                } else if delta == 1 {
                    if let (Some(can_fwd), Some(go_fwd)) = (
                        state.api.webkit_web_view_can_go_forward,
                        state.api.webkit_web_view_go_forward,
                    ) {
                        if can_fwd(webview) != 0 {
                            go_fwd(webview);
                            let _ = reply.send(Ok(true));
                            return;
                        }
                    }
                }
                let js = format!("window.history.go({delta}); true");
                let (tx, _rx) = oneshot::channel();
                start_webkit_js_eval_inner(state, &js, true, tx);
                let _ = reply.send(Ok(true));
            } else {
                let _ = reply.send(Ok(false));
            }
        }
        WebKitActorCmd::SetPolicies {
            adblock,
            mock_routes,
            blocked_patterns,
            reply,
        } => {
            let adblock_changed = state.adblock != adblock;
            state.adblock = adblock;
            state.mock_routes = mock_routes;
            state.blocked_patterns = blocked_patterns;
            if adblock_changed {
                refresh_native_content_filters(state);
            }
            let webviews: Vec<*mut c_void> = state
                .tabs
                .values()
                .filter_map(|s| (!s.webview.is_null()).then_some(s.webview))
                .collect();
            for wv in webviews {
                sync_policies_into_webview(state, wv);
            }
            let _ = reply.send(Ok(()));
        }
        WebKitActorCmd::GetActiveMeta { reply } => {
            if let Some(slot) = state.active_slot() {
                if !slot.frozen_drawing_area.is_null() {
                    let _ = reply.send((slot.frozen_url.clone(), slot.frozen_title.clone(), true));
                    return;
                }
                if !slot.webview.is_null() {
                    let uri_ptr = (state.api.webkit_web_view_get_uri)(slot.webview);
                    let uri = if uri_ptr.is_null() {
                        "about:blank".to_string()
                    } else {
                        CStr::from_ptr(uri_ptr).to_string_lossy().into_owned()
                    };
                    let title_ptr = (state.api.webkit_web_view_get_title)(slot.webview);
                    let title = if title_ptr.is_null() {
                        String::new()
                    } else {
                        CStr::from_ptr(title_ptr).to_string_lossy().into_owned()
                    };
                    let _ = reply.send((uri, title, false));
                    return;
                }
            }
            let _ = reply.send(("about:blank".to_string(), String::new(), false));
        }
        WebKitActorCmd::IsTabPlayingAudio { target_id, reply } => {
            let mut playing = false;
            if let Some(slot) = state.tabs.get(&target_id) {
                if slot.frozen_drawing_area.is_null() && !slot.webview.is_null() {
                    if let Some(is_playing_fn) = state.api.webkit_web_view_is_playing_audio {
                        playing = is_playing_fn(slot.webview) != 0;
                    }
                }
            }
            let _ = reply.send(playing);
        }
        WebKitActorCmd::SetWebGl { enabled, reply } => {
            state.webgl = enabled;
            let mut recycled = 0usize;
            for slot in state.tabs.values() {
                if !slot.webview.is_null() {
                    apply_webkit_diet_settings(
                        &state.api,
                        slot.webview,
                        !slot.is_gui_window,
                        enabled,
                        state.resource_profile,
                    );
                    if slot.frozen_drawing_area.is_null() {
                        if let Some(term_fn) = state.api.webkit_web_view_terminate_web_process {
                            term_fn(slot.webview);
                            recycled += 1;
                        }
                    }
                }
            }
            trim_process_heap();
            let _ = reply.send(recycled);
        }
        WebKitActorCmd::FreezeGui {
            freeze,
            idle_min_secs,
            reply,
        } => {
            if freeze {
                if let Some(min_s) = idle_min_secs {
                    if state.last_user_input.elapsed() < Duration::from_secs(min_s)
                        || !state.pending_loads.is_empty()
                        || !state.pending_evals.is_empty()
                    {
                        let _ = reply.send(false);
                        return;
                    }
                }
                let tids: Vec<String> = state
                    .tabs
                    .iter()
                    .filter(|(_, s)| {
                        s.is_gui_window && s.frozen_drawing_area.is_null() && !s.webview.is_null()
                    })
                    .map(|(tid, _)| tid.clone())
                    .collect();
                let mut any_frozen = false;
                for tid in tids {
                    if freeze_gui_slot(state, &tid) {
                        any_frozen = true;
                    }
                }
                let _ = reply.send(any_frozen);
            } else {
                let tids: Vec<String> = state
                    .tabs
                    .iter()
                    .filter(|(_, s)| !s.frozen_drawing_area.is_null())
                    .map(|(tid, _)| tid.clone())
                    .collect();
                let mut any_thawed = false;
                for tid in tids {
                    if ensure_slot_thawed(state, &tid, true) {
                        any_thawed = true;
                    }
                }
                let _ = reply.send(any_thawed);
            }
        }
        WebKitActorCmd::RecycleActiveRenderer { reply } => {
            let mut did_recycle = false;
            if let (Some(term_fn), Some(wv)) = (
                state.api.webkit_web_view_terminate_web_process,
                state
                    .active_slot()
                    .map(|s| s.webview)
                    .filter(|w| !w.is_null()),
            ) {
                term_fn(wv);
                did_recycle = true;
            }
            let default_ctx = (state.api.webkit_web_context_get_default)();
            if !default_ctx.is_null() {
                if let Some(clear_fn) = state.api.webkit_web_context_clear_cache {
                    clear_fn(default_ctx);
                }
            }
            trim_process_heap();
            let _ = reply.send(did_recycle);
        }
        WebKitActorCmd::TrimMemory {
            recycle_idle_renderer,
            reply,
        } => {
            let default_ctx = (state.api.webkit_web_context_get_default)();
            if !default_ctx.is_null() {
                if let Some(clear_fn) = state.api.webkit_web_context_clear_cache {
                    clear_fn(default_ctx);
                }
            }
            let mut recycled_count = 0usize;
            if recycle_idle_renderer {
                if let Some(term_fn) = state.api.webkit_web_view_terminate_web_process {
                    for slot in state.tabs.values() {
                        if !slot.webview.is_null() {
                            term_fn(slot.webview);
                            recycled_count += 1;
                        }
                    }
                }
            } else if let Ok(c_js) =
                CString::new("if (window.__minibrowse) window.__minibrowse.trimMediaAndCaches();")
            {
                for slot in state.tabs.values() {
                    if !slot.webview.is_null() {
                        (state.api.webkit_web_view_evaluate_javascript)(
                            slot.webview,
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
            }
            trim_process_heap();
            let _ = reply.send(recycled_count);
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
  var __fmtErr = function(e) {{
    if (!e) return "Unknown JavaScript error";
    if (typeof e === "string") return e;
    var name = e.name ? String(e.name) : "Error";
    var msg = e.message ? String(e.message) : "";
    var head = msg ? (msg.indexOf(name) === 0 ? msg : (name + ": " + msg)) : String(e);
    var st = e.stack ? String(e.stack).trim() : "";
    if (!st || st === "@" || st === "eval code@" || st === head) return head;
    return st.indexOf(head) === 0 ? st : (head + "\n" + st);
  }};
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
  var __src = {expr_json};
  try {{
    var __raw;
    try {{
      __raw = (0, eval)(__src);
    }} catch (__parseErr) {{
      var __pm = String((__parseErr && __parseErr.message) || "");
      if (__parseErr instanceof SyntaxError && (__pm.indexOf("Return statements") !== -1 || __pm.indexOf("'return'") !== -1 || __pm.indexOf("'await'") !== -1 || __pm.indexOf("await ") !== -1)) {{
        __raw = (0, eval)("(async function() {{\n" + __src + "\n}})()");
      }} else {{
        throw __parseErr;
      }}
    }}
    Promise.resolve(__raw).then(function(__v) {{
      __post(true, typeof __v, __v === undefined ? null : __v, __v === undefined, "");
    }}, function(__e) {{
      __post(false, "error", null, false, __fmtErr(__e));
    }});
    return "__mb_async__";
  }} catch (__syncErr) {{
    var __msg = __fmtErr(__syncErr);
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

type FrameCaptureReply = oneshot::Sender<Result<(RawFrame, Vec<u8>), String>>;

struct FrameCaptureFinishCtx {
    state_ptr: *mut WebKitActorState,
    webview: *mut c_void,
    boxes: Vec<AnnotationBox>,
    pointer: Option<(i32, i32, bool)>,
    reply: Option<FrameCaptureReply>,
}

unsafe fn start_webkit_frame_capture(
    state: &mut WebKitActorState,
    boxes: Vec<AnnotationBox>,
    pointer: Option<(i32, i32, bool)>,
    reply: oneshot::Sender<Result<(RawFrame, Vec<u8>), String>>,
) {
    let Some(webview) = state.active_slot().map(|s| s.webview) else {
        let frame =
            crate::vision::synthesize_gear0_frame(state.width, state.height, &boxes, "empty");
        let png = encode_bgra_frame_png(&frame);
        let _ = reply.send(Ok((frame, png)));
        return;
    };
    (state.api.gtk_widget_queue_draw)(webview);
    while (state.api.gtk_main_iteration_do)(0) != 0 {}

    let ctx_ptr = Box::into_raw(Box::new(FrameCaptureFinishCtx {
        state_ptr: state as *mut WebKitActorState,
        webview,
        boxes,
        pointer,
        reply: Some(reply),
    }));

    (state.api.webkit_web_view_get_snapshot)(
        webview,
        WEBKIT_SNAPSHOT_REGION_VISIBLE,
        WEBKIT_SNAPSHOT_OPTIONS_NONE,
        ptr::null_mut(),
        Some(on_webkit_frame_capture_ready),
        ctx_ptr as *mut c_void,
    );
}

unsafe extern "C" fn on_webkit_frame_capture_ready(
    source: *mut c_void,
    res: *mut c_void,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let mut ctx = Box::from_raw(user_data as *mut FrameCaptureFinishCtx);
    let state = &mut *ctx.state_ptr;
    let Some(reply) = ctx.reply.take() else {
        return;
    };

    let mut err: *mut GError = ptr::null_mut();
    let snap_surface = (state.api.webkit_web_view_get_snapshot_finish)(source, res, &mut err);
    if !err.is_null() {
        (state.api.g_error_free)(err);
    }

    let w = state
        .api
        .cairo_image_surface_get_width
        .and_then(|f| (!snap_surface.is_null()).then(|| f(snap_surface)))
        .filter(|&v| v > 0)
        .unwrap_or(state.width as c_int);
    let h = state
        .api
        .cairo_image_surface_get_height
        .and_then(|f| (!snap_surface.is_null()).then(|| f(snap_surface)))
        .filter(|&v| v > 0)
        .unwrap_or(state.height as c_int);

    // Blit into an explicit CAIRO_FORMAT_ARGB32 image surface so `cairo_image_surface_get_data`
    // is guaranteed non-null even when WebKit returns an Xlib/GL surface.
    let img_surf = (state.api.cairo_image_surface_create)(CAIRO_FORMAT_ARGB32, w, h);
    if !img_surf.is_null() {
        let cr = (state.api.cairo_create)(img_surf);
        if !cr.is_null() {
            if !snap_surface.is_null() {
                if let (Some(set_src), Some(paint)) =
                    (state.api.cairo_set_source_surface, state.api.cairo_paint)
                {
                    set_src(cr, snap_surface, 0.0, 0.0);
                    paint(cr);
                }
            } else if !ctx.webview.is_null() {
                (state.api.gtk_widget_draw)(ctx.webview, cr);
            }
            (state.api.cairo_destroy)(cr);
        }
    }
    if !snap_surface.is_null() {
        (state.api.cairo_surface_destroy)(snap_surface);
    }

    let mut png_bytes = Vec::with_capacity(32 * 1024);
    let mut frame_opt: Option<RawFrame> = None;

    if !img_surf.is_null() {
        (state.api.cairo_surface_flush)(img_surf);
        if let (Some(get_data), Some(get_stride)) = (
            state.api.cairo_image_surface_get_data,
            state.api.cairo_image_surface_get_stride,
        ) {
            let data_ptr = get_data(img_surf);
            let stride = get_stride(img_surf) as usize;
            let total_len = stride.saturating_mul(h as usize);
            if !data_ptr.is_null() && total_len > 0 {
                let slice = std::slice::from_raw_parts_mut(data_ptr, total_len);
                if !ctx.boxes.is_empty() {
                    draw_annotations_bgra(slice, w as u32, h as u32, stride, &ctx.boxes);
                }
                if let Some((px, py, clicked)) = ctx.pointer {
                    draw_pointer_indicator_bgra(slice, w as u32, h as u32, stride, px, py, clicked);
                }
                if !ctx.boxes.is_empty() || ctx.pointer.is_some() {
                    if let Some(mark_dirty) = state.api.cairo_surface_mark_dirty {
                        mark_dirty(img_surf);
                    }
                }
                frame_opt = Some(pack_bgra_frame(slice, w as u32, h as u32, stride));
            }
        }
        let _ = (state.api.cairo_surface_write_to_png_stream)(
            img_surf,
            cairo_vec_writer,
            (&mut png_bytes as *mut Vec<u8>).cast::<c_void>(),
        );
        (state.api.cairo_surface_destroy)(img_surf);
    }

    let frame = frame_opt.unwrap_or_else(|| {
        crate::vision::synthesize_gear0_frame(w as u32, h as u32, &ctx.boxes, "webkit-fallback")
    });
    if png_bytes.is_empty() {
        png_bytes = encode_bgra_frame_png(&frame);
    }
    trim_process_heap();
    let _ = reply.send(Ok((frame, png_bytes)));
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

struct PdfFinishCtx {
    state_ptr: *mut WebKitActorState,
    webview: *mut c_void,
    landscape: bool,
    reply: Option<oneshot::Sender<Result<String, String>>>,
}

unsafe fn start_webkit_pdf_export(
    state: &mut WebKitActorState,
    landscape: bool,
    reply: oneshot::Sender<Result<String, String>>,
) {
    let Some(webview) = state.active_slot().map(|s| s.webview) else {
        let fallback = encode_minimal_pdf(&["MiniBrowse Document".to_string()], landscape);
        let _ = reply.send(Ok(base64_encode(&fallback)));
        return;
    };
    (state.api.gtk_widget_queue_draw)(webview);
    while (state.api.gtk_main_iteration_do)(0) != 0 {}

    let ctx_ptr = Box::into_raw(Box::new(PdfFinishCtx {
        state_ptr: state as *mut WebKitActorState,
        webview,
        landscape,
        reply: Some(reply),
    }));

    (state.api.webkit_web_view_get_snapshot)(
        webview,
        WEBKIT_SNAPSHOT_REGION_VISIBLE,
        WEBKIT_SNAPSHOT_OPTIONS_NONE,
        ptr::null_mut(),
        Some(on_webkit_pdf_snapshot_ready),
        ctx_ptr as *mut c_void,
    );
}

unsafe extern "C" fn on_webkit_pdf_snapshot_ready(
    source: *mut c_void,
    res: *mut c_void,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let mut ctx = Box::from_raw(user_data as *mut PdfFinishCtx);
    let state = &mut *ctx.state_ptr;
    let Some(reply) = ctx.reply.take() else {
        return;
    };

    let mut err: *mut GError = ptr::null_mut();
    let surface = (state.api.webkit_web_view_get_snapshot_finish)(source, res, &mut err);
    if !err.is_null() {
        (state.api.g_error_free)(err);
    }

    let mut pdf_bytes = Vec::with_capacity(48 * 1024);
    let (page_w, page_h) = if ctx.landscape {
        (792.0_f64, 612.0_f64)
    } else {
        (612.0_f64, 792.0_f64)
    };

    if let (Some(create_pdf), Some(surf_finish), Some(set_src), Some(paint), Some(scale)) = (
        state.api.cairo_pdf_surface_create_for_stream,
        state.api.cairo_surface_finish,
        state.api.cairo_set_source_surface,
        state.api.cairo_paint,
        state.api.cairo_scale,
    ) {
        let pdf_surf = create_pdf(
            cairo_vec_writer,
            (&mut pdf_bytes as *mut Vec<u8>).cast::<c_void>(),
            page_w,
            page_h,
        );
        if !pdf_surf.is_null() {
            let cr = (state.api.cairo_create)(pdf_surf);
            if !cr.is_null() {
                if !surface.is_null() {
                    let sw = state
                        .api
                        .cairo_image_surface_get_width
                        .map_or(state.width as c_int, |f| f(surface))
                        .max(1) as f64;
                    let sh = state
                        .api
                        .cairo_image_surface_get_height
                        .map_or(state.height as c_int, |f| f(surface))
                        .max(1) as f64;
                    let factor = (page_w / sw).min(page_h / sh);
                    scale(cr, factor, factor);
                    set_src(cr, surface, 0.0, 0.0);
                    paint(cr);
                } else if !ctx.webview.is_null() {
                    let w = (state.api.gtk_widget_get_allocated_width)(ctx.webview)
                        .max(state.width as c_int) as f64;
                    let factor = (page_w / w.max(1.0)).min(1.0);
                    scale(cr, factor, factor);
                    (state.api.gtk_widget_draw)(ctx.webview, cr);
                }
                (state.api.cairo_destroy)(cr);
            }
            surf_finish(pdf_surf);
            (state.api.cairo_surface_destroy)(pdf_surf);
        }
    }

    if !surface.is_null() {
        (state.api.cairo_surface_destroy)(surface);
    }

    if !pdf_bytes.starts_with(b"%PDF-") {
        pdf_bytes = encode_minimal_pdf(&["MiniBrowse Rendered Page".to_string()], ctx.landscape);
    }

    trim_process_heap();
    let _ = reply.send(Ok(base64_encode(&pdf_bytes)));
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

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessTreeMemory {
    pub worker_rss_mb: f64,
    pub renderer_rss_mb: f64,
    pub network_rss_mb: f64,
    pub tree_rss_mb: f64,
    pub tree_pss_mb: f64,
    pub tree_threads: u32,
    pub process_count: usize,
}

/// Walks the entire `minibrowse` + `WebKitNetworkProcess` + `WebKitWebProcess` descendant tree
/// under `/proc` so status tools report honest full-session RSS, PSS (`smaps_rollup`), and threads.
#[must_use]
pub fn sample_process_tree_memory() -> ProcessTreeMemory {
    #[cfg(target_os = "linux")]
    {
        let root_pid = std::process::id();
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
        let mut queue = vec![root_pid];
        let mut visited = BTreeSet::new();
        visited.insert(root_pid);

        let mut idx = 0;
        while idx < queue.len() {
            let pid = queue[idx];
            idx += 1;
            let task_dir = format!("/proc/{pid}/task");
            if let Ok(tasks) = std::fs::read_dir(task_dir) {
                for entry in tasks.flatten() {
                    let children_path = entry.path().join("children");
                    if let Ok(raw) = std::fs::read_to_string(children_path) {
                        for tok in raw.split_whitespace() {
                            if let Ok(child_pid) = tok.parse::<u32>() {
                                if visited.insert(child_pid) {
                                    queue.push(child_pid);
                                }
                            }
                        }
                    }
                }
            }
        }

        let round1 = |mb: f64| (mb * 10.0).round() / 10.0;
        let mut worker_rss = 0.0;
        let mut renderer_rss = 0.0;
        let mut network_rss = 0.0;
        let mut total_rss = 0.0;
        let mut total_pss = 0.0;
        let mut total_threads = 0u32;
        let mut live_procs = 0usize;

        for pid in visited {
            let rss_mb = std::fs::read_to_string(format!("/proc/{pid}/statm"))
                .ok()
                .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
                .map(|pages| (pages * page_size) as f64 / (1024.0 * 1024.0));
            let Some(pid_rss) = rss_mb else {
                continue;
            };
            live_procs += 1;
            total_rss += pid_rss;

            let pss_mb = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
                .ok()
                .and_then(|rollup| {
                    for line in rollup.lines() {
                        if let Some(rest) = line.strip_prefix("Pss:") {
                            let kb = rest.split_whitespace().next()?.parse::<u64>().ok()?;
                            return Some(kb as f64 / 1024.0);
                        }
                    }
                    None
                })
                .unwrap_or(pid_rss);
            total_pss += pss_mb;

            if let Ok(status_txt) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
                for line in status_txt.lines() {
                    if let Some(rest) = line.strip_prefix("Threads:") {
                        if let Ok(th) = rest.trim().parse::<u32>() {
                            total_threads += th;
                        }
                        break;
                    }
                }
            }

            if pid == root_pid {
                worker_rss += pid_rss;
            } else {
                let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                if comm.contains("network") {
                    network_rss += pid_rss;
                } else {
                    renderer_rss += pid_rss;
                }
            }
        }

        ProcessTreeMemory {
            worker_rss_mb: round1(worker_rss),
            renderer_rss_mb: round1(renderer_rss),
            network_rss_mb: round1(network_rss),
            tree_rss_mb: round1(total_rss),
            tree_pss_mb: round1(total_pss),
            tree_threads: total_threads,
            process_count: live_procs,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        ProcessTreeMemory::default()
    }
}

/// Returns total session-tree Resident Set Size (RSS) in MiB across worker + WebKit sub-processes.
#[must_use]
pub fn current_rss_mb() -> f64 {
    sample_process_tree_memory().tree_rss_mb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_init_script_iife_isolates_top_level_consts() {
        let src = "const _mm = window.matchMedia.bind(window); window.__ok = true;";
        let wrapped = wrap_init_script_iife(src);
        assert!(wrapped.starts_with("(function(){\n"));
        assert!(wrapped.ends_with("\n})();"));
        assert!(wrapped.contains(src));
    }

    #[test]
    fn test_build_emulated_media_js_serializes_features_and_clear_state() {
        let empty = EmulatedMediaState::default();
        assert!(empty.is_empty());
        assert_eq!(
            build_emulated_media_js(&empty),
            "window.__minibrowse && window.__minibrowse.setEmulatedMedia(\"\", []);"
        );

        let reduced = EmulatedMediaState {
            media: Some("screen".to_string()),
            features: vec![("prefers-reduced-motion".to_string(), "reduce".to_string())],
        };
        assert!(!reduced.is_empty());
        let js = build_emulated_media_js(&reduced);
        assert!(js.contains("setEmulatedMedia(\"screen\""));
        assert!(js.contains("prefers-reduced-motion"));
        assert!(js.contains("reduce"));
    }
}
