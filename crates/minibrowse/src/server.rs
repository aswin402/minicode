//! Single-Port CDP (`1.3`) + `MiniBrowse.*` HTTP Discovery & WebSocket Server.
//!
//! Multiplexes HTTP `/json/version`, `/json/list`, `/health` and WebSocket `/devtools/...`
//! on a single loopback port (`127.0.0.1:<port>`) so `minicode`'s `CdpClient` connects seamlessly
//! while unlocking native `MiniBrowse.*` research, Web debugging, and Flutter VM capabilities.

use crate::fallback::MockRoute;
use crate::flutter_vm::{execute_flutter_vm_action, FlutterVmAction};
use crate::webview::{BrowserBackendCmd, BrowserBackendEvent, BrowserEngineHandle};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

pub struct MiniBrowseServer {
    listener: TcpListener,
    local_addr: SocketAddr,
    engine: BrowserEngineHandle,
    default_width: u32,
    default_height: u32,
    started_at: Instant,
}

impl MiniBrowseServer {
    pub async fn bind(
        port: u16,
        engine: BrowserEngineHandle,
        default_width: u32,
        default_height: u32,
    ) -> Result<Self, String> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| format!("Failed binding minibrowse on {addr}: {e}"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| format!("Failed reading local socket addr: {e}"))?;
        Ok(Self {
            listener,
            local_addr,
            engine,
            default_width,
            default_height,
            started_at: Instant::now(),
        })
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.local_addr.port()
    }

    pub async fn run(self) -> Result<(), String> {
        let port = self.local_addr.port();
        loop {
            let Ok((stream, _peer)) = self.listener.accept().await else {
                continue;
            };
            let engine = self.engine.clone();
            let def_w = self.default_width;
            let def_h = self.default_height;
            let started = self.started_at;
            tokio::spawn(async move {
                let _ = handle_connection(stream, port, engine, def_w, def_h, started).await;
            });
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    port: u16,
    engine: BrowserEngineHandle,
    default_width: u32,
    default_height: u32,
    started_at: Instant,
) -> Result<(), String> {
    let _ = stream.set_nodelay(true);
    let mut peek_buf = [0u8; 2048];
    let n = tokio::time::timeout(Duration::from_secs(5), stream.peek(&mut peek_buf))
        .await
        .map_err(|_| "Connection peek timed out".to_string())?
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(());
    }

    let head_preview = String::from_utf8_lossy(&peek_buf[..n]).to_ascii_lowercase();
    if head_preview.contains("upgrade: websocket") {
        return serve_websocket_session(stream, engine, default_width, default_height, started_at)
            .await;
    }

    // Standard HTTP discovery endpoint (/json/version, /json/list, /health)
    let mut req_buf = vec![0u8; 4096];
    let read_n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut req_buf))
        .await
        .map_err(|_| "HTTP read timed out".to_string())?
        .map_err(|e| e.to_string())?;
    let req_str = String::from_utf8_lossy(&req_buf[..read_n]);
    let first_line = req_str.lines().next().unwrap_or("");
    let raw_path = first_line.split_whitespace().nth(1).unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    let status_val = query_engine_status(&engine).await;
    let cur_url = status_val
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("about:blank");
    let cur_title = status_val
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("");
    let backend_name = status_val
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or("minibrowse");

    let (status_line, body_val) = match path {
        "/json/version" => (
            "HTTP/1.1 200 OK",
            json!({
                "Browser": format!("MiniBrowse/{}", env!("CARGO_PKG_VERSION")),
                "Protocol-Version": "1.3",
                "User-Agent": format!("MiniBrowse/{} ({backend_name})", env!("CARGO_PKG_VERSION")),
                "V8-Version": "JSC-MiniVM",
                "WebKit-Version": backend_name,
                "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/browser/minibrowse")
            }),
        ),
        "/json" | "/json/list" => (
            "HTTP/1.1 200 OK",
            json!([
                {
                    "id": "main",
                    "type": "page",
                    "title": cur_title,
                    "url": cur_url,
                    "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/main")
                }
            ]),
        ),
        "/json/new" => (
            "HTTP/1.1 200 OK",
            json!({
                "id": "main",
                "type": "page",
                "title": cur_title,
                "url": cur_url,
                "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/main")
            }),
        ),
        "/health" | "/status" => {
            let mut h = status_val.clone();
            if let Some(obj) = h.as_object_mut() {
                obj.insert(
                    "uptime_secs".to_string(),
                    json!(started_at.elapsed().as_secs()),
                );
                obj.insert("version".to_string(), json!(env!("CARGO_PKG_VERSION")));
            }
            ("HTTP/1.1 200 OK", h)
        }
        _ => (
            "HTTP/1.1 404 Not Found",
            json!({ "error": "Unknown endpoint", "path": path }),
        ),
    };

    let body_bytes = serde_json::to_vec(&body_val).unwrap_or_default();
    let header = format!(
        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    let _ = stream.write_all(header.as_bytes()).await;
    let _ = stream.write_all(&body_bytes).await;
    Ok(())
}

async fn query_engine_status(engine: &BrowserEngineHandle) -> Value {
    let (tx, rx) = oneshot::channel();
    if engine
        .send(BrowserBackendCmd::GetStatus { reply: tx })
        .is_ok()
    {
        if let Ok(Ok(v)) = tokio::time::timeout(Duration::from_secs(2), rx).await {
            return v;
        }
    }
    json!({
        "engine": engine.backend_name(),
        "url": "about:blank",
        "title": "",
    })
}

async fn serve_websocket_session(
    stream: TcpStream,
    engine: BrowserEngineHandle,
    default_width: u32,
    default_height: u32,
    started_at: Instant,
) -> Result<(), String> {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| format!("WebSocket handshake failed: {e}"))?;
    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    // Outbound writer loop
    tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if ws_write.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Forward asynchronous browser events (Console, NetworkError, Lifecycle) as CDP notifications
    let mut ev_rx = engine.subscribe_events();
    let ev_out_tx = out_tx.clone();
    let event_task = tokio::spawn(async move {
        while let Ok(ev) = ev_rx.recv().await {
            let cdp_event = match ev {
                BrowserBackendEvent::Console { level, text } => json!({
                    "method": "Runtime.consoleAPICalled",
                    "params": {
                        "type": level,
                        "args": [{ "type": "string", "value": text }]
                    }
                }),
                BrowserBackendEvent::NetworkError {
                    method,
                    url,
                    status,
                    error,
                } => {
                    if status >= 400 {
                        json!({
                            "method": "Network.responseReceived",
                            "params": {
                                "type": "Fetch",
                                "response": {
                                    "url": url,
                                    "status": status,
                                    "statusText": error,
                                    "requestHeaders": { ":method": method }
                                }
                            }
                        })
                    } else {
                        json!({
                            "method": "Network.loadingFailed",
                            "params": {
                                "type": method,
                                "errorText": format!("{url}: {error}"),
                                "canceled": false
                            }
                        })
                    }
                }
                BrowserBackendEvent::Lifecycle(name) => json!({
                    "method": "Page.lifecycleEvent",
                    "params": { "name": name }
                }),
            };
            if ev_out_tx
                .send(Message::Text(cdp_event.to_string()))
                .is_err()
            {
                break;
            }
        }
    });

    while let Some(msg_res) = ws_read.next().await {
        match msg_res {
            Ok(Message::Text(text)) => {
                let Ok(req) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let session_id = req
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let method = req
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

                let eng = engine.clone();
                let reply_tx = out_tx.clone();
                tokio::spawn(async move {
                    let outcome = dispatch_cdp_method(
                        &eng,
                        &method,
                        &params,
                        default_width,
                        default_height,
                        started_at,
                    )
                    .await;
                    let mut resp = match outcome {
                        Ok(result) => json!({
                            "id": id,
                            "result": result,
                        }),
                        Err((code, msg)) => json!({
                            "id": id,
                            "error": { "code": code, "message": msg }
                        }),
                    };
                    if let Some(sid) = session_id {
                        resp["sessionId"] = json!(sid);
                    }
                    let _ = reply_tx.send(Message::Text(resp.to_string()));
                });
            }
            Ok(Message::Ping(payload)) => {
                let _ = out_tx.send(Message::Pong(payload));
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    event_task.abort();
    Ok(())
}

async fn dispatch_cdp_method(
    engine: &BrowserEngineHandle,
    method: &str,
    params: &Value,
    default_width: u32,
    default_height: u32,
    started_at: Instant,
) -> Result<Value, (i32, String)> {
    match method {
        // ── Target & Session Lifecycle ───────────────────────────────────────
        "Target.createTarget" => {
            if let Some(u) = params.get("url").and_then(Value::as_str) {
                let trimmed = u.trim();
                if !trimmed.is_empty() && trimmed != "about:blank" {
                    let (tx, rx) = oneshot::channel();
                    engine
                        .send(BrowserBackendCmd::Navigate {
                            url: trimmed.to_string(),
                            reply: tx,
                        })
                        .map_err(|e| (-32000, e))?;
                    let _ = tokio::time::timeout(Duration::from_secs(15), rx).await;
                }
            }
            Ok(json!({ "targetId": "main" }))
        }
        "Target.attachToTarget" => Ok(json!({ "sessionId": "mb-session-1" })),
        "Target.getTargets" => {
            let st = query_engine_status(engine).await;
            Ok(json!({
                "targetInfos": [{
                    "targetId": "main",
                    "type": "page",
                    "title": st.get("title").cloned().unwrap_or_else(|| json!("")),
                    "url": st.get("url").cloned().unwrap_or_else(|| json!("about:blank")),
                    "attached": true
                }]
            }))
        }
        "Target.activateTarget" | "Target.closeTarget" => Ok(json!({ "success": true })),

        // ── Core Domain Enable / No-Op Acknowledgements ──────────────────────
        "Page.enable"
        | "Page.bringToFront"
        | "Page.setLifecycleEventsEnabled"
        | "Page.handleJavaScriptDialog"
        | "Runtime.enable"
        | "DOM.enable"
        | "Network.enable"
        | "Network.setCacheDisabled"
        | "Network.emulateNetworkConditions"
        | "Log.enable"
        | "Fetch.enable"
        | "Fetch.disable"
        | "Fetch.continueRequest"
        | "Fetch.fulfillRequest"
        | "Fetch.failRequest"
        | "Emulation.setTouchEmulationEnabled"
        | "Emulation.setEmulatedMedia"
        | "Emulation.setFocusEmulationEnabled" => Ok(json!({})),

        // ── Navigation & Scripts ─────────────────────────────────────────────
        "Page.addScriptToEvaluateOnNewDocument" => {
            let source = params
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::AddInitScript { source, reply: tx })
                .map_err(|e| (-32000, e))?;
            let _ = rx.await;
            Ok(json!({ "identifier": "1" }))
        }
        "Page.navigate" => {
            let url = params
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("about:blank")
                .to_string();
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::Navigate { url, reply: tx })
                .map_err(|e| (-32000, e))?;
            match tokio::time::timeout(Duration::from_secs(20), rx).await {
                Ok(Ok(Ok(val))) => Ok(val),
                Ok(Ok(Err(e))) => Err((-32000, e)),
                _ => Ok(json!({ "frameId": "main" })),
            }
        }
        "Page.reload" => {
            let ignore_cache = params
                .get("ignoreCache")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::Reload {
                    ignore_cache,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let _ = tokio::time::timeout(Duration::from_secs(15), rx).await;
            Ok(json!({}))
        }
        "Runtime.evaluate" => {
            let expression = params
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::EvaluateJs {
                    expression,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            match tokio::time::timeout(Duration::from_secs(15), rx).await {
                Ok(Ok(Ok(val))) => Ok(val),
                Ok(Ok(Err(e))) => Err((-32000, e)),
                _ => Err((
                    -32000,
                    "Timeout evaluating JavaScript in MiniBrowse".to_string(),
                )),
            }
        }

        // ── Screenshot, Viewport & Input ─────────────────────────────────────
        "Page.captureScreenshot" => {
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::CaptureScreenshotPng { reply: tx })
                .map_err(|e| (-32000, e))?;
            match tokio::time::timeout(Duration::from_secs(10), rx).await {
                Ok(Ok(Ok(b64))) => Ok(json!({ "data": b64 })),
                Ok(Ok(Err(e))) => Err((-32000, e)),
                _ => Err((
                    -32000,
                    "Timeout capturing MiniBrowse PNG snapshot".to_string(),
                )),
            }
        }
        "Emulation.setDeviceMetricsOverride" => {
            let width = params
                .get("width")
                .and_then(Value::as_u64)
                .unwrap_or(u64::from(default_width)) as u32;
            let height = params
                .get("height")
                .and_then(Value::as_u64)
                .unwrap_or(u64::from(default_height)) as u32;
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::SetViewport {
                    width,
                    height,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let _ = rx.await;
            Ok(json!({}))
        }
        "Emulation.clearDeviceMetricsOverride" => {
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::SetViewport {
                    width: default_width,
                    height: default_height,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let _ = rx.await;
            Ok(json!({}))
        }
        "Input.dispatchMouseEvent" => {
            let event_type = params
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("mouseMoved")
                .to_string();
            let x = params.get("x").and_then(Value::as_f64).unwrap_or(0.0);
            let y = params.get("y").and_then(Value::as_f64).unwrap_or(0.0);
            let delta_x = params.get("deltaX").and_then(Value::as_f64).unwrap_or(0.0);
            let delta_y = params.get("deltaY").and_then(Value::as_f64).unwrap_or(0.0);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::DispatchMouse {
                    event_type,
                    x,
                    y,
                    delta_x,
                    delta_y,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let _ = rx.await;
            Ok(json!({}))
        }
        "Input.dispatchKeyEvent" => {
            let event_type = params
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("keyDown")
                .to_string();
            let key = params
                .get("key")
                .or_else(|| params.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let code = params
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or(&key)
                .to_string();
            let modifiers = params.get("modifiers").and_then(Value::as_u64).unwrap_or(0) as u32;
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::DispatchKey {
                    event_type,
                    key,
                    code,
                    modifiers,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let _ = rx.await;
            Ok(json!({}))
        }
        "Network.getCookies" => {
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::EvaluateJs {
                    expression: "document.cookie".to_string(),
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let raw_cookie = rx
                .await
                .ok()
                .and_then(Result::ok)
                .and_then(|v| {
                    v.pointer("/result/value")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            let cookies: Vec<Value> = raw_cookie
                .split(';')
                .filter_map(|pair| {
                    let (k, v) = pair.trim().split_once('=')?;
                    (!k.is_empty()).then(|| json!({ "name": k.trim(), "value": v.trim() }))
                })
                .collect();
            Ok(json!({ "cookies": cookies }))
        }
        "Network.setCookies" => {
            if let Some(arr) = params.get("cookies").and_then(Value::as_array) {
                for c in arr {
                    if let (Some(name), Some(val)) = (
                        c.get("name").and_then(Value::as_str),
                        c.get("value").and_then(Value::as_str),
                    ) {
                        let assign = format!("{name}={val}; path=/");
                        if let Ok(assign_json) = serde_json::to_string(&assign) {
                            let (tx, _rx) = oneshot::channel();
                            let _ = engine.send(BrowserBackendCmd::EvaluateJs {
                                expression: format!("document.cookie = {assign_json}"),
                                reply: tx,
                            });
                        }
                    }
                }
            }
            Ok(json!({}))
        }

        // ── Native `MiniBrowse.*` Agentic Domain ─────────────────────────────
        "MiniBrowse.status" => {
            let mut st = query_engine_status(engine).await;
            if let Some(obj) = st.as_object_mut() {
                obj.insert(
                    "uptime_secs".to_string(),
                    json!(started_at.elapsed().as_secs()),
                );
            }
            Ok(st)
        }
        "MiniBrowse.setMockRoutes" => {
            let routes: Vec<MockRoute> = params
                .get("routes")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::SetMockRoutes { routes, reply: tx })
                .map_err(|e| (-32000, e))?;
            let count = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(json!({ "mock_routes": count }))
        }
        "MiniBrowse.setBlockedUrls" => {
            let patterns: Vec<String> = params
                .get("patterns")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::SetBlockedPatterns {
                    patterns,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let count = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(json!({ "blocked_patterns": count }))
        }
        "MiniBrowse.setAdBlock" => {
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::SetAdBlock { enabled, reply: tx })
                .map_err(|e| (-32000, e))?;
            let active = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(json!({ "adblock_enabled": active }))
        }
        "MiniBrowse.researchExtract" => {
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .map(str::to_string);
            let max_chars = params
                .get("maxChars")
                .and_then(Value::as_u64)
                .unwrap_or(12_000) as usize;
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::ResearchExtract {
                    query,
                    max_chars,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .map_err(|_| (-32000, "Timeout in researchExtract".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.webDebugAudit" => {
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::WebDebugAudit { reply: tx })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .map_err(|_| (-32000, "Timeout in webDebugAudit".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.uiDiff" => {
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .map(str::to_string);
            let max_chars = params
                .get("maxChars")
                .and_then(Value::as_u64)
                .unwrap_or(8_000) as usize;
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::UiDiff {
                    query,
                    max_chars,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .map_err(|_| (-32000, "Timeout in uiDiff".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.networkWaterfall" => {
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::NetworkWaterfall { reply: tx })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(8), rx)
                .await
                .map_err(|_| (-32000, "Timeout in networkWaterfall".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.storageInspect" => {
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::StorageInspect { reply: tx })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(8), rx)
                .await
                .map_err(|_| (-32000, "Timeout in storageInspect".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.flutterInspect" => {
            let vm_action = FlutterVmAction::from_params(params);
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .map(str::to_string);
            let (ui_tx, ui_rx) = oneshot::channel();
            let _ = engine.send(BrowserBackendCmd::UiDiff {
                query,
                max_chars: 8_000,
                reply: ui_tx,
            });
            let ui_tree = tokio::time::timeout(Duration::from_secs(5), ui_rx)
                .await
                .ok()
                .and_then(Result::ok)
                .and_then(Result::ok)
                .unwrap_or(Value::Null);

            let vm_result = if let Some(vm_url) = params
                .get("vmUrl")
                .or_else(|| params.get("vm_url"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                match execute_flutter_vm_action(vm_url, vm_action).await {
                    Ok(v) => v,
                    Err(e) => json!({ "connected": false, "error": e }),
                }
            } else {
                json!({
                    "connected": false,
                    "hint": "Pass vm_url (e.g. from `flutter run --machine` or Dart VM Service port) to inspect .dart widget source lines, trigger sub-second reloadSources, or toggle debugPaint."
                })
            };

            Ok(json!({
                "semantics_ui": ui_tree,
                "dart_vm": vm_result,
            }))
        }

        // Unsupported optional methods (e.g. Page.printToPDF) return standard -32601 so CdpClient's
        // `is_unsupported_cdp_error` fallback triggers cleanly!
        other => Err((
            -32601,
            format!("Method '{other}' is not supported by MiniBrowse"),
        )),
    }
}
