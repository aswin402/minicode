//! Dedicated Low-RAM WebKit2GTK Actor Thread + Automatic Pure-Rust Tier-0 Fallback Router.
//!
//! - Uses `g_idle_add_full` cross-thread `eventfd` wakeups so idle CPU usage is strictly `0.0%`.
//! - Resolves both synchronous JS and async `Promise`s via the `minibrowseBridge` microtask channel.
//! - Captures PNG frames directly in RAM via Cairo `cairo_surface_write_to_png_stream` (zero temp files).

use crate::fallback::{encode_solid_aura_png, MockRoute, PureRustPageState};
use crate::ffi::{
    base64_encode, trim_process_heap, tune_allocator_env, GError, WebKitGtkApi,
    CAIRO_FORMAT_ARGB32, CAIRO_STATUS_SUCCESS, GTK_WINDOW_TOPLEVEL, G_PRIORITY_DEFAULT_IDLE,
    WEBKIT_CACHE_MODEL_DOCUMENT_VIEWER, WEBKIT_HARDWARE_ACCELERATION_POLICY_NEVER,
    WEBKIT_HARDWARE_ACCELERATION_POLICY_ON_DEMAND, WEBKIT_LOAD_FINISHED,
    WEBKIT_SNAPSHOT_OPTIONS_NONE, WEBKIT_SNAPSHOT_REGION_VISIBLE,
    WEBKIT_USER_CONTENT_INJECTED_ALL_FRAMES, WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::{c_int, c_void, CStr, CString};
use std::ptr;
use std::sync::{mpsc, Arc, Mutex};
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
    GetStatus {
        reply: oneshot::Sender<Value>,
    },
    #[allow(dead_code)]
    Shutdown,
}

#[derive(Clone)]
pub struct BrowserEngineHandle {
    sender: EngineSender,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
    backend_kind: Arc<Mutex<String>>,
}

#[derive(Clone)]
enum EngineSender {
    WebKit {
        cmd_tx: mpsc::Sender<BrowserBackendCmd>,
        wakeup_fn: unsafe extern "C" fn(
            c_int,
            crate::ffi::GSourceFunc,
            *mut c_void,
            Option<crate::ffi::GDestroyNotify>,
        ) -> u32,
        state_ptr: usize,
    },
    PureRust {
        cmd_tx: tokio::sync::mpsc::UnboundedSender<BrowserBackendCmd>,
    },
}

impl BrowserEngineHandle {
    pub fn subscribe_events(&self) -> broadcast::Receiver<BrowserBackendEvent> {
        self.events_tx.subscribe()
    }

    pub fn backend_name(&self) -> String {
        self.backend_kind
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "minibrowse".to_string())
    }

    pub fn send(&self, cmd: BrowserBackendCmd) -> Result<(), String> {
        match &self.sender {
            EngineSender::WebKit {
                cmd_tx,
                wakeup_fn,
                state_ptr,
            } => {
                cmd_tx
                    .send(cmd)
                    .map_err(|_| "WebKit actor thread terminated".to_string())?;
                unsafe {
                    (wakeup_fn)(
                        G_PRIORITY_DEFAULT_IDLE,
                        drain_webkit_commands_idle,
                        *state_ptr as *mut c_void,
                        None,
                    );
                }
                Ok(())
            }
            EngineSender::PureRust { cmd_tx } => cmd_tx
                .send(cmd)
                .map_err(|_| "Pure-Rust actor task terminated".to_string()),
        }
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

    if !force_fallback && has_display {
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
        let (cmd_tx, cmd_rx) = mpsc::channel::<BrowserBackendCmd>();
        let ev_clone = events_tx.clone();
        let opts_clone = opts.clone();

        std::thread::Builder::new()
            .name("minibrowse-gtk".to_string())
            .spawn(move || {
                run_webkit_thread(opts_clone, cmd_rx, ev_clone, init_tx);
            })
            .map_err(|e| format!("Failed spawning minibrowse GTK thread: {e}"))?;

        if let Ok(Ok((state_ptr, wakeup_fn))) = init_rx.recv() {
            let backend_kind = Arc::new(Mutex::new(if opts.headless {
                "webkit2gtk-offscreen".to_string()
            } else {
                "webkit2gtk-gui".to_string()
            }));
            return Ok(BrowserEngineHandle {
                sender: EngineSender::WebKit {
                    cmd_tx,
                    wakeup_fn,
                    state_ptr,
                },
                events_tx,
                backend_kind,
            });
        }
    }

    // Tier-0 Displayless / Container Pure-Rust Fallback
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let ev_clone = events_tx.clone();
    tokio::spawn(run_pure_rust_actor(opts, cmd_rx, ev_clone));

    Ok(BrowserEngineHandle {
        sender: EngineSender::PureRust { cmd_tx },
        events_tx,
        backend_kind: Arc::new(Mutex::new("pure-rust-dom".to_string())),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tier-1 Native WebKit2GTK + Cairo Actor Thread
// ─────────────────────────────────────────────────────────────────────────────

struct WebKitActorState {
    api: WebKitGtkApi,
    window: *mut c_void,
    webview: *mut c_void,
    ucm: *mut c_void,
    headless: bool,
    width: u32,
    height: u32,
    adblock: bool,
    mock_routes: Vec<MockRoute>,
    blocked_patterns: Vec<String>,
    cmd_rx: mpsc::Receiver<BrowserBackendCmd>,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
    pending_loads: Vec<oneshot::Sender<Result<Value, String>>>,
    next_eval_token: u64,
    pending_evals: HashMap<u64, (bool, oneshot::Sender<Result<Value, String>>)>,
}

#[allow(clippy::type_complexity)]
fn run_webkit_thread(
    opts: LaunchOptions,
    cmd_rx: mpsc::Receiver<BrowserBackendCmd>,
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

        // 1. Aggressive MemoryPressureSettings (128MB cap, strict 50% purge)
        let mp = (api.webkit_memory_pressure_settings_new)();
        if !mp.is_null() {
            (api.webkit_memory_pressure_settings_set_memory_limit)(mp, 128);
            (api.webkit_memory_pressure_settings_set_conservative_threshold)(mp, 0.30);
            (api.webkit_memory_pressure_settings_set_strict_threshold)(mp, 0.50);
            (api.webkit_memory_pressure_settings_set_kill_threshold)(mp, 0.85);
            (api.webkit_memory_pressure_settings_set_poll_interval)(mp, 2.0);
            (api.webkit_website_data_manager_set_memory_pressure_settings)(mp);
            (api.webkit_memory_pressure_settings_free)(mp);
        }

        // 2. Single-process DocumentViewer cache diet (zero BFCache bloat)
        let web_ctx = (api.webkit_web_context_get_default)();
        if !web_ctx.is_null() {
            (api.webkit_web_context_set_cache_model)(web_ctx, WEBKIT_CACHE_MODEL_DOCUMENT_VIEWER);
            (api.webkit_web_context_set_web_process_count_limit)(web_ctx, 1);
        }

        // 3. UserContentManager + minibrowseBridge script message channel + bridge.js injection
        let ucm = (api.webkit_user_content_manager_new)();
        if let Ok(handler_name) = CString::new("minibrowseBridge") {
            let _ = (api.webkit_user_content_manager_register_script_message_handler)(
                ucm,
                handler_name.as_ptr(),
            );
        }
        inject_user_script(&api, ucm, BRIDGE_JS);

        // 4. WebKitWebView + Settings
        let webview = (api.webkit_web_view_new_with_user_content_manager)(ucm);
        let settings = (api.webkit_web_view_get_settings)(webview);
        if !settings.is_null() {
            (api.webkit_settings_set_enable_page_cache)(settings, 0);
            (api.webkit_settings_set_enable_smooth_scrolling)(settings, 0);
            let hw_policy = if opts.headless {
                WEBKIT_HARDWARE_ACCELERATION_POLICY_NEVER
            } else {
                WEBKIT_HARDWARE_ACCELERATION_POLICY_ON_DEMAND
            };
            (api.webkit_settings_set_hardware_acceleration_policy)(settings, hw_policy);
        }

        // 5. Window container: GtkOffscreenWindow in headless mode (zero desktop clutter!)
        let w_i32 = opts.width.max(320) as c_int;
        let h_i32 = opts.height.max(240) as c_int;
        let window = if opts.headless {
            (api.gtk_offscreen_window_new)()
        } else {
            let win = (api.gtk_window_new)(GTK_WINDOW_TOPLEVEL);
            if let Ok(title) = CString::new("minibrowse — Agentic Browser") {
                (api.gtk_window_set_title)(win, title.as_ptr());
            }
            win
        };
        (api.gtk_window_set_default_size)(window, w_i32, h_i32);
        (api.gtk_widget_set_size_request)(webview, w_i32, h_i32);
        (api.gtk_container_add)(window, webview);
        (api.gtk_widget_show_all)(window);

        let wakeup_fn = api.g_idle_add_full;
        let boxed_state = Box::new(WebKitActorState {
            api,
            window,
            webview,
            ucm,
            headless: opts.headless,
            width: opts.width.max(320),
            height: opts.height.max(240),
            adblock: opts.adblock,
            mock_routes: Vec::new(),
            blocked_patterns: Vec::new(),
            cmd_rx,
            events_tx,
            pending_loads: Vec::new(),
            next_eval_token: 1,
            pending_evals: HashMap::new(),
        });
        let state_ptr = Box::into_raw(boxed_state);

        // Connect GObject signals for load lifecycle and script messages
        if let Ok(sig) = CString::new("load-changed") {
            let _ = ((*state_ptr).api.g_signal_connect_data)(
                webview,
                sig.as_ptr(),
                on_webkit_load_changed as *const c_void,
                state_ptr as *mut c_void,
                ptr::null(),
                0,
            );
        }
        if let Ok(sig) = CString::new("script-message-received::minibrowseBridge") {
            let _ = ((*state_ptr).api.g_signal_connect_data)(
                ucm,
                sig.as_ptr(),
                on_script_message_received as *const c_void,
                state_ptr as *mut c_void,
                ptr::null(),
                0,
            );
        }
        if !opts.headless {
            if let Ok(sig) = CString::new("destroy") {
                let _ = ((*state_ptr).api.g_signal_connect_data)(
                    window,
                    sig.as_ptr(),
                    on_window_destroy as *const c_void,
                    state_ptr as *mut c_void,
                    ptr::null(),
                    0,
                );
            }
        }

        // Load initial blank or user-requested document
        let start_uri = opts.initial_url.as_deref().unwrap_or("about:blank");
        if let Ok(c_uri) = CString::new(start_uri) {
            ((*state_ptr).api.webkit_web_view_load_uri)(webview, c_uri.as_ptr());
        }

        let _ = init_tx.send(Ok((state_ptr as usize, wakeup_fn)));
        ((*state_ptr).api.gtk_main)();

        let _ = Box::from_raw(state_ptr);
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
    _webview: *mut c_void,
    load_event: c_int,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let state = &mut *(user_data as *mut WebKitActorState);
    if load_event == WEBKIT_LOAD_FINISHED {
        // Re-sync active mock routes, blocked patterns, and adblock policy into the fresh document
        sync_policies_into_page(state);

        for waiter in state.pending_loads.drain(..) {
            let _ = waiter.send(Ok(json!({ "frameId": "main" })));
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

unsafe fn sync_policies_into_page(state: &WebKitActorState) {
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
            state.webview,
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

unsafe fn handle_webkit_command(state: &mut WebKitActorState, cmd: BrowserBackendCmd) {
    match cmd {
        BrowserBackendCmd::Navigate { url, reply } => {
            if let Some(rest) = url.strip_prefix("data:text/html,") {
                let decoded = percent_decode_simple(rest);
                if let (Ok(c_html), Ok(c_base)) =
                    (CString::new(decoded), CString::new("http://localhost/"))
                {
                    state.pending_loads.push(reply);
                    (state.api.webkit_web_view_load_html)(
                        state.webview,
                        c_html.as_ptr(),
                        c_base.as_ptr(),
                    );
                } else {
                    let _ = reply.send(Err("Invalid NUL byte in data URL".to_string()));
                }
            } else if let Ok(c_url) = CString::new(url) {
                state.pending_loads.push(reply);
                (state.api.webkit_web_view_load_uri)(state.webview, c_url.as_ptr());
            } else {
                let _ = reply.send(Err("Invalid URL containing NUL byte".to_string()));
            }
        }
        BrowserBackendCmd::Reload {
            ignore_cache,
            reply,
        } => {
            state.pending_loads.push(reply);
            if ignore_cache {
                (state.api.webkit_web_view_reload_bypass_cache)(state.webview);
            } else {
                (state.api.webkit_web_view_reload)(state.webview);
            }
        }
        BrowserBackendCmd::EvaluateJs { expression, reply } => {
            start_webkit_js_eval(state, &expression, reply);
        }
        BrowserBackendCmd::AddInitScript { source, reply } => {
            inject_user_script(&state.api, state.ucm, &source);
            let _ = reply.send(Ok(()));
        }
        BrowserBackendCmd::SetViewport {
            width,
            height,
            reply,
        } => {
            state.width = width.max(320);
            state.height = height.max(240);
            (state.api.gtk_widget_set_size_request)(
                state.webview,
                state.width as c_int,
                state.height as c_int,
            );
            (state.api.gtk_window_resize)(
                state.window,
                state.width as c_int,
                state.height as c_int,
            );
            (state.api.gtk_widget_queue_resize)(state.webview);
            (state.api.gtk_widget_queue_draw)(state.webview);
            let _ = reply.send(Ok(()));
        }
        BrowserBackendCmd::CaptureScreenshotPng { reply } => {
            start_webkit_snapshot(state, reply);
        }
        BrowserBackendCmd::DispatchMouse {
            event_type,
            x,
            y,
            delta_x,
            delta_y,
            reply,
        } => {
            let ev_json =
                serde_json::to_string(&event_type).unwrap_or_else(|_| "\"mouseMoved\"".to_string());
            let js = format!(
                "window.__minibrowse ? window.__minibrowse.dispatchPointer({ev_json}, {x}, {y}, {delta_x}, {delta_y}) : false"
            );
            let (tx, _rx) = oneshot::channel();
            start_webkit_js_eval(state, &js, tx);
            let _ = reply.send(Ok(()));
        }
        BrowserBackendCmd::DispatchKey {
            event_type,
            key,
            code,
            modifiers,
            reply,
        } => {
            let ev_json =
                serde_json::to_string(&event_type).unwrap_or_else(|_| "\"keyDown\"".to_string());
            let key_json = serde_json::to_string(&key).unwrap_or_else(|_| "\"\"".to_string());
            let code_json = serde_json::to_string(&code).unwrap_or_else(|_| "\"\"".to_string());
            let js = format!(
                "window.__minibrowse ? window.__minibrowse.dispatchKeyboard({ev_json}, {key_json}, {code_json}, {modifiers}) : false"
            );
            let (tx, _rx) = oneshot::channel();
            start_webkit_js_eval(state, &js, tx);
            let _ = reply.send(Ok(()));
        }
        BrowserBackendCmd::SetMockRoutes { routes, reply } => {
            let count = routes.len();
            state.mock_routes = routes;
            sync_policies_into_page(state);
            let _ = reply.send(Ok(count));
        }
        BrowserBackendCmd::SetBlockedPatterns { patterns, reply } => {
            let count = patterns.len();
            state.blocked_patterns = patterns;
            sync_policies_into_page(state);
            let _ = reply.send(Ok(count));
        }
        BrowserBackendCmd::SetAdBlock { enabled, reply } => {
            state.adblock = enabled;
            sync_policies_into_page(state);
            let _ = reply.send(Ok(enabled));
        }
        BrowserBackendCmd::ResearchExtract {
            query,
            max_chars,
            reply,
        } => {
            let q_json = serde_json::to_string(&query.unwrap_or_default())
                .unwrap_or_else(|_| "\"\"".to_string());
            let expr = format!("window.__minibrowse.extractResearch({q_json}, {max_chars})");
            start_webkit_unwrapped_call(state, &expr, reply);
        }
        BrowserBackendCmd::WebDebugAudit { reply } => {
            start_webkit_unwrapped_call(state, "window.__minibrowse.auditWebDebug()", reply);
        }
        BrowserBackendCmd::UiDiff {
            query,
            max_chars,
            reply,
        } => {
            let q_json = serde_json::to_string(&query.unwrap_or_default())
                .unwrap_or_else(|_| "\"\"".to_string());
            let expr = format!("window.__minibrowse.captureUiDiff({q_json}, {max_chars})");
            start_webkit_unwrapped_call(state, &expr, reply);
        }
        BrowserBackendCmd::NetworkWaterfall { reply } => {
            start_webkit_unwrapped_call(state, "window.__minibrowse.networkWaterfall()", reply);
        }
        BrowserBackendCmd::StorageInspect { reply } => {
            start_webkit_unwrapped_call(state, "window.__minibrowse.inspectStorage()", reply);
        }
        BrowserBackendCmd::GetStatus { reply } => {
            let uri_ptr = (state.api.webkit_web_view_get_uri)(state.webview);
            let uri = if uri_ptr.is_null() {
                "about:blank".to_string()
            } else {
                CStr::from_ptr(uri_ptr).to_string_lossy().into_owned()
            };
            let title_ptr = (state.api.webkit_web_view_get_title)(state.webview);
            let title = if title_ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(title_ptr).to_string_lossy().into_owned()
            };
            let _ = reply.send(json!({
                "engine": if state.headless { "webkit2gtk-offscreen" } else { "webkit2gtk-gui" },
                "url": uri,
                "title": title,
                "viewport": { "width": state.width, "height": state.height },
                "rss_mb": current_rss_mb(),
            }));
        }
        BrowserBackendCmd::Shutdown => {
            (state.api.gtk_widget_destroy)(state.window);
            (state.api.gtk_main_quit)();
        }
    }
}

struct EvalFinishCtx {
    state_ptr: *mut WebKitActorState,
    token: u64,
}

unsafe fn start_webkit_unwrapped_call(
    state: &mut WebKitActorState,
    expr: &str,
    reply: oneshot::Sender<Result<Value, String>>,
) {
    start_webkit_js_eval_inner(state, expr, true, reply);
}

unsafe fn start_webkit_js_eval(
    state: &mut WebKitActorState,
    expression: &str,
    reply: oneshot::Sender<Result<Value, String>>,
) {
    start_webkit_js_eval_inner(state, expression, false, reply);
}

unsafe fn start_webkit_js_eval_inner(
    state: &mut WebKitActorState,
    expression: &str,
    unwrap_val: bool,
    reply: oneshot::Sender<Result<Value, String>>,
) {
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
        state.webview,
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
        // Fallback if `minibrowseBridge` handler was unavailable on an exotic scheme
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
    reply: Option<oneshot::Sender<Result<String, String>>>,
}

unsafe fn start_webkit_snapshot(
    state: &mut WebKitActorState,
    reply: oneshot::Sender<Result<String, String>>,
) {
    (state.api.gtk_widget_queue_draw)(state.webview);
    while (state.api.gtk_main_iteration_do)(0) != 0 {}

    let ctx_ptr = Box::into_raw(Box::new(SnapshotFinishCtx {
        state_ptr: state as *mut WebKitActorState,
        reply: Some(reply),
    }));

    (state.api.webkit_web_view_get_snapshot)(
        state.webview,
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

    // Fallback to direct GTK widget draw onto an ARGB32 Cairo surface if WebKit snapshot was empty
    if png_bytes.is_empty() {
        let w = (state.api.gtk_widget_get_allocated_width)(state.webview).max(state.width as c_int);
        let h =
            (state.api.gtk_widget_get_allocated_height)(state.webview).max(state.height as c_int);
        let fallback_surf = (state.api.cairo_image_surface_create)(CAIRO_FORMAT_ARGB32, w, h);
        if !fallback_surf.is_null() {
            let cr = (state.api.cairo_create)(fallback_surf);
            if !cr.is_null() {
                (state.api.gtk_widget_draw)(state.webview, cr);
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

// ─────────────────────────────────────────────────────────────────────────────
//  Tier-0 Displayless / Container Pure-Rust Actor Loop
// ─────────────────────────────────────────────────────────────────────────────

async fn run_pure_rust_actor(
    opts: LaunchOptions,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<BrowserBackendCmd>,
    events_tx: broadcast::Sender<BrowserBackendEvent>,
) {
    let mut page = PureRustPageState::new(opts.width, opts.height, opts.adblock);
    if let Some(ref url) = opts.initial_url {
        let _ = page.navigate(url).await;
    }

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            BrowserBackendCmd::Navigate { url, reply } => {
                let res = page.navigate(&url).await;
                match res {
                    Ok(net_opt) => {
                        if let Some(net) = net_opt {
                            if !net.ok || net.status >= 400 {
                                let _ = events_tx.send(BrowserBackendEvent::NetworkError {
                                    method: net.method,
                                    url: net.url,
                                    status: net.status,
                                    error: net.error.unwrap_or_else(|| "HTTP error".to_string()),
                                });
                            }
                        }
                        let _ = events_tx.send(BrowserBackendEvent::Lifecycle(
                            "DOMContentLoaded".to_string(),
                        ));
                        let _ = events_tx.send(BrowserBackendEvent::Lifecycle("load".to_string()));
                        let _ = events_tx
                            .send(BrowserBackendEvent::Lifecycle("networkIdle".to_string()));
                        let _ = reply.send(Ok(json!({ "frameId": "main" })));
                    }
                    Err(e) => {
                        let _ = events_tx.send(BrowserBackendEvent::NetworkError {
                            method: "GET".to_string(),
                            url,
                            status: 0,
                            error: e.clone(),
                        });
                        let _ = reply.send(Err(e));
                    }
                }
            }
            BrowserBackendCmd::Reload { reply, .. } => {
                let current = page.current_url.clone();
                let res = page
                    .navigate(&current)
                    .await
                    .map(|_| json!({ "frameId": "main" }));
                let _ = events_tx.send(BrowserBackendEvent::Lifecycle("networkIdle".to_string()));
                let _ = reply.send(res);
            }
            BrowserBackendCmd::EvaluateJs { expression, reply } => {
                let val = page.evaluate_expression(&expression);
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
            BrowserBackendCmd::AddInitScript { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            BrowserBackendCmd::SetViewport {
                width,
                height,
                reply,
            } => {
                page.viewport_width = width.max(320);
                page.viewport_height = height.max(240);
                let _ = reply.send(Ok(()));
            }
            BrowserBackendCmd::CaptureScreenshotPng { reply } => {
                let png = page.render_synthetic_png();
                let _ = reply.send(Ok(base64_encode(&png)));
            }
            BrowserBackendCmd::DispatchMouse { reply, .. }
            | BrowserBackendCmd::DispatchKey { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            BrowserBackendCmd::SetMockRoutes { routes, reply } => {
                let count = routes.len();
                page.mock_routes = routes;
                let _ = reply.send(Ok(count));
            }
            BrowserBackendCmd::SetBlockedPatterns { patterns, reply } => {
                let count = patterns.len();
                page.blocked_patterns = patterns;
                let _ = reply.send(Ok(count));
            }
            BrowserBackendCmd::SetAdBlock { enabled, reply } => {
                page.adblock_enabled = enabled;
                let _ = reply.send(Ok(enabled));
            }
            BrowserBackendCmd::ResearchExtract {
                query,
                max_chars,
                reply,
            } => {
                let _ = reply.send(Ok(page.extract_research(query.as_deref(), max_chars)));
            }
            BrowserBackendCmd::WebDebugAudit { reply } => {
                let _ = reply.send(Ok(page.audit_web_debug()));
            }
            BrowserBackendCmd::UiDiff {
                query,
                max_chars,
                reply,
            } => {
                let _ = reply.send(Ok(page.capture_ui_diff(query.as_deref(), max_chars)));
            }
            BrowserBackendCmd::NetworkWaterfall { reply } => {
                let _ = reply.send(Ok(page.network_waterfall()));
            }
            BrowserBackendCmd::StorageInspect { reply } => {
                let _ = reply.send(Ok(page.inspect_storage()));
            }
            BrowserBackendCmd::GetStatus { reply } => {
                let _ = reply.send(json!({
                    "engine": "pure-rust-dom",
                    "url": page.current_url,
                    "title": page.title,
                    "viewport": { "width": page.viewport_width, "height": page.viewport_height },
                    "rss_mb": current_rss_mb(),
                }));
            }
            BrowserBackendCmd::Shutdown => break,
        }
    }
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
