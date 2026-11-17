//! W3C WebDriver HTTP (`safaridriver` / `geckodriver`) and W3C WebDriver BiDi (`Firefox 141+`)
//! protocol adapters for `CdpClient`.
//!
//! Enables `minicode` to drive **Safari** (macOS `safaridriver`) and **Firefox 141+**
//! (`--remote-debugging-port` WebDriver BiDi or `geckodriver`) through the exact same
//! `CdpClient` interface used for `MiniBrowse` and `Chrome`.

use super::debug::{DebugCollector, LogLevel};
use super::engine::{BrowserEngine, BrowserMode};
use crate::error::{Result, ToolError};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;

/// Unified non-CDP browser transport backing `CdpClient` for Safari and modern Firefox.
pub enum WebDriverAdapter {
    /// W3C WebDriver HTTP REST (`safaridriver -p <port>`, `WebKitWebDriver`, or `geckodriver --port <port>`)
    W3cHttp(W3cHttpDriver),
    /// W3C WebDriver BiDi over WebSocket (`firefox --remote-debugging-port <port>` at `ws://127.0.0.1:<port>/session`)
    BidiWs(BidiWsDriver),
}

pub struct W3cHttpDriver {
    http: reqwest::Client,
    base_url: String,
    session_id: String,
    primary_handle: Mutex<String>,
    active_handle: Mutex<String>,
    preload_scripts: Mutex<Vec<String>>,
    lifecycle_tx: broadcast::Sender<String>,
}

pub struct BidiWsDriver {
    tx: mpsc::UnboundedSender<Message>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    primary_context: Mutex<String>,
    lifecycle_tx: broadcast::Sender<String>,
}

impl WebDriverAdapter {
    /// Connects to a W3C WebDriver HTTP endpoint (`safaridriver` / `geckodriver`).
    pub async fn connect_w3c_http(
        base_url: &str,
        engine: BrowserEngine,
        mode: BrowserMode,
        lifecycle_tx: broadcast::Sender<String>,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| ToolError::CommandExec(format!("WebDriver HTTP client error: {e}")))?;

        let caps = match engine {
            BrowserEngine::Safari => json!({
                "capabilities": {
                    "alwaysMatch": {
                        "browserName": "safari"
                    }
                }
            }),
            BrowserEngine::Firefox => {
                let ff_args = if mode == BrowserMode::Headless {
                    vec!["-headless"]
                } else {
                    vec![]
                };
                json!({
                    "capabilities": {
                        "alwaysMatch": {
                            "browserName": "firefox",
                            "moz:firefoxOptions": { "args": ff_args }
                        }
                    }
                })
            }
            _ => json!({ "capabilities": { "alwaysMatch": {} } }),
        };

        let url = format!("{}/session", base_url.trim_end_matches('/'));
        let resp =
            http.post(&url).json(&caps).send().await.map_err(|e| {
                ToolError::CommandExec(format!("WebDriver session init failed: {e}"))
            })?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| ToolError::CommandExec(format!("WebDriver session JSON error: {e}")))?;

        if !status.is_success() {
            let msg = body
                .pointer("/value/message")
                .and_then(Value::as_str)
                .unwrap_or("Failed creating WebDriver session");
            return Err(ToolError::CommandExec(format!("WebDriver ({engine}): {msg}")).into());
        }

        let session_id = body
            .pointer("/value/sessionId")
            .or_else(|| body.get("sessionId"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::CommandExec("WebDriver response missing sessionId".to_string())
            })?
            .to_string();

        let handle_url = format!(
            "{}/session/{}/window",
            base_url.trim_end_matches('/'),
            session_id
        );
        let initial_handle = match http.get(&handle_url).send().await {
            Ok(r) => r
                .json::<Value>()
                .await
                .ok()
                .and_then(|v| v.get("value").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| "default".to_string()),
            Err(_) => "default".to_string(),
        };

        Ok(Self::W3cHttp(W3cHttpDriver {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            session_id,
            primary_handle: Mutex::new(initial_handle.clone()),
            active_handle: Mutex::new(initial_handle),
            preload_scripts: Mutex::new(Vec::new()),
            lifecycle_tx,
        }))
    }

    /// Connects to a W3C WebDriver BiDi WebSocket endpoint (`ws://127.0.0.1:<port>/session`, used by Firefox 141+).
    pub async fn connect_bidi_ws(
        http_base: &str,
        console_log: Arc<Mutex<Vec<String>>>,
        debug_collector: Arc<DebugCollector>,
        lifecycle_tx: broadcast::Sender<String>,
    ) -> Result<Self> {
        let ws_base = if let Some(rest) = http_base.strip_prefix("https://") {
            format!("wss://{}", rest.trim_end_matches('/'))
        } else if let Some(rest) = http_base.strip_prefix("http://") {
            format!("ws://{}", rest.trim_end_matches('/'))
        } else {
            format!("ws://{}", http_base.trim_end_matches('/'))
        };
        let session_ws_url = format!("{ws_base}/session");

        let (ws_stream, _) = tokio_tungstenite::connect_async(&session_ws_url)
            .await
            .map_err(|e| {
                ToolError::CommandExec(format!(
                    "Failed connecting to WebDriver BiDi '{session_ws_url}': {e}"
                ))
            })?;

        let (mut write, mut read) = ws_stream.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_reader = Arc::clone(&pending);

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if write.send(msg).await.is_err() {
                    break;
                }
            }
        });

        let lifecycle_reader = lifecycle_tx.clone();
        tokio::spawn(async move {
            while let Some(msg_res) = read.next().await {
                match msg_res {
                    Ok(Message::Text(text)) => {
                        let Ok(val) = serde_json::from_str::<Value>(&text) else {
                            continue;
                        };
                        if let Some(id) = val.get("id").and_then(Value::as_u64) {
                            let mut map = pending_reader.lock().await;
                            if let Some(sender) = map.remove(&id) {
                                let _ = sender.send(val);
                            }
                        } else if let Some(method) = val.get("method").and_then(Value::as_str) {
                            match method {
                                "log.entryAdded" => {
                                    let level = val
                                        .pointer("/params/level")
                                        .and_then(Value::as_str)
                                        .unwrap_or("info");
                                    let text_msg = val
                                        .pointer("/params/text")
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    let formatted = format!("[console.{level}] {text_msg}");
                                    console_log.lock().await.push(formatted.clone());
                                    let lvl = match level {
                                        "error" => LogLevel::Error,
                                        "warn" | "warning" => LogLevel::Warn,
                                        _ => LogLevel::Info,
                                    };
                                    debug_collector.record_console(lvl, &formatted);
                                }
                                "browsingContext.load" | "browsingContext.domContentLoaded" => {
                                    let _ = lifecycle_reader.send("networkIdle".to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        });

        let driver = BidiWsDriver {
            tx,
            next_id: AtomicU64::new(1),
            pending,
            primary_context: Mutex::new(String::new()),
            lifecycle_tx,
        };

        // 1. Initialize BiDi session
        driver
            .send_bidi(
                "session.new",
                json!({ "capabilities": { "alwaysMatch": { "acceptInsecureCerts": true } } }),
                Duration::from_secs(10),
            )
            .await?;

        // 2. Subscribe to console & lifecycle events
        let _ = driver
            .send_bidi(
                "session.subscribe",
                json!({
                    "events": [
                        "log.entryAdded",
                        "browsingContext.load",
                        "browsingContext.domContentLoaded"
                    ]
                }),
                Duration::from_secs(5),
            )
            .await;

        // 3. Discover or create initial top-level browsing context
        let tree = driver
            .send_bidi("browsingContext.getTree", json!({}), Duration::from_secs(5))
            .await?;
        let existing_ctx = tree
            .pointer("/contexts/0/context")
            .and_then(Value::as_str)
            .map(str::to_string);
        let ctx_id = if let Some(c) = existing_ctx {
            c
        } else {
            let created = driver
                .send_bidi(
                    "browsingContext.create",
                    json!({ "type": "tab" }),
                    Duration::from_secs(5),
                )
                .await?;
            created
                .get("context")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ToolError::CommandExec("BiDi browsingContext.create returned no context".into())
                })?
                .to_string()
        };
        *driver.primary_context.lock().await = ctx_id;

        Ok(Self::BidiWs(driver))
    }

    /// Translates a CDP `Domain.method` request into W3C WebDriver HTTP or WebDriver BiDi and
    /// formats the response identically to CDP's `result` payload.
    pub async fn execute_cdp_command(
        &self,
        method: &str,
        params: Value,
        session_override: Option<&str>,
        timeout_dur: Duration,
    ) -> Result<Value> {
        match self {
            Self::W3cHttp(d) => d.execute_cdp(method, params, session_override).await,
            Self::BidiWs(d) => {
                d.execute_cdp(method, params, session_override, timeout_dur)
                    .await
            }
        }
    }
}

impl W3cHttpDriver {
    async fn ensure_window_selected(&self, session_override: Option<&str>) -> Result<String> {
        let desired = match session_override.filter(|s| !s.is_empty()) {
            Some(s) => s.to_string(),
            None => self.primary_handle.lock().await.clone(),
        };
        let mut active = self.active_handle.lock().await;
        if *active != desired && desired != "default" {
            let url = format!("{}/session/{}/window", self.base_url, self.session_id);
            let _ = self
                .http
                .post(&url)
                .json(&json!({ "handle": desired }))
                .send()
                .await;
            *active = desired.clone();
        }
        Ok(desired)
    }

    async fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}/session/{}{}", self.base_url, self.session_id, path);
        let resp =
            self.http.post(&url).json(body).send().await.map_err(|e| {
                ToolError::CommandExec(format!("WebDriver POST {path} failed: {e}"))
            })?;
        let status = resp.status();
        let val: Value = resp.json().await.unwrap_or_else(|_| json!({}));
        if !status.is_success() {
            let msg = val
                .pointer("/value/message")
                .and_then(Value::as_str)
                .unwrap_or("WebDriver request failed");
            return Err(ToolError::CommandExec(format!("WebDriver error ({path}): {msg}")).into());
        }
        Ok(val.get("value").cloned().unwrap_or(Value::Null))
    }

    async fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}/session/{}{}", self.base_url, self.session_id, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError::CommandExec(format!("WebDriver GET {path} failed: {e}")))?;
        let status = resp.status();
        let val: Value = resp.json().await.unwrap_or_else(|_| json!({}));
        if !status.is_success() {
            let msg = val
                .pointer("/value/message")
                .and_then(Value::as_str)
                .unwrap_or("WebDriver GET failed");
            return Err(ToolError::CommandExec(format!("WebDriver error ({path}): {msg}")).into());
        }
        Ok(val.get("value").cloned().unwrap_or(Value::Null))
    }

    async fn delete_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}/session/{}{}", self.base_url, self.session_id, path);
        let resp =
            self.http.delete(&url).send().await.map_err(|e| {
                ToolError::CommandExec(format!("WebDriver DELETE {path} failed: {e}"))
            })?;
        let val: Value = resp.json().await.unwrap_or_else(|_| json!({}));
        Ok(val.get("value").cloned().unwrap_or(Value::Null))
    }

    async fn eval_expression(&self, expression: &str) -> Result<Value> {
        let wrapper = r#"
var __expr = arguments[0];
var __done = arguments[arguments.length - 1];
Promise.resolve().then(function() {
  return (0, eval)(__expr);
}).then(function(__val) {
  var __t = typeof __val;
  if (__val === undefined || __val === null || __t === 'string' || __t === 'number' || __t === 'boolean') {
    __done({ ok: true, type: __t, value: __val === undefined ? null : __val });
  } else {
    try {
      __done({ ok: true, type: 'object', value: JSON.parse(JSON.stringify(__val)) });
    } catch (_e) {
      __done({ ok: true, type: 'string', value: String(__val) });
    }
  }
}, function(__err) {
  __done({ ok: false, error: String((__err && (__err.stack || __err.message)) || __err) });
});
"#;
        let out = self
            .post_json(
                "/execute/async",
                &json!({ "script": wrapper, "args": [expression] }),
            )
            .await?;
        let ok = out.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if ok {
            let val_type = out
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("undefined");
            let val = out.get("value").cloned().unwrap_or(Value::Null);
            Ok(json!({
                "result": {
                    "type": val_type,
                    "value": val
                }
            }))
        } else {
            let err_text = out
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("JavaScript execution error");
            Ok(json!({
                "exceptionDetails": {
                    "text": err_text,
                    "exception": { "description": err_text }
                }
            }))
        }
    }

    async fn execute_cdp(
        &self,
        method: &str,
        params: Value,
        session_override: Option<&str>,
    ) -> Result<Value> {
        match method {
            "Page.enable"
            | "Page.setLifecycleEventsEnabled"
            | "Page.bringToFront"
            | "Runtime.enable"
            | "DOM.enable"
            | "Network.enable"
            | "Network.setCacheDisabled"
            | "Network.emulateNetworkConditions"
            | "Log.enable"
            | "Fetch.enable"
            | "Fetch.disable"
            | "Emulation.clearDeviceMetricsOverride" => Ok(json!({})),

            "Page.addScriptToEvaluateOnNewDocument" => {
                if let Some(src) = params.get("source").and_then(Value::as_str) {
                    let mut scripts = self.preload_scripts.lock().await;
                    if !scripts.iter().any(|s| s == src) {
                        scripts.push(src.to_string());
                    }
                }
                Ok(json!({ "identifier": "1" }))
            }

            "Emulation.setDeviceMetricsOverride" => {
                let width = params.get("width").and_then(Value::as_u64).unwrap_or(1440);
                let height = params.get("height").and_then(Value::as_u64).unwrap_or(900);
                let _ = self
                    .post_json("/window/rect", &json!({ "width": width, "height": height }))
                    .await;
                Ok(json!({}))
            }

            "Page.navigate" => {
                let handle = self.ensure_window_selected(session_override).await?;
                let url = params
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("about:blank");
                self.post_json("/url", &json!({ "url": url })).await?;
                let scripts = self.preload_scripts.lock().await.clone();
                for s in &scripts {
                    let _ = self
                        .post_json("/execute/sync", &json!({ "script": s, "args": [] }))
                        .await;
                }
                let _ = self.lifecycle_tx.send("networkIdle".to_string());
                Ok(json!({ "frameId": handle }))
            }

            "MiniBrowse.goBack" => {
                let _ = self.ensure_window_selected(session_override).await?;
                self.post_json("/back", &json!({})).await?;
                Ok(json!({ "navigated": true }))
            }

            "MiniBrowse.goForward" => {
                let _ = self.ensure_window_selected(session_override).await?;
                self.post_json("/forward", &json!({})).await?;
                Ok(json!({ "navigated": true }))
            }

            "Runtime.evaluate" => {
                let _ = self.ensure_window_selected(session_override).await?;
                let expr = params
                    .get("expression")
                    .and_then(Value::as_str)
                    .unwrap_or("null");
                self.eval_expression(expr).await
            }

            "DOM.getDocument" => Ok(json!({ "root": { "nodeId": 1 } })),

            "DOM.getOuterHTML" => {
                let _ = self.ensure_window_selected(session_override).await?;
                let res = self
                    .eval_expression(
                        "document.documentElement ? document.documentElement.outerHTML : ''",
                    )
                    .await?;
                let html = res
                    .pointer("/result/value")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Ok(json!({ "outerHTML": html }))
            }

            "Page.captureScreenshot" => {
                let _ = self.ensure_window_selected(session_override).await?;
                let data = self.get_json("/screenshot").await?;
                let b64 = data.as_str().unwrap_or("");
                Ok(json!({ "data": b64 }))
            }

            "Page.printToPDF" => {
                let _ = self.ensure_window_selected(session_override).await?;
                let landscape = params
                    .get("landscape")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let bg = params
                    .get("printBackground")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let data = self
                    .post_json(
                        "/print",
                        &json!({
                            "orientation": if landscape { "landscape" } else { "portrait" },
                            "background": bg
                        }),
                    )
                    .await?;
                Ok(json!({ "data": data.as_str().unwrap_or("") }))
            }

            "Input.dispatchMouseEvent" => {
                let _ = self.ensure_window_selected(session_override).await?;
                let ev_type = params.get("type").and_then(Value::as_str).unwrap_or("");
                let x = params
                    .get("x")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    .round() as i64;
                let y = params
                    .get("y")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    .round() as i64;
                let btn = match params.get("button").and_then(Value::as_str) {
                    Some("right") => 2,
                    Some("middle") => 1,
                    _ => 0,
                };
                let step = match ev_type {
                    "mouseMoved" => json!({
                        "type": "pointerMove",
                        "duration": 10,
                        "x": x,
                        "y": y,
                        "origin": "viewport"
                    }),
                    "mousePressed" => json!({ "type": "pointerDown", "button": btn }),
                    "mouseReleased" => json!({ "type": "pointerUp", "button": btn }),
                    "mouseWheel" => {
                        let dx = params.get("deltaX").and_then(Value::as_f64).unwrap_or(0.0) as i64;
                        let dy = params.get("deltaY").and_then(Value::as_f64).unwrap_or(0.0) as i64;
                        return self
                            .post_json(
                                "/actions",
                                &json!({
                                    "actions": [{
                                        "type": "wheel",
                                        "id": "wheel0",
                                        "actions": [{
                                            "type": "scroll",
                                            "x": x,
                                            "y": y,
                                            "deltaX": dx,
                                            "deltaY": dy,
                                            "duration": 20
                                        }]
                                    }]
                                }),
                            )
                            .await
                            .map(|_| json!({}));
                    }
                    _ => return Ok(json!({})),
                };
                let _ = self
                    .post_json(
                        "/actions",
                        &json!({
                            "actions": [{
                                "type": "pointer",
                                "id": "mouse0",
                                "parameters": { "pointerType": "mouse" },
                                "actions": [step]
                            }]
                        }),
                    )
                    .await;
                Ok(json!({}))
            }

            "Input.dispatchKeyEvent" => {
                let _ = self.ensure_window_selected(session_override).await?;
                let ev_type = params.get("type").and_then(Value::as_str).unwrap_or("");
                let key_raw = params
                    .get("text")
                    .or_else(|| params.get("key"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let mapped = map_webdriver_key(key_raw);
                if mapped.is_empty() {
                    return Ok(json!({}));
                }
                let action_type = match ev_type {
                    "keyDown" | "rawKeyDown" | "char" => "keyDown",
                    "keyUp" => "keyUp",
                    _ => return Ok(json!({})),
                };
                let actions = if ev_type == "char" {
                    vec![
                        json!({ "type": "keyDown", "value": mapped }),
                        json!({ "type": "keyUp", "value": mapped }),
                    ]
                } else {
                    vec![json!({ "type": action_type, "value": mapped })]
                };
                let _ = self
                    .post_json(
                        "/actions",
                        &json!({
                            "actions": [{
                                "type": "key",
                                "id": "kbd0",
                                "actions": actions
                            }]
                        }),
                    )
                    .await;
                Ok(json!({}))
            }

            "Target.getTargets" => {
                let handles_val = self.get_json("/window/handles").await?;
                let handles = handles_val.as_array().cloned().unwrap_or_default();
                let mut infos = Vec::with_capacity(handles.len());
                for h in handles {
                    if let Some(id) = h.as_str() {
                        infos.push(json!({
                            "targetId": id,
                            "type": "page",
                            "title": "Tab",
                            "url": "about:blank",
                            "attached": true
                        }));
                    }
                }
                Ok(json!({ "targetInfos": infos }))
            }

            "Target.createTarget" => {
                let created = self
                    .post_json("/window/new", &json!({ "type": "tab" }))
                    .await?;
                let handle = created
                    .get("handle")
                    .and_then(Value::as_str)
                    .unwrap_or("default")
                    .to_string();
                if let Some(url) = params
                    .get("url")
                    .and_then(Value::as_str)
                    .filter(|u| !u.is_empty() && *u != "about:blank")
                {
                    let _ = self.ensure_window_selected(Some(&handle)).await;
                    let _ = self.post_json("/url", &json!({ "url": url })).await;
                }
                Ok(json!({ "targetId": handle }))
            }

            "Target.attachToTarget" => {
                let target_id = params
                    .get("targetId")
                    .and_then(Value::as_str)
                    .unwrap_or("default");
                Ok(json!({ "sessionId": target_id }))
            }

            "Target.activateTarget" => {
                if let Some(tid) = params.get("targetId").and_then(Value::as_str) {
                    *self.primary_handle.lock().await = tid.to_string();
                    let _ = self.ensure_window_selected(Some(tid)).await;
                }
                Ok(json!({}))
            }

            "Target.closeTarget" => {
                if let Some(tid) = params.get("targetId").and_then(Value::as_str) {
                    let _ = self.ensure_window_selected(Some(tid)).await;
                    let _ = self.delete_json("/window").await;
                }
                Ok(json!({ "success": true }))
            }

            "Target.createBrowserContext" => Ok(json!({ "browserContextId": "w3c-default" })),
            "Target.disposeBrowserContext" => Ok(json!({})),

            "Network.getCookies" => {
                let _ = self.ensure_window_selected(session_override).await?;
                let cookies = self.get_json("/cookie").await?;
                Ok(json!({ "cookies": cookies }))
            }

            "Network.setCookies" => {
                let _ = self.ensure_window_selected(session_override).await?;
                if let Some(arr) = params.get("cookies").and_then(Value::as_array) {
                    for c in arr {
                        let _ = self.post_json("/cookie", &json!({ "cookie": c })).await;
                    }
                }
                Ok(json!({}))
            }

            "Network.setCookie" => {
                let _ = self.ensure_window_selected(session_override).await?;
                let ok = self
                    .post_json("/cookie", &json!({ "cookie": params }))
                    .await
                    .is_ok();
                Ok(json!({ "success": ok }))
            }

            "Network.clearBrowserCookies" => {
                let _ = self.ensure_window_selected(session_override).await?;
                let _ = self.delete_json("/cookie").await;
                Ok(json!({}))
            }

            other => Err(ToolError::CommandExec(format!(
                "CDP method '{other}' is not supported on W3C WebDriver engine"
            ))
            .into()),
        }
    }
}

impl BidiWsDriver {
    async fn send_bidi(&self, method: &str, params: Value, timeout_dur: Duration) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx_once, rx_once) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx_once);
        }
        let req = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        self.tx
            .send(Message::Text(req.to_string()))
            .map_err(|e| ToolError::CommandExec(format!("BiDi WebSocket send error: {e}")))?;

        let raw = match tokio::time::timeout(timeout_dur, rx_once).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => {
                self.pending.lock().await.remove(&id);
                return Err(ToolError::CommandExec(format!(
                    "BiDi connection closed during '{method}'"
                ))
                .into());
            }
            Err(_) => {
                self.pending.lock().await.remove(&id);
                return Err(
                    ToolError::CommandExec(format!("Timeout waiting for BiDi '{method}'")).into(),
                );
            }
        };

        if raw.get("type").and_then(Value::as_str) == Some("error") || raw.get("error").is_some() {
            let msg = raw
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Unknown BiDi error");
            return Err(ToolError::CommandExec(format!("BiDi error in '{method}': {msg}")).into());
        }

        Ok(raw.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn resolve_context(&self, session_override: Option<&str>) -> String {
        match session_override.filter(|s| !s.is_empty()) {
            Some(s) => s.to_string(),
            None => self.primary_context.lock().await.clone(),
        }
    }

    async fn eval_bidi(
        &self,
        expr: &str,
        await_promise: bool,
        session_override: Option<&str>,
        timeout_dur: Duration,
    ) -> Result<Value> {
        let ctx = self.resolve_context(session_override).await;
        let res = self
            .send_bidi(
                "script.evaluate",
                json!({
                    "expression": expr,
                    "target": { "context": ctx },
                    "awaitPromise": await_promise,
                    "resultOwnership": "none"
                }),
                timeout_dur,
            )
            .await?;
        if res.get("type").and_then(Value::as_str) == Some("exception") {
            let txt = res
                .pointer("/exceptionDetails/text")
                .and_then(Value::as_str)
                .unwrap_or("Unhandled JS exception");
            return Ok(json!({
                "exceptionDetails": {
                    "text": txt,
                    "exception": { "description": txt }
                }
            }));
        }
        let remote = res.get("result").cloned().unwrap_or(Value::Null);
        let r_type = remote
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("undefined")
            .to_string();
        let decoded = decode_bidi_remote_value(&remote);
        Ok(json!({
            "result": {
                "type": r_type,
                "value": decoded
            }
        }))
    }

    async fn execute_cdp(
        &self,
        method: &str,
        params: Value,
        session_override: Option<&str>,
        timeout_dur: Duration,
    ) -> Result<Value> {
        match method {
            "Page.enable"
            | "Page.setLifecycleEventsEnabled"
            | "Page.bringToFront"
            | "Runtime.enable"
            | "DOM.enable"
            | "Network.enable"
            | "Network.setCacheDisabled"
            | "Network.emulateNetworkConditions"
            | "Log.enable"
            | "Fetch.enable"
            | "Fetch.disable"
            | "Emulation.clearDeviceMetricsOverride" => Ok(json!({})),

            "Page.addScriptToEvaluateOnNewDocument" => {
                if let Some(src) = params.get("source").and_then(Value::as_str) {
                    let fn_decl = format!("() => {{ {src} }}");
                    let _ = self
                        .send_bidi(
                            "script.addPreloadScript",
                            json!({ "functionDeclaration": fn_decl }),
                            timeout_dur,
                        )
                        .await;
                }
                Ok(json!({ "identifier": "1" }))
            }

            "Emulation.setDeviceMetricsOverride" => {
                let ctx = self.resolve_context(session_override).await;
                let width = params.get("width").and_then(Value::as_u64).unwrap_or(1440);
                let height = params.get("height").and_then(Value::as_u64).unwrap_or(900);
                let _ = self
                    .send_bidi(
                        "browsingContext.setViewport",
                        json!({
                            "context": ctx,
                            "viewport": { "width": width, "height": height }
                        }),
                        timeout_dur,
                    )
                    .await;
                Ok(json!({}))
            }

            "Page.navigate" => {
                let ctx = self.resolve_context(session_override).await;
                let url = params
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("about:blank");
                let res = self
                    .send_bidi(
                        "browsingContext.navigate",
                        json!({
                            "context": ctx,
                            "url": url,
                            "wait": "complete"
                        }),
                        timeout_dur,
                    )
                    .await?;
                let _ = self.lifecycle_tx.send("networkIdle".to_string());
                let nav_id = res
                    .get("navigation")
                    .and_then(Value::as_str)
                    .unwrap_or(&ctx);
                Ok(json!({ "frameId": ctx, "loaderId": nav_id }))
            }

            "MiniBrowse.goBack" => {
                let ctx = self.resolve_context(session_override).await;
                self.send_bidi(
                    "browsingContext.traverseHistory",
                    json!({ "context": ctx, "delta": -1 }),
                    timeout_dur,
                )
                .await?;
                Ok(json!({ "navigated": true }))
            }

            "MiniBrowse.goForward" => {
                let ctx = self.resolve_context(session_override).await;
                self.send_bidi(
                    "browsingContext.traverseHistory",
                    json!({ "context": ctx, "delta": 1 }),
                    timeout_dur,
                )
                .await?;
                Ok(json!({ "navigated": true }))
            }

            "Runtime.evaluate" => {
                let expr = params
                    .get("expression")
                    .and_then(Value::as_str)
                    .unwrap_or("null");
                let await_promise = params
                    .get("awaitPromise")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                self.eval_bidi(expr, await_promise, session_override, timeout_dur)
                    .await
            }

            "DOM.getDocument" => Ok(json!({ "root": { "nodeId": 1 } })),

            "DOM.getOuterHTML" => {
                let eval = self
                    .eval_bidi(
                        "document.documentElement ? document.documentElement.outerHTML : ''",
                        false,
                        session_override,
                        timeout_dur,
                    )
                    .await?;
                let html = eval
                    .pointer("/result/value")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Ok(json!({ "outerHTML": html }))
            }

            "Page.captureScreenshot" => {
                let ctx = self.resolve_context(session_override).await;
                let res = self
                    .send_bidi(
                        "browsingContext.captureScreenshot",
                        json!({ "context": ctx }),
                        timeout_dur,
                    )
                    .await?;
                let data = res.get("data").and_then(Value::as_str).unwrap_or("");
                Ok(json!({ "data": data }))
            }

            "Page.printToPDF" => {
                let ctx = self.resolve_context(session_override).await;
                let landscape = params
                    .get("landscape")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let bg = params
                    .get("printBackground")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let res = self
                    .send_bidi(
                        "browsingContext.print",
                        json!({
                            "context": ctx,
                            "orientation": if landscape { "landscape" } else { "portrait" },
                            "background": bg
                        }),
                        timeout_dur,
                    )
                    .await?;
                Ok(json!({ "data": res.get("data").and_then(Value::as_str).unwrap_or("") }))
            }

            "Input.dispatchMouseEvent" => {
                let ctx = self.resolve_context(session_override).await;
                let ev_type = params.get("type").and_then(Value::as_str).unwrap_or("");
                let x = params
                    .get("x")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    .round() as i64;
                let y = params
                    .get("y")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    .round() as i64;
                let btn = match params.get("button").and_then(Value::as_str) {
                    Some("right") => 2,
                    Some("middle") => 1,
                    _ => 0,
                };
                let actions = match ev_type {
                    "mouseMoved" => json!([{
                        "type": "pointer",
                        "id": "mouse0",
                        "parameters": { "pointerType": "mouse" },
                        "actions": [{ "type": "pointerMove", "x": x, "y": y, "duration": 10, "origin": "viewport" }]
                    }]),
                    "mousePressed" => json!([{
                        "type": "pointer",
                        "id": "mouse0",
                        "parameters": { "pointerType": "mouse" },
                        "actions": [{ "type": "pointerDown", "button": btn }]
                    }]),
                    "mouseReleased" => json!([{
                        "type": "pointer",
                        "id": "mouse0",
                        "parameters": { "pointerType": "mouse" },
                        "actions": [{ "type": "pointerUp", "button": btn }]
                    }]),
                    "mouseWheel" => {
                        let dx = params.get("deltaX").and_then(Value::as_f64).unwrap_or(0.0) as i64;
                        let dy = params.get("deltaY").and_then(Value::as_f64).unwrap_or(0.0) as i64;
                        json!([{
                            "type": "wheel",
                            "id": "wheel0",
                            "actions": [{ "type": "scroll", "x": x, "y": y, "deltaX": dx, "deltaY": dy, "duration": 20 }]
                        }])
                    }
                    _ => return Ok(json!({})),
                };
                let _ = self
                    .send_bidi(
                        "input.performActions",
                        json!({ "context": ctx, "actions": actions }),
                        timeout_dur,
                    )
                    .await;
                Ok(json!({}))
            }

            "Input.dispatchKeyEvent" => {
                let ctx = self.resolve_context(session_override).await;
                let ev_type = params.get("type").and_then(Value::as_str).unwrap_or("");
                let key_raw = params
                    .get("text")
                    .or_else(|| params.get("key"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let mapped = map_webdriver_key(key_raw);
                if mapped.is_empty() {
                    return Ok(json!({}));
                }
                let seq = match ev_type {
                    "char" => vec![
                        json!({ "type": "keyDown", "value": mapped }),
                        json!({ "type": "keyUp", "value": mapped }),
                    ],
                    "keyDown" | "rawKeyDown" => vec![json!({ "type": "keyDown", "value": mapped })],
                    "keyUp" => vec![json!({ "type": "keyUp", "value": mapped })],
                    _ => return Ok(json!({})),
                };
                let _ = self
                    .send_bidi(
                        "input.performActions",
                        json!({
                            "context": ctx,
                            "actions": [{ "type": "key", "id": "kbd0", "actions": seq }]
                        }),
                        timeout_dur,
                    )
                    .await;
                Ok(json!({}))
            }

            "Target.getTargets" => {
                let tree = self
                    .send_bidi("browsingContext.getTree", json!({}), timeout_dur)
                    .await?;
                let mut infos = Vec::new();
                if let Some(ctxs) = tree.get("contexts").and_then(Value::as_array) {
                    for c in ctxs {
                        if let Some(id) = c.get("context").and_then(Value::as_str) {
                            let url = c
                                .get("url")
                                .and_then(Value::as_str)
                                .unwrap_or("about:blank");
                            infos.push(json!({
                                "targetId": id,
                                "type": "page",
                                "title": url,
                                "url": url,
                                "attached": true
                            }));
                        }
                    }
                }
                Ok(json!({ "targetInfos": infos }))
            }

            "Target.createBrowserContext" => {
                let res = self
                    .send_bidi("browser.createUserContext", json!({}), timeout_dur)
                    .await?;
                let uc = res
                    .get("userContext")
                    .and_then(Value::as_str)
                    .unwrap_or("default");
                Ok(json!({ "browserContextId": uc }))
            }

            "Target.disposeBrowserContext" => {
                if let Some(uc) = params
                    .get("browserContextId")
                    .and_then(Value::as_str)
                    .filter(|s| *s != "default")
                {
                    let _ = self
                        .send_bidi(
                            "browser.removeUserContext",
                            json!({ "userContext": uc }),
                            timeout_dur,
                        )
                        .await;
                }
                Ok(json!({}))
            }

            "Target.createTarget" => {
                let mut create_params = json!({ "type": "tab", "background": true });
                if let Some(uc) = params
                    .get("browserContextId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && *s != "default")
                {
                    create_params["userContext"] = json!(uc);
                }
                let created = self
                    .send_bidi("browsingContext.create", create_params, timeout_dur)
                    .await?;
                let tid = created
                    .get("context")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if let Some(url) = params
                    .get("url")
                    .and_then(Value::as_str)
                    .filter(|u| !u.is_empty() && *u != "about:blank")
                {
                    let _ = self
                        .send_bidi(
                            "browsingContext.navigate",
                            json!({ "context": tid, "url": url, "wait": "complete" }),
                            timeout_dur,
                        )
                        .await;
                }
                Ok(json!({ "targetId": tid }))
            }

            "Target.attachToTarget" => {
                let tid = params.get("targetId").and_then(Value::as_str).unwrap_or("");
                Ok(json!({ "sessionId": tid }))
            }

            "Target.activateTarget" => {
                if let Some(tid) = params.get("targetId").and_then(Value::as_str) {
                    *self.primary_context.lock().await = tid.to_string();
                    let _ = self
                        .send_bidi(
                            "browsingContext.activate",
                            json!({ "context": tid }),
                            timeout_dur,
                        )
                        .await;
                }
                Ok(json!({}))
            }

            "Target.closeTarget" => {
                if let Some(tid) = params.get("targetId").and_then(Value::as_str) {
                    let _ = self
                        .send_bidi(
                            "browsingContext.close",
                            json!({ "context": tid }),
                            timeout_dur,
                        )
                        .await;
                }
                Ok(json!({ "success": true }))
            }

            "Network.getCookies" => {
                let res = self
                    .send_bidi("storage.getCookies", json!({}), timeout_dur)
                    .await?;
                let cookies = res.get("cookies").cloned().unwrap_or_else(|| json!([]));
                Ok(json!({ "cookies": cookies }))
            }

            "Network.setCookies" => {
                if let Some(arr) = params.get("cookies").and_then(Value::as_array) {
                    for c in arr {
                        let name = c.get("name").and_then(Value::as_str).unwrap_or("");
                        let val = c.get("value").and_then(Value::as_str).unwrap_or("");
                        let domain = c
                            .get("domain")
                            .and_then(Value::as_str)
                            .unwrap_or("localhost");
                        let _ = self
                            .send_bidi(
                                "storage.setCookie",
                                json!({
                                    "cookie": {
                                        "name": name,
                                        "value": { "type": "string", "value": val },
                                        "domain": domain.trim_start_matches('.'),
                                        "path": c.get("path").and_then(Value::as_str).unwrap_or("/")
                                    }
                                }),
                                timeout_dur,
                            )
                            .await;
                    }
                }
                Ok(json!({}))
            }

            "Network.clearBrowserCookies" => {
                let _ = self
                    .send_bidi("storage.deleteCookies", json!({}), timeout_dur)
                    .await;
                Ok(json!({}))
            }

            other => Err(ToolError::CommandExec(format!(
                "CDP method '{other}' is not supported on WebDriver BiDi engine"
            ))
            .into()),
        }
    }
}

/// Decodes a W3C WebDriver BiDi `RemoteValue` into a plain `serde_json::Value`.
pub fn decode_bidi_remote_value(rv: &Value) -> Value {
    let Some(t) = rv.get("type").and_then(Value::as_str) else {
        return rv.clone();
    };
    match t {
        "undefined" | "null" => Value::Null,
        "string" | "boolean" | "number" => rv.get("value").cloned().unwrap_or(Value::Null),
        "bigint" => rv
            .get("value")
            .and_then(Value::as_str)
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
        "array" | "set" => {
            let items = rv
                .get("value")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().map(decode_bidi_remote_value).collect())
                .unwrap_or_default();
            Value::Array(items)
        }
        "object" | "map" => {
            let mut obj = serde_json::Map::new();
            if let Some(pairs) = rv.get("value").and_then(Value::as_array) {
                for pair in pairs {
                    if let Some(kv) = pair.as_array() {
                        if kv.len() == 2 {
                            let key = kv[0]
                                .as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| decode_bidi_remote_value(&kv[0]).to_string());
                            obj.insert(key, decode_bidi_remote_value(&kv[1]));
                        }
                    }
                }
            }
            Value::Object(obj)
        }
        _ => rv.get("value").cloned().unwrap_or(Value::Null),
    }
}

/// Maps human/DOM key names to W3C WebDriver Unicode PUA codepoints (`\u{E000}`..`\u{E03D}`).
fn map_webdriver_key(key: &str) -> String {
    match key {
        "Enter" | "Return" => "\u{E007}".to_string(),
        "Tab" => "\u{E004}".to_string(),
        "Backspace" => "\u{E003}".to_string(),
        "Escape" | "Esc" => "\u{E00C}".to_string(),
        "Space" | " " => " ".to_string(),
        "ArrowLeft" => "\u{E012}".to_string(),
        "ArrowUp" => "\u{E013}".to_string(),
        "ArrowRight" => "\u{E014}".to_string(),
        "ArrowDown" => "\u{E015}".to_string(),
        "Delete" => "\u{E017}".to_string(),
        "Home" => "\u{E011}".to_string(),
        "End" => "\u{E010}".to_string(),
        "PageUp" => "\u{E00E}".to_string(),
        "PageDown" => "\u{E00F}".to_string(),
        other if other.chars().count() == 1 => other.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_bidi_remote_value_primitives_and_nested_objects() {
        let rv = json!({
            "type": "object",
            "value": [
                ["title", { "type": "string", "value": "Firefox BiDi" }],
                ["count", { "type": "number", "value": 3 }],
                ["items", { "type": "array", "value": [
                    { "type": "boolean", "value": true },
                    { "type": "null" }
                ]}]
            ]
        });
        let decoded = decode_bidi_remote_value(&rv);
        assert_eq!(decoded["title"], "Firefox BiDi");
        assert_eq!(decoded["count"], 3);
        assert_eq!(decoded["items"][0], true);
        assert!(decoded["items"][1].is_null());
    }

    #[test]
    fn test_webdriver_key_mapping() {
        assert_eq!(map_webdriver_key("Enter"), "\u{E007}");
        assert_eq!(map_webdriver_key("Escape"), "\u{E00C}");
        assert_eq!(map_webdriver_key("a"), "a");
    }
}
