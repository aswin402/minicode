//! Single-Port CDP (`1.3`) + `MiniBrowse.*` HTTP Discovery & WebSocket Server.
//!
//! Multiplexes HTTP `/json/version`, `/json/list`, `/health` and WebSocket `/devtools/...`
//! on a single loopback port (`127.0.0.1:<port>`) so `minicode`'s `CdpClient` connects seamlessly
//! while unlocking native `MiniBrowse.*` research, Web debugging, and Flutter VM capabilities.

use crate::fallback::MockRoute;
use crate::flutter_vm::{execute_flutter_vm_action, FlutterVmAction};
use crate::net_policy::ResourceProfile;
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

    let raw_head = String::from_utf8_lossy(&peek_buf[..n]);
    if raw_head.to_ascii_lowercase().contains("upgrade: websocket") {
        let ws_target_hint = extract_ws_target_hint(&raw_head);
        return serve_websocket_session(
            stream,
            engine,
            default_width,
            default_height,
            started_at,
            ws_target_hint,
        )
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

    let status_val = query_engine_status(&engine, None).await;
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
        "/json" | "/json/list" => {
            let (tx, rx) = oneshot::channel();
            let items = if engine
                .send(BrowserBackendCmd::ListTargets { reply: tx })
                .is_ok()
            {
                tokio::time::timeout(Duration::from_secs(2), rx)
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let formatted: Vec<Value> = if items.is_empty() {
                vec![json!({
                    "id": "main",
                    "type": "page",
                    "title": cur_title,
                    "url": cur_url,
                    "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/main")
                })]
            } else {
                items
                    .into_iter()
                    .map(|t| {
                        let tid = t
                            .get("targetId")
                            .and_then(Value::as_str)
                            .unwrap_or("main");
                        json!({
                            "id": tid,
                            "type": "page",
                            "title": t.get("title").cloned().unwrap_or_else(|| json!("")),
                            "url": t.get("url").cloned().unwrap_or_else(|| json!("about:blank")),
                            "browserContextId": t.get("browserContextId").cloned().unwrap_or(Value::Null),
                            "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/{tid}")
                        })
                    })
                    .collect()
            };
            ("HTTP/1.1 200 OK", Value::Array(formatted))
        }
        "/json/new" => {
            let (tx, rx) = oneshot::channel();
            let tid = if engine
                .send(BrowserBackendCmd::CreateTarget {
                    url: None,
                    context_id: None,
                    background: false,
                    reply: tx,
                })
                .is_ok()
            {
                tokio::time::timeout(Duration::from_secs(3), rx)
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .and_then(Result::ok)
                    .unwrap_or_else(|| "main".to_string())
            } else {
                "main".to_string()
            };
            (
                "HTTP/1.1 200 OK",
                json!({
                    "id": tid,
                    "type": "page",
                    "title": "",
                    "url": "about:blank",
                    "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/{tid}")
                }),
            )
        }
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

fn extract_ws_target_hint(raw_head: &str) -> Option<String> {
    let first_line = raw_head.lines().next()?;
    let raw_path = first_line.split_whitespace().nth(1)?;
    let clean_path = raw_path.split('?').next().unwrap_or(raw_path);
    let tid = clean_path.strip_prefix("/devtools/page/")?.trim();
    (!tid.is_empty() && tid != "browser" && tid != "minibrowse").then(|| tid.to_string())
}

fn send_to_target(
    engine: &BrowserEngineHandle,
    target_scope: Option<&str>,
    cmd: BrowserBackendCmd,
) -> Result<(), (i32, String)> {
    let wrapped = match target_scope.filter(|s| !s.is_empty()) {
        Some(tid) => BrowserBackendCmd::ScopedTarget {
            target_id: tid.to_string(),
            cmd: Box::new(cmd),
        },
        None => cmd,
    };
    engine.send(wrapped).map_err(|e| (-32000, e))
}

fn infer_mime_type(path: &std::path::Path) -> &'static str {
    const EXT_TABLE: &[(&str, &str)] = &[
        ("csv", "text/csv"),
        ("gif", "image/gif"),
        ("htm", "text/html"),
        ("html", "text/html"),
        ("jpeg", "image/jpeg"),
        ("jpg", "image/jpeg"),
        ("json", "application/json"),
        ("md", "text/markdown"),
        ("pdf", "application/pdf"),
        ("png", "image/png"),
        ("svg", "image/svg+xml"),
        ("txt", "text/plain"),
        ("webp", "image/webp"),
        ("xml", "application/xml"),
        ("zip", "application/zip"),
    ];
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    EXT_TABLE
        .binary_search_by(|(k, _)| (*k).cmp(ext.as_str()))
        .ok()
        .map_or("application/octet-stream", |idx| EXT_TABLE[idx].1)
}

async fn query_engine_status(engine: &BrowserEngineHandle, target_scope: Option<&str>) -> Value {
    let (tx, rx) = oneshot::channel();
    if send_to_target(
        engine,
        target_scope,
        BrowserBackendCmd::GetStatus { reply: tx },
    )
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
    ws_target_hint: Option<String>,
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
                let target_scope = session_id
                    .as_deref()
                    .and_then(|s| s.strip_prefix("mb-session-"))
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .or_else(|| ws_target_hint.clone());
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
                        target_scope.as_deref(),
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
    target_scope: Option<&str>,
    default_width: u32,
    default_height: u32,
    started_at: Instant,
) -> Result<Value, (i32, String)> {
    match method {
        // ── Target & Isolated BrowserContext Lifecycle ──────────────────────
        "Target.createBrowserContext" => {
            let proxy_server = params
                .get("proxyServer")
                .or_else(|| params.get("proxy_server"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::CreateBrowserContext {
                    proxy_server,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let cid = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?;
            Ok(json!({ "browserContextId": cid }))
        }
        "Target.disposeBrowserContext" => {
            let cid = params
                .get("browserContextId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::DisposeBrowserContext {
                    context_id: cid,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let _ = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?;
            Ok(json!({}))
        }
        "Target.getBrowserContexts" => {
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::ListBrowserContexts { reply: tx })
                .map_err(|e| (-32000, e))?;
            let ids = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?;
            Ok(json!({ "browserContextIds": ids }))
        }
        "Target.createTarget" => {
            let url = params
                .get("url")
                .and_then(Value::as_str)
                .map(str::to_string);
            let context_id = params
                .get("browserContextId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let background = params
                .get("background")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::CreateTarget {
                    url,
                    context_id,
                    background,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let tid = tokio::time::timeout(Duration::from_secs(15), rx)
                .await
                .map_err(|_| (-32000, "Timeout creating target".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(json!({ "targetId": tid }))
        }
        "Target.attachToTarget" => {
            let tid = params
                .get("targetId")
                .and_then(Value::as_str)
                .unwrap_or("main")
                .to_string();
            Ok(json!({ "sessionId": format!("mb-session-{tid}") }))
        }
        "Target.getTargets" => {
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::ListTargets { reply: tx })
                .map_err(|e| (-32000, e))?;
            let infos = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?;
            Ok(json!({ "targetInfos": infos }))
        }
        "Target.activateTarget" => {
            let tid = params
                .get("targetId")
                .and_then(Value::as_str)
                .unwrap_or("main")
                .to_string();
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::ActivateTarget {
                    target_id: tid,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            rx.await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(json!({ "success": true }))
        }
        "Target.closeTarget" => {
            let tid = params
                .get("targetId")
                .and_then(Value::as_str)
                .unwrap_or("main")
                .to_string();
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::CloseTarget {
                    target_id: tid,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let closed = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(json!({ "success": closed }))
        }
        "Target.setDiscoverTargets" | "Target.setAutoAttach" | "Target.detachFromTarget" => {
            Ok(json!({}))
        }

        // ── Core Domain Enable / No-Op Acknowledgements ──────────────────────
        "Page.enable"
        | "Page.bringToFront"
        | "Page.setLifecycleEventsEnabled"
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

        "Page.handleJavaScriptDialog" | "MiniBrowse.configureDialog" => {
            let accept = params
                .get("accept")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let prompt_text = params
                .get("promptText")
                .or_else(|| params.get("prompt_text"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::ConfigureDialog {
                    accept,
                    prompt_text,
                    reply: tx,
                },
            )?;
            let _ = rx.await;
            Ok(json!({}))
        }

        // ── Navigation, History & Scripts ────────────────────────────────────
        "Page.addScriptToEvaluateOnNewDocument" => {
            let source = params
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::AddInitScript { source, reply: tx },
            )?;
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
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::Navigate { url, reply: tx },
            )?;
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
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::Reload {
                    ignore_cache,
                    reply: tx,
                },
            )?;
            let _ = tokio::time::timeout(Duration::from_secs(15), rx).await;
            Ok(json!({}))
        }
        "Page.setDocumentContent" | "MiniBrowse.setContent" => {
            let html = params
                .get("html")
                .or_else(|| params.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetDocumentContent { html, reply: tx },
            )?;
            tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .map_err(|_| (-32000, "Timeout in setDocumentContent".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(json!({}))
        }
        "Page.getNavigationHistory" => {
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::GetHistory { reply: tx },
            )?;
            let hist = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?;
            Ok(hist)
        }
        "Page.navigateToHistoryEntry" => {
            let entry_id = params.get("entryId").and_then(Value::as_i64).unwrap_or(0) as isize;
            let (htx, hrx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::GetHistory { reply: htx },
            )?;
            let hist = hrx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?;
            let cur_idx = hist
                .get("currentIndex")
                .and_then(Value::as_i64)
                .unwrap_or(0) as isize;
            let delta = entry_id - cur_idx;
            if delta != 0 {
                let (ntx, nrx) = oneshot::channel();
                send_to_target(
                    engine,
                    target_scope,
                    BrowserBackendCmd::NavigateHistoryDelta { delta, reply: ntx },
                )?;
                let _ = tokio::time::timeout(Duration::from_secs(10), nrx).await;
            }
            Ok(json!({}))
        }
        "MiniBrowse.goBack" | "MiniBrowse.goForward" => {
            let delta = if method == "MiniBrowse.goBack" { -1 } else { 1 };
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::NavigateHistoryDelta { delta, reply: tx },
            )?;
            let moved = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .map_err(|_| (-32000, "Timeout navigating history".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(json!({ "navigated": moved }))
        }
        "Runtime.evaluate" => {
            let expression = params
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::EvaluateJs {
                    expression,
                    reply: tx,
                },
            )?;
            match tokio::time::timeout(Duration::from_secs(15), rx).await {
                Ok(Ok(Ok(val))) => Ok(val),
                Ok(Ok(Err(e))) => Err((-32000, e)),
                _ => Err((
                    -32000,
                    "Timeout evaluating JavaScript in MiniBrowse".to_string(),
                )),
            }
        }

        // ── Screenshot, PDF, File Upload, Viewport & Input ───────────────────
        "Page.captureScreenshot" | "MiniBrowse.captureScreenshot" => {
            let if_changed = params
                .get("ifChanged")
                .or_else(|| params.get("if_changed"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let annotate = params
                .get("annotate")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let threshold_bits = params
                .get("thresholdBits")
                .or_else(|| params.get("threshold_bits"))
                .and_then(Value::as_u64)
                .unwrap_or(1) as u32;
            let boxes: Vec<crate::vision::AnnotationBox> = params
                .get("boxes")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();

            if if_changed
                || annotate
                || !boxes.is_empty()
                || method == "MiniBrowse.captureScreenshot"
            {
                let (tx, rx) = oneshot::channel();
                send_to_target(
                    engine,
                    target_scope,
                    BrowserBackendCmd::CaptureScreenshotEx {
                        if_changed,
                        threshold_bits,
                        annotate,
                        boxes,
                        reply: tx,
                    },
                )?;
                return match tokio::time::timeout(Duration::from_secs(12), rx).await {
                    Ok(Ok(Ok(val))) => Ok(val),
                    Ok(Ok(Err(e))) => Err((-32000, e)),
                    _ => Err((
                        -32000,
                        "Timeout capturing extended MiniBrowse screenshot".to_string(),
                    )),
                };
            }

            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::CaptureScreenshotPng { reply: tx },
            )?;
            match tokio::time::timeout(Duration::from_secs(10), rx).await {
                Ok(Ok(Ok(b64))) => Ok(json!({ "data": b64 })),
                Ok(Ok(Err(e))) => Err((-32000, e)),
                _ => Err((
                    -32000,
                    "Timeout capturing MiniBrowse PNG snapshot".to_string(),
                )),
            }
        }
        "Page.printToPDF" => {
            let landscape = params
                .get("landscape")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::PrintToPdf {
                    landscape,
                    reply: tx,
                },
            )?;
            match tokio::time::timeout(Duration::from_secs(15), rx).await {
                Ok(Ok(Ok(b64))) => Ok(json!({ "data": b64 })),
                Ok(Ok(Err(e))) => Err((-32000, e)),
                _ => Err((-32000, "Timeout exporting PDF in MiniBrowse".to_string())),
            }
        }
        "DOM.setFileInputFiles" | "MiniBrowse.setFileInput" => {
            let selector = params
                .get("selector")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);
            let raw_files: Vec<String> = params
                .get("files")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let mut specs = Vec::with_capacity(raw_files.len());
            for path_str in raw_files {
                let p = std::path::Path::new(&path_str);
                let bytes = tokio::fs::read(p).await.map_err(|e| {
                    (
                        -32000,
                        format!("Failed reading upload file '{path_str}': {e}"),
                    )
                })?;
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("upload.bin")
                    .to_string();
                let mime = infer_mime_type(p);
                let b64 = crate::ffi::base64_encode(&bytes);
                specs.push(json!({
                    "name": name,
                    "mime": mime,
                    "base64": b64,
                }));
            }
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetFileInputFiles {
                    selector,
                    specs,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .map_err(|_| (-32000, "Timeout setting file input files".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
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
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetViewport {
                    width,
                    height,
                    overridden: true,
                    reply: tx,
                },
            )?;
            let _ = rx.await;
            Ok(json!({}))
        }
        "Emulation.clearDeviceMetricsOverride" => {
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetViewport {
                    width: default_width,
                    height: default_height,
                    overridden: false,
                    reply: tx,
                },
            )?;
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
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::DispatchMouse {
                    event_type,
                    x,
                    y,
                    delta_x,
                    delta_y,
                    reply: tx,
                },
            )?;
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
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::DispatchKey {
                    event_type,
                    key,
                    code,
                    modifiers,
                    reply: tx,
                },
            )?;
            let _ = rx.await;
            Ok(json!({}))
        }
        "Network.getCookies" => {
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::GetCookies { reply: tx },
            )?;
            let cookies = rx
                .await
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(json!({ "cookies": cookies }))
        }
        "Network.setCookies" => {
            let mut pairs = Vec::new();
            if let Some(arr) = params.get("cookies").and_then(Value::as_array) {
                for c in arr {
                    if let (Some(name), Some(val)) = (
                        c.get("name").and_then(Value::as_str),
                        c.get("value").and_then(Value::as_str),
                    ) {
                        pairs.push((name.to_string(), val.to_string()));
                    }
                }
            }
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetCookies {
                    cookies: pairs,
                    reply: tx,
                },
            )?;
            let _ = rx.await;
            Ok(json!({}))
        }

        // ── Native `MiniBrowse.*` Agentic Domain ─────────────────────────────
        "MiniBrowse.waitForIdle" => {
            let quiet_ms = params.get("quietMs").and_then(Value::as_u64).unwrap_or(25);
            let timeout_ms = params
                .get("timeoutMs")
                .and_then(Value::as_u64)
                .unwrap_or(600);
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::WaitForIdle {
                    quiet_ms,
                    timeout_ms,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_millis(timeout_ms + 1500), rx)
                .await
                .map_err(|_| (-32000, "Timeout in waitForIdle".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.waitForSelector" => {
            let selector = params
                .get("selector")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let state_mode = params
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("attached")
                .to_string();
            let timeout_ms = params
                .get("timeoutMs")
                .or_else(|| params.get("timeout_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(5000);
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::WaitForSelector {
                    selector,
                    state_mode,
                    timeout_ms,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_millis(timeout_ms + 1500), rx)
                .await
                .map_err(|_| (-32000, "Timeout in waitForSelector".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.visualDiff" => {
            let baseline_name = params
                .get("baseline")
                .and_then(Value::as_str)
                .unwrap_or("default")
                .to_string();
            let save_only = params
                .get("saveOnly")
                .or_else(|| params.get("save_only"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let url_a = params
                .get("urlA")
                .or_else(|| params.get("url_a"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);
            let url_b = params
                .get("urlB")
                .or_else(|| params.get("url_b"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);

            if let (Some(ua), Some(ub)) = (url_a, url_b) {
                let tmp_key = format!("__url_diff_{baseline_name}");
                let (n1_tx, n1_rx) = oneshot::channel();
                send_to_target(
                    engine,
                    target_scope,
                    BrowserBackendCmd::Navigate {
                        url: ua.clone(),
                        reply: n1_tx,
                    },
                )?;
                let _ = tokio::time::timeout(Duration::from_secs(15), n1_rx).await;

                let (s_tx, s_rx) = oneshot::channel();
                send_to_target(
                    engine,
                    target_scope,
                    BrowserBackendCmd::VisualDiff {
                        baseline_name: tmp_key.clone(),
                        save_only: true,
                        reply: s_tx,
                    },
                )?;
                let _ = tokio::time::timeout(Duration::from_secs(10), s_rx).await;

                let (n2_tx, n2_rx) = oneshot::channel();
                send_to_target(
                    engine,
                    target_scope,
                    BrowserBackendCmd::Navigate {
                        url: ub.clone(),
                        reply: n2_tx,
                    },
                )?;
                let _ = tokio::time::timeout(Duration::from_secs(15), n2_rx).await;

                let (c_tx, c_rx) = oneshot::channel();
                send_to_target(
                    engine,
                    target_scope,
                    BrowserBackendCmd::VisualDiff {
                        baseline_name: tmp_key,
                        save_only: false,
                        reply: c_tx,
                    },
                )?;
                let mut diff_val = tokio::time::timeout(Duration::from_secs(12), c_rx)
                    .await
                    .map_err(|_| (-32000, "Timeout in visualDiff URL comparison".to_string()))?
                    .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                    .map_err(|e| (-32000, e))?;
                if let Some(obj) = diff_val.as_object_mut() {
                    obj.insert("url_a".to_string(), json!(ua));
                    obj.insert("url_b".to_string(), json!(ub));
                }
                return Ok(diff_val);
            }

            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::VisualDiff {
                    baseline_name,
                    save_only,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(12), rx)
                .await
                .map_err(|_| (-32000, "Timeout in visualDiff".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.setResourceProfile" => {
            let profile_str = params
                .get("profile")
                .and_then(Value::as_str)
                .unwrap_or("full");
            let profile = if profile_str.eq_ignore_ascii_case("research")
                || profile_str.eq_ignore_ascii_case("lean")
            {
                ResourceProfile::ResearchLean
            } else {
                ResourceProfile::Full
            };
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetResourceProfile { profile, reply: tx },
            )?;
            let _ = rx.await;
            Ok(json!({ "profile": profile_str }))
        }
        "MiniBrowse.status" => {
            let mut st = query_engine_status(engine, target_scope).await;
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
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetMockRoutes { routes, reply: tx },
            )?;
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
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetBlockedPatterns {
                    patterns,
                    reply: tx,
                },
            )?;
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
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetAdBlock { enabled, reply: tx },
            )?;
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
            let format = params
                .get("format")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::ResearchExtract {
                    query,
                    max_chars,
                    format,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .map_err(|_| (-32000, "Timeout in researchExtract".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.webDebugAudit" => {
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::WebDebugAudit { reply: tx },
            )?;
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
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::UiDiff {
                    query,
                    max_chars,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .map_err(|_| (-32000, "Timeout in uiDiff".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.networkWaterfall" => {
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::NetworkWaterfall { reply: tx },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(8), rx)
                .await
                .map_err(|_| (-32000, "Timeout in networkWaterfall".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.apiTraffic" => {
            let filter = params
                .get("filter")
                .or_else(|| params.get("pattern"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);
            let only_errors = params
                .get("onlyErrors")
                .or_else(|| params.get("only_errors"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::ApiTraffic {
                    filter,
                    only_errors,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(8), rx)
                .await
                .map_err(|_| (-32000, "Timeout in apiTraffic".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.storageInspect" => {
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::StorageInspect { reply: tx },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(8), rx)
                .await
                .map_err(|_| (-32000, "Timeout in storageInspect".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.extractTables" => {
            let selector = params
                .get("selector")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::ExtractTables {
                    selector,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(8), rx)
                .await
                .map_err(|_| (-32000, "Timeout in extractTables".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.setWindowMode" => {
            let mode_str = params.get("mode").and_then(Value::as_str).unwrap_or("gui");
            let gui = !mode_str.eq_ignore_ascii_case("headless");
            let steal_focus = params
                .get("stealFocus")
                .or_else(|| params.get("steal_focus"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetWindowMode {
                    gui,
                    steal_focus,
                    title,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .map_err(|_| (-32000, "Timeout in setWindowMode".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.openOrReuseTab" => {
            let url = params
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("about:blank")
                .to_string();
            let context_id = params
                .get("browserContextId")
                .or_else(|| params.get("contextId"))
                .or_else(|| params.get("context_id"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::OpenOrReuseTab {
                    url,
                    context_id,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(15), rx)
                .await
                .map_err(|_| (-32000, "Timeout in openOrReuseTab".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.suspendIdleTabs" => {
            let idle_secs = params
                .get("idleSecs")
                .or_else(|| params.get("idle_secs"))
                .and_then(Value::as_u64)
                .unwrap_or(60);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::SuspendIdleTabs {
                    idle_secs,
                    reply: tx,
                })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(6), rx)
                .await
                .map_err(|_| (-32000, "Timeout in suspendIdleTabs".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?;
            Ok(val)
        }
        "MiniBrowse.setMediaDiet" => {
            let enabled = params.get("enabled").and_then(Value::as_bool);
            let audio_only = params
                .get("audioOnly")
                .or_else(|| params.get("audio_only"))
                .and_then(Value::as_bool);
            let (tx, rx) = oneshot::channel();
            send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::SetMediaDiet {
                    enabled,
                    audio_only,
                    reply: tx,
                },
            )?;
            let val = tokio::time::timeout(Duration::from_secs(6), rx)
                .await
                .map_err(|_| (-32000, "Timeout in setMediaDiet".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.setWebGL" => {
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::SetWebGl { enabled, reply: tx })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(6), rx)
                .await
                .map_err(|_| (-32000, "Timeout in setWebGL".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.freezeGui" => {
            let freeze = params
                .get("freeze")
                .or_else(|| params.get("enabled"))
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::FreezeGui { freeze, reply: tx })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(6), rx)
                .await
                .map_err(|_| (-32000, "Timeout in freezeGui".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?
                .map_err(|e| (-32000, e))?;
            Ok(val)
        }
        "MiniBrowse.trimMemory" => {
            let hard = params.get("hard").and_then(Value::as_bool).unwrap_or(false);
            let (tx, rx) = oneshot::channel();
            engine
                .send(BrowserBackendCmd::TrimMemory { hard, reply: tx })
                .map_err(|e| (-32000, e))?;
            let val = tokio::time::timeout(Duration::from_secs(8), rx)
                .await
                .map_err(|_| (-32000, "Timeout in trimMemory".to_string()))?
                .map_err(|_| (-32000, "Actor channel closed".to_string()))?;
            Ok(val)
        }
        "MiniBrowse.flutterInspect" => {
            let vm_action = FlutterVmAction::from_params(params);
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .map(str::to_string);
            let (ui_tx, ui_rx) = oneshot::channel();
            let _ = send_to_target(
                engine,
                target_scope,
                BrowserBackendCmd::UiDiff {
                    query,
                    max_chars: 8_000,
                    reply: ui_tx,
                },
            );
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

        // Unsupported optional methods return standard -32601 so CdpClient's
        // `is_unsupported_cdp_error` fallback triggers cleanly!
        other => Err((
            -32601,
            format!("Method '{other}' is not supported by MiniBrowse"),
        )),
    }
}
