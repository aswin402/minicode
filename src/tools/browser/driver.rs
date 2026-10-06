use super::debug::DebugCollector;
use crate::constants::BROWSER_NAVIGATE_TIMEOUT_MS;
use crate::error::{Result, ToolError};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;

/// Console-capture shim: wraps console.* and window.onerror into an in-page
/// buffer. Re-installed after every navigation (new document = new window).
const CONSOLE_SHIM_JS: &str = r#"(function(){if(window.__minicode_console)return;window.__minicode_console=[];['log','info','warn','error'].forEach(function(m){var orig=console[m]?console[m].bind(console):function(){};console[m]=function(){try{window.__minicode_console.push('['+m+'] '+Array.prototype.map.call(arguments,function(a){try{return typeof a==='object'?JSON.stringify(a):String(a)}catch(e){return String(a)}}).join(' '))}catch(e){}orig.apply(null,arguments)}});window.addEventListener('error',function(e){window.__minicode_console.push('[error] '+(e.message||'unknown'))})})()"#;

/// Declarative rule for intercepting and mocking HTTP network requests over CDP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockRouteRule {
    pub pattern: String,
    pub status: u16,
    pub body: String,
    pub content_type: Option<String>,
}

/// Client for bidirectional Chrome DevTools Protocol (CDP) communication over WebSockets
pub struct CdpClient {
    tx: mpsc::UnboundedSender<Message>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>,
    page_ws_url: String,
    /// Flatten-session id (Target.attachToTarget); required by engines like
    /// Obscura whose page sockets reject unattached commands, and standard
    /// on Chrome/Firefox browser-level endpoints.
    session_id: Mutex<Option<String>>,
    /// Captured console output and runtime exceptions for diagnostics.
    console_log: Arc<Mutex<Vec<String>>>,
    /// Shared runtime diagnostics collector holding console logs and network errors.
    debug_collector: Arc<DebugCollector>,
    /// Registered mock route rules for intercepting requests.
    #[allow(dead_code)]
    mock_routes: Arc<Mutex<Vec<MockRouteRule>>>,
    /// Blocked URL patterns for ad/tracker/telemetry blocking.
    #[allow(dead_code)]
    blocked_urls: Arc<Mutex<Vec<String>>>,
    /// Broadcast channel for CDP page lifecycle events (networkIdle, DOMContentLoaded, etc.)
    lifecycle_tx: broadcast::Sender<String>,
}

impl CdpClient {
    /// Connects to a running browser engine via its HTTP base URL (e.g. "http://127.0.0.1:9222")
    pub async fn connect(cdp_http_url: &str) -> Result<Self> {
        // Prefer the browser-level endpoint + explicit target attachment:
        // works on Chrome/Firefox and is REQUIRED by Obscura.
        let (page_ws_url, wants_session) =
            match Self::resolve_browser_websocket_url(cdp_http_url).await {
                Ok(url) => (url, true),
                Err(_) => (Self::resolve_page_websocket_url(cdp_http_url).await?, false),
            };

        tracing::info!(ws_url = %page_ws_url, "Connecting to browser CDP WebSocket");

        let (ws_stream, _) = tokio_tungstenite::connect_async(&page_ws_url)
            .await
            .map_err(|e| {
                ToolError::CommandExec(format!(
                    "Failed connecting to CDP WebSocket '{}': {}",
                    page_ws_url, e
                ))
            })?;

        let (mut write, mut read) = ws_stream.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();
        let tx_clone = tx.clone();

        let console_log = Arc::new(Mutex::new(Vec::new()));
        let console_clone = Arc::clone(&console_log);

        let (lifecycle_tx, _) = broadcast::channel(128);
        let lifecycle_tx_clone = lifecycle_tx.clone();

        let debug_collector = Arc::new(DebugCollector::new());
        let debug_collector_clone = Arc::clone(&debug_collector);

        let mock_routes: Arc<Mutex<Vec<MockRouteRule>>> = Arc::new(Mutex::new(Vec::new()));
        let mock_routes_clone = Arc::clone(&mock_routes);

        let blocked_urls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let blocked_urls_clone = Arc::clone(&blocked_urls);

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if write.send(msg).await.is_err() {
                    break;
                }
            }
        });

        // Reader task & event dispatcher
        tokio::spawn(async move {
            while let Some(msg_res) = read.next().await {
                match msg_res {
                    Ok(Message::Text(text)) => {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                            // Check if this is a response to a pending command
                            if let Some(id) = val.get("id").and_then(|v| v.as_u64()) {
                                let mut map = pending_clone.lock().await;
                                if let Some(sender) = map.remove(&id) {
                                    let _ = sender.send(val.clone());
                                }
                            } else if let Some(method) = val.get("method").and_then(|v| v.as_str())
                            {
                                match method {
                                    // Capture console output for browser_debug_logs
                                    "Runtime.consoleAPICalled" => {
                                        if let Some(args) =
                                            val.pointer("/params/args").and_then(|a| a.as_array())
                                        {
                                            let parts: Vec<String> = args
                                                .iter()
                                                .map(|a| {
                                                    a.pointer("/value")
                                                        .map(|v| v.to_string())
                                                        .or_else(|| {
                                                            a.get("description")
                                                                .and_then(|d| d.as_str())
                                                                .map(str::to_string)
                                                        })
                                                        .unwrap_or_default()
                                                })
                                                .collect();
                                            let entry_type = val
                                                .pointer("/params/type")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("log");
                                            let msg = format!(
                                                "[console.{}] {}",
                                                entry_type,
                                                parts.join(" ")
                                            );
                                            console_clone.lock().await.push(msg.clone());

                                            let log_level = match entry_type {
                                                "error" => super::debug::LogLevel::Error,
                                                "warn" | "warning" => super::debug::LogLevel::Warn,
                                                _ => super::debug::LogLevel::Info,
                                            };
                                            debug_collector_clone.record_console(log_level, &msg);
                                        }
                                    }
                                    "Runtime.exceptionThrown" => {
                                        let text = val
                                            .pointer("/params/exceptionDetails/exception/detail")
                                            .or_else(|| {
                                                val.pointer("/params/exceptionDetails/text")
                                            })
                                            .and_then(|t| t.as_str())
                                            .unwrap_or("Unhandled exception")
                                            .to_string();
                                        let msg = format!("[exception] {}", text);
                                        console_clone.lock().await.push(msg.clone());
                                        debug_collector_clone
                                            .record_console(super::debug::LogLevel::Error, &msg);
                                    }
                                    "Log.entryAdded" => {
                                        let level = val
                                            .pointer("/params/entry/level")
                                            .and_then(|l| l.as_str())
                                            .unwrap_or("info");
                                        let text = val
                                            .pointer("/params/entry/text")
                                            .and_then(|t| t.as_str())
                                            .unwrap_or("");
                                        let msg = format!("[{}] {}", level, text);
                                        console_clone.lock().await.push(msg.clone());

                                        let log_level = match level {
                                            "error" => super::debug::LogLevel::Error,
                                            "warning" | "warn" => super::debug::LogLevel::Warn,
                                            _ => super::debug::LogLevel::Info,
                                        };
                                        debug_collector_clone.record_console(log_level, &msg);
                                    }
                                    // CDP Page Lifecycle events (networkIdle, DOMContentLoaded, load, etc.)
                                    "Page.lifecycleEvent" => {
                                        if let Some(name) =
                                            val.pointer("/params/name").and_then(|n| n.as_str())
                                        {
                                            let _ = lifecycle_tx_clone.send(name.to_string());
                                        }
                                    }
                                    // Live network telemetry: track failed HTTP 4xx/5xx responses
                                    "Network.responseReceived" => {
                                        if let Some(status) = val
                                            .pointer("/params/response/status")
                                            .and_then(|s| s.as_u64())
                                        {
                                            if status >= 400 {
                                                let url = val
                                                    .pointer("/params/response/url")
                                                    .and_then(|u| u.as_str())
                                                    .unwrap_or("");
                                                let status_text = val
                                                    .pointer("/params/response/statusText")
                                                    .and_then(|st| st.as_str())
                                                    .unwrap_or("");
                                                let method = val
                                                    .pointer(
                                                        "/params/response/requestHeaders/:method",
                                                    )
                                                    .or_else(|| val.pointer("/params/type"))
                                                    .and_then(|m| m.as_str())
                                                    .unwrap_or("GET");
                                                debug_collector_clone.record_network_error(
                                                    method,
                                                    url,
                                                    status as u16,
                                                    Some(status_text),
                                                );
                                            }
                                        }
                                    }
                                    // Network resource load failure
                                    "Network.loadingFailed" => {
                                        let canceled = val
                                            .pointer("/params/canceled")
                                            .and_then(|c| c.as_bool())
                                            .unwrap_or(false);
                                        if !canceled {
                                            let error_text = val
                                                .pointer("/params/errorText")
                                                .and_then(|e| e.as_str())
                                                .unwrap_or("Failed to load");
                                            let req_type = val
                                                .pointer("/params/type")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("Resource");
                                            debug_collector_clone.record_network_error(
                                                req_type,
                                                "Network Request",
                                                0,
                                                Some(error_text),
                                            );
                                        }
                                    }
                                    // CDP Fetch domain route mocking and blocking
                                    "Fetch.requestPaused" => {
                                        if let Some(req_id) = val
                                            .pointer("/params/requestId")
                                            .and_then(|r| r.as_str())
                                        {
                                            let req_url = val
                                                .pointer("/params/request/url")
                                                .and_then(|u| u.as_str())
                                                .unwrap_or("");
                                            let session_id_opt =
                                                val.get("sessionId").and_then(|s| s.as_str());
                                            let routes = mock_routes_clone.lock().await;
                                            let blocked = blocked_urls_clone.lock().await;

                                            let is_blocked = blocked
                                                .iter()
                                                .any(|pat| matches_pattern(pat, req_url));
                                            if is_blocked {
                                                let mut fail_cmd = json!({
                                                    "id": 999_998,
                                                    "method": "Fetch.failRequest",
                                                    "params": {
                                                        "requestId": req_id,
                                                        "errorReason": "BlockedByClient"
                                                    }
                                                });
                                                if let Some(sid) = session_id_opt {
                                                    fail_cmd["sessionId"] = json!(sid);
                                                }
                                                let _ = tx_clone
                                                    .send(Message::Text(fail_cmd.to_string()));
                                            } else if let Some(rule) = routes
                                                .iter()
                                                .find(|r| matches_pattern(&r.pattern, req_url))
                                            {
                                                let encoded_body =
                                                    general_base64_encode(rule.body.as_bytes());
                                                let content_type = rule
                                                    .content_type
                                                    .as_deref()
                                                    .unwrap_or("application/json");
                                                let phrase = match rule.status {
                                                    200 => "OK",
                                                    201 => "Created",
                                                    400 => "Bad Request",
                                                    401 => "Unauthorized",
                                                    403 => "Forbidden",
                                                    404 => "Not Found",
                                                    500 => "Internal Server Error",
                                                    _ => "Mocked Response",
                                                };
                                                if rule.status >= 400 {
                                                    debug_collector_clone.record_network_error(
                                                        "MOCK",
                                                        req_url,
                                                        rule.status,
                                                        Some(phrase),
                                                    );
                                                }
                                                let mut fulfill_cmd = json!({
                                                    "id": 999_998,
                                                    "method": "Fetch.fulfillRequest",
                                                    "params": {
                                                        "requestId": req_id,
                                                        "responseCode": rule.status,
                                                        "responsePhrase": phrase,
                                                        "responseHeaders": [
                                                            { "name": "Content-Type", "value": content_type },
                                                            { "name": "Access-Control-Allow-Origin", "value": "*" }
                                                        ],
                                                        "body": encoded_body
                                                    }
                                                });
                                                if let Some(sid) = session_id_opt {
                                                    fulfill_cmd["sessionId"] = json!(sid);
                                                }
                                                let _ = tx_clone
                                                    .send(Message::Text(fulfill_cmd.to_string()));
                                            } else {
                                                let mut cont_cmd = json!({
                                                    "id": 999_998,
                                                    "method": "Fetch.continueRequest",
                                                    "params": {
                                                        "requestId": req_id
                                                    }
                                                });
                                                if let Some(sid) = session_id_opt {
                                                    cont_cmd["sessionId"] = json!(sid);
                                                }
                                                let _ = tx_clone
                                                    .send(Message::Text(cont_cmd.to_string()));
                                            }
                                        }
                                    }
                                    // Auto-handle modal alerts and dialogs
                                    "Page.javascriptDialogOpening" => {
                                        tracing::info!("Auto-dismissing browser JavaScript dialog");
                                        let session_id_opt =
                                            val.get("sessionId").and_then(|s| s.as_str());
                                        let mut dismiss_cmd = json!({
                                            "id": 999_999,
                                            "method": "Page.handleJavaScriptDialog",
                                            "params": { "accept": true }
                                        });
                                        if let Some(sid) = session_id_opt {
                                            dismiss_cmd["sessionId"] = json!(sid);
                                        }
                                        let _ =
                                            tx_clone.send(Message::Text(dismiss_cmd.to_string()));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        });

        let client = Self {
            tx,
            next_id: AtomicU64::new(1),
            pending,
            page_ws_url,
            session_id: Mutex::new(None),
            console_log,
            debug_collector,
            mock_routes,
            blocked_urls,
            lifecycle_tx,
        };

        if wants_session {
            client.attach_fresh_page().await?;
        }

        // Enable core domains
        client.enable_core_domains().await?;

        // Wrap console.* into an in-page buffer: some engines (Obscura) do not
        // emit Runtime.consoleAPICalled events, so capture at the source.
        let _ = client.evaluate_js(CONSOLE_SHIM_JS).await;

        Ok(client)
    }

    /// Creates a blank page target and attaches with a flatten session,
    /// storing the session id for all subsequent commands.
    async fn attach_fresh_page(&self) -> Result<()> {
        let created = self
            .send_command("Target.createTarget", json!({"url": "about:blank"}))
            .await?;
        let target_id = created
            .get("targetId")
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                ToolError::CommandExec("CDP createTarget returned no targetId".to_string())
            })?
            .to_string();

        let attached = self
            .send_command(
                "Target.attachToTarget",
                json!({"targetId": target_id, "flatten": true}),
            )
            .await?;
        let session = attached
            .get("sessionId")
            .and_then(|s| s.as_str())
            .ok_or_else(|| {
                ToolError::CommandExec("CDP attachToTarget returned no sessionId".to_string())
            })?
            .to_string();

        *self.session_id.lock().await = Some(session);
        Ok(())
    }

    /// Resolves the browser-level WebSocket endpoint from /json/version
    async fn resolve_browser_websocket_url(http_base: &str) -> Result<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();
        let ver_url = format!("{}/json/version", http_base);
        let resp = client
            .get(&ver_url)
            .send()
            .await
            .map_err(|e| ToolError::CommandExec(format!("CDP version probe failed: {}", e)))?;
        let ver = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| ToolError::CommandExec(format!("CDP version decode failed: {}", e)))?;
        ver.get("webSocketDebuggerUrl")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                ToolError::CommandExec(format!(
                    "No webSocketDebuggerUrl in /json/version of '{}'",
                    http_base
                ))
                .into()
            })
    }

    /// Resolves the WebSocket debugger URL for the active page target
    async fn resolve_page_websocket_url(http_base: &str) -> Result<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();

        // 1. Try GET /json/list for existing page targets
        let list_url = format!("{}/json/list", http_base);
        if let Ok(resp) = client.get(&list_url).send().await {
            if let Ok(targets) = resp.json::<Vec<serde_json::Value>>().await {
                for target in targets {
                    let target_type = target.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if target_type == "page" || target_type.is_empty() {
                        if let Some(ws_url) =
                            target.get("webSocketDebuggerUrl").and_then(|u| u.as_str())
                        {
                            return Ok(ws_url.to_string());
                        }
                    }
                }
            }
        }

        // 2. Try PUT /json/new to create a new page
        let new_url = format!("{}/json/new?about:blank", http_base);
        if let Ok(resp) = client.put(&new_url).send().await {
            if let Ok(target) = resp.json::<serde_json::Value>().await {
                if let Some(ws_url) = target.get("webSocketDebuggerUrl").and_then(|u| u.as_str()) {
                    return Ok(ws_url.to_string());
                }
            }
        }

        // 3. Fallback: GET /json/version
        let ver_url = format!("{}/json/version", http_base);
        if let Ok(resp) = client.get(&ver_url).send().await {
            if let Ok(ver) = resp.json::<serde_json::Value>().await {
                if let Some(ws_url) = ver.get("webSocketDebuggerUrl").and_then(|u| u.as_str()) {
                    return Ok(ws_url.to_string());
                }
            }
        }

        Err(ToolError::CommandExec(format!(
            "Unable to discover CDP WebSocket endpoint from '{}'",
            http_base
        ))
        .into())
    }

    /// Sends a JSON-RPC command to the browser and awaits the response
    pub async fn send_command(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (resp_tx, resp_rx) = oneshot::channel();

        {
            let mut map = self.pending.lock().await;
            map.insert(id, resp_tx);
        }

        let mut req = json!({
            "id": id,
            "method": method,
            "params": params
        });
        if let Some(sid) = self.session_id.lock().await.as_ref() {
            req["sessionId"] = json!(sid);
        }

        self.tx
            .send(Message::Text(req.to_string()))
            .map_err(|e| ToolError::CommandExec(format!("Failed sending CDP command: {}", e)))?;

        let timeout_dur = Duration::from_millis(BROWSER_NAVIGATE_TIMEOUT_MS);
        let res = match tokio::time::timeout(timeout_dur, resp_rx).await {
            Ok(Ok(val)) => val,
            Ok(Err(_)) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                return Err(ToolError::CommandExec(format!(
                    "CDP connection dropped during method '{}'",
                    method
                ))
                .into());
            }
            Err(_) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                return Err(ToolError::CommandExec(format!(
                    "Timeout waiting for CDP method '{}'",
                    method
                ))
                .into());
            }
        };

        if let Some(err) = res.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown CDP error");
            return Err(
                ToolError::CommandExec(format!("CDP Error in '{}': {}", method, msg)).into(),
            );
        }

        Ok(res
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Enables standard DevTools domains
    /// Returns and clears captured console output and runtime exceptions.
    pub async fn drain_console(&self) -> Vec<String> {
        std::mem::take(&mut *self.console_log.lock().await)
    }

    /// Returns captured console output without clearing it.
    #[allow(dead_code)]
    pub async fn peek_console(&self) -> Vec<String> {
        self.console_log.lock().await.clone()
    }

    /// Enables standard DevTools domains
    pub async fn enable_core_domains(&self) -> Result<()> {
        let _ = self.send_command("Page.enable", json!({})).await;
        let _ = self
            .send_command("Page.setLifecycleEventsEnabled", json!({ "enabled": true }))
            .await;
        let _ = self.send_command("Runtime.enable", json!({})).await;
        let _ = self.send_command("DOM.enable", json!({})).await;
        let _ = self.send_command("Network.enable", json!({})).await;
        let _ = self.send_command("Log.enable", json!({})).await;
        Ok(())
    }

    /// Access the shared runtime debug collector
    pub fn debug_collector(&self) -> &Arc<DebugCollector> {
        &self.debug_collector
    }

    /// Enables CDP request interception via the Fetch domain
    pub async fn enable_fetch_interception(&self) -> Result<()> {
        self.send_command(
            "Fetch.enable",
            json!({
                "patterns": [{ "urlPattern": "*", "requestStage": "Request" }]
            }),
        )
        .await?;
        Ok(())
    }

    /// Disables CDP request interception via the Fetch domain
    pub async fn disable_fetch_interception(&self) -> Result<()> {
        self.send_command("Fetch.disable", json!({})).await?;
        Ok(())
    }

    /// Registers a mock route rule and activates Fetch domain interception
    pub async fn add_mock_route(&self, rule: MockRouteRule) -> Result<()> {
        {
            let mut routes = self.mock_routes.lock().await;
            routes.retain(|r| r.pattern != rule.pattern);
            routes.push(rule);
        }
        let _ = self.enable_fetch_interception().await;
        Ok(())
    }

    /// Clears all registered mock route rules
    pub async fn clear_mock_routes(&self) -> Result<()> {
        {
            let mut routes = self.mock_routes.lock().await;
            routes.clear();
        }
        let blocked = self.blocked_urls.lock().await;
        if blocked.is_empty() {
            let _ = self.disable_fetch_interception().await;
        }
        Ok(())
    }

    /// Blocks a URL pattern (e.g. ad networks, analytics)
    #[allow(dead_code)]
    pub async fn block_url_pattern(&self, pattern: &str) -> Result<()> {
        {
            let mut blocked = self.blocked_urls.lock().await;
            if !blocked.iter().any(|p| p == pattern) {
                blocked.push(pattern.to_string());
            }
        }
        let _ = self.enable_fetch_interception().await;
        Ok(())
    }

    /// Clears all blocked URL patterns
    #[allow(dead_code)]
    pub async fn clear_blocked_urls(&self) -> Result<()> {
        {
            let mut blocked = self.blocked_urls.lock().await;
            blocked.clear();
        }
        let routes = self.mock_routes.lock().await;
        if routes.is_empty() {
            let _ = self.disable_fetch_interception().await;
        }
        Ok(())
    }

    /// Waits for a specific CDP page lifecycle event (e.g. "networkIdle", "DOMContentLoaded", "load")
    pub async fn wait_for_lifecycle_event(
        &self,
        target_event: &str,
        timeout_dur: Duration,
    ) -> Result<()> {
        let mut rx = self.lifecycle_tx.subscribe();
        let start = Instant::now();
        while start.elapsed() < timeout_dur {
            let remaining = timeout_dur.saturating_sub(start.elapsed());
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Ok(event)) if event == target_event => return Ok(()),
                Ok(Ok(_)) => continue,
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(broadcast::error::RecvError::Closed)) => break,
                Err(_) => break, // Timeout reached
            }
        }
        // Fallback: If lifecycle event didn't fire (e.g. Obscura or static page), proceed gracefully
        Ok(())
    }

    /// Waits until the page's network has settled (networkIdle lifecycle event)
    pub async fn wait_for_network_idle(&self, timeout_dur: Duration) -> Result<()> {
        self.wait_for_lifecycle_event("networkIdle", timeout_dur)
            .await
    }

    /// Waits dynamically until a CSS selector appears in the DOM or times out
    pub async fn wait_for_selector(&self, selector: &str, timeout_dur: Duration) -> Result<()> {
        let escaped = selector.replace('"', "\\\"");
        let check_script = format!("document.querySelector(\"{}\") !== null", escaped);
        let start = Instant::now();
        let poll_interval = Duration::from_millis(50);

        while start.elapsed() < timeout_dur {
            if let Ok(res) = self.evaluate_js(&check_script).await {
                if res.trim() == "true" {
                    return Ok(());
                }
            }
            tokio::time::sleep(poll_interval).await;
        }

        Err(ToolError::CommandExec(format!(
            "Timed out after {:?} waiting for selector '{}'",
            timeout_dur, selector
        ))
        .into())
    }

    /// Emulates device dimensions and viewport (e.g. mobile vs desktop)
    #[allow(dead_code)]
    pub async fn emulate_device(
        &self,
        width: u32,
        height: u32,
        mobile: bool,
        device_scale_factor: f64,
    ) -> Result<()> {
        self.send_command(
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": device_scale_factor,
                "mobile": mobile,
            }),
        )
        .await?;
        Ok(())
    }

    /// Emulates color scheme preference ('dark' or 'light')
    #[allow(dead_code)]
    pub async fn emulate_color_scheme(&self, scheme: &str) -> Result<()> {
        self.send_command(
            "Emulation.setEmulatedMedia",
            json!({
                "media": "screen",
                "features": [
                    { "name": "prefers-color-scheme", "value": scheme }
                ]
            }),
        )
        .await?;
        Ok(())
    }

    /// Navigates to the specified URL and waits for network idle or settled DOM
    pub async fn navigate(&self, url: &str) -> Result<()> {
        let current_url = self
            .evaluate_js("window.location.href")
            .await
            .unwrap_or_default();
        let clean_current = current_url.trim_matches('"').trim_end_matches('/');
        let clean_target = url.trim().trim_end_matches('/');

        if !clean_current.is_empty() && clean_current == clean_target {
            let _ = self.send_command("Page.reload", json!({})).await;
        } else {
            self.send_command("Page.navigate", json!({ "url": url }))
                .await?;
        }
        // Dynamically wait for network idle with fallback deadline
        let _ = self.wait_for_network_idle(Duration::from_millis(800)).await;
        // New document => reinstall the console shim.
        let _ = self.evaluate_js(CONSOLE_SHIM_JS).await;
        Ok(())
    }

    /// Retrieves full document HTML from the current page
    pub async fn get_document_html(&self) -> Result<String> {
        let res = self
            .send_command(
                "Runtime.evaluate",
                json!({
                    "expression": "document.documentElement ? document.documentElement.outerHTML : ''",
                    "returnByValue": true
                }),
            )
            .await?;

        let html = res
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        Ok(html.to_string())
    }

    /// Evaluates arbitrary JavaScript in the page context and returns the stringified result
    pub async fn evaluate_js(&self, script: &str) -> Result<String> {
        let safe_script = prepare_cdp_script(script);
        let res = self
            .send_command(
                "Runtime.evaluate",
                json!({
                    "expression": safe_script,
                    "returnByValue": true,
                    "awaitPromise": true
                }),
            )
            .await?;

        let val = res.get("result").and_then(|r| r.get("value"));
        let is_empty_or_undefined = match &val {
            None => true,
            Some(serde_json::Value::Null) => true,
            Some(serde_json::Value::String(s)) => s == "undefined" || s.trim().is_empty(),
            _ => false,
        };

        if is_empty_or_undefined && script.contains("console.") {
            if let Ok(console_res) = self
                .send_command(
                    "Runtime.evaluate",
                    json!({
                        "expression": "(window.__minicode_console || []).slice(-8).join('\\n')",
                        "returnByValue": true
                    }),
                )
                .await
            {
                if let Some(logs) = console_res
                    .get("result")
                    .and_then(|r| r.get("value"))
                    .and_then(|v| v.as_str())
                {
                    if !logs.trim().is_empty() {
                        return Ok(logs.to_string());
                    }
                }
            }
        }

        match val {
            Some(serde_json::Value::String(s)) => Ok(s.clone()),
            Some(other) => Ok(other.to_string()),
            None => {
                let desc = res
                    .get("result")
                    .and_then(|r| r.get("description"))
                    .and_then(|d| d.as_str())
                    .unwrap_or("undefined");
                Ok(desc.to_string())
            }
        }
    }

    /// Captures a viewport screenshot as PNG bytes
    pub async fn take_screenshot(&self) -> Result<Vec<u8>> {
        let res = self
            .send_command("Page.captureScreenshot", json!({ "format": "png" }))
            .await?;

        let base64_str = res.get("data").and_then(|d| d.as_str()).ok_or_else(|| {
            ToolError::CommandExec("Missing screenshot data in CDP response".to_string())
        })?;

        // Base64 decode
        let decoded = general_base64_decode(base64_str)?;
        Ok(decoded)
    }

    /// Clears any active device/viewport metric overrides
    pub async fn clear_device_metrics(&self) -> Result<()> {
        self.send_command("Emulation.clearDeviceMetricsOverride", json!({}))
            .await?;
        Ok(())
    }

    /// Dispatches a mouse event at (x, y) coordinates via CDP Input domain
    #[allow(clippy::too_many_arguments)]
    pub async fn dispatch_mouse_event(
        &self,
        event_type: &str,
        x: f64,
        y: f64,
        button: Option<&str>,
        click_count: Option<i32>,
        delta_x: Option<f64>,
        delta_y: Option<f64>,
    ) -> Result<()> {
        let mut params = json!({
            "type": event_type,
            "x": x,
            "y": y,
        });
        if let Some(b) = button {
            params["button"] = json!(b);
        }
        if let Some(c) = click_count {
            params["clickCount"] = json!(c);
        }
        if let Some(dx) = delta_x {
            params["deltaX"] = json!(dx);
        }
        if let Some(dy) = delta_y {
            params["deltaY"] = json!(dy);
        }
        let _ = self.send_command("Input.dispatchMouseEvent", params).await;
        Ok(())
    }

    /// Moves the mouse pointer to (x, y) coordinates via CDP
    #[allow(dead_code)]
    pub async fn mouse_move_to(&self, x: f64, y: f64) -> Result<()> {
        self.dispatch_mouse_event("mouseMoved", x, y, None, None, None, None)
            .await
    }

    /// Synthesizes a true OS-level mouse click at (x, y) coordinates via CDP
    pub async fn mouse_click_at(&self, x: f64, y: f64) -> Result<()> {
        let _ = self
            .dispatch_mouse_event("mouseMoved", x, y, None, None, None, None)
            .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        let _ = self
            .dispatch_mouse_event("mousePressed", x, y, Some("left"), Some(1), None, None)
            .await;
        tokio::time::sleep(Duration::from_millis(40)).await;
        let _ = self
            .dispatch_mouse_event("mouseReleased", x, y, Some("left"), Some(1), None, None)
            .await;
        Ok(())
    }

    /// Synthesizes mouse wheel scroll via CDP
    pub async fn mouse_wheel_scroll(
        &self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<()> {
        let _ = self
            .dispatch_mouse_event("mouseWheel", x, y, None, None, Some(delta_x), Some(delta_y))
            .await;
        Ok(())
    }

    /// Emulates network conditions (e.g. offline, slow 3G, fast 3G) over CDP
    pub async fn emulate_network_conditions(
        &self,
        offline: bool,
        latency_ms: f64,
        download_bytes_per_sec: f64,
        upload_bytes_per_sec: f64,
    ) -> Result<()> {
        let res = self
            .send_command(
                "Network.emulateNetworkConditions",
                json!({
                    "offline": offline,
                    "latency": latency_ms,
                    "downloadThroughput": download_bytes_per_sec,
                    "uploadThroughput": upload_bytes_per_sec,
                }),
            )
            .await;

        match res {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("Unknown")
                    || err_str.contains("not found")
                    || err_str.contains("not supported")
                {
                    tracing::warn!("Engine does not support Network.emulateNetworkConditions; proceeding with emulation");
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Retrieves all browser cookies for the current session
    pub async fn get_cookies(&self) -> Result<serde_json::Value> {
        let res = self.send_command("Network.getCookies", json!({})).await;
        match res {
            Ok(val) => Ok(val.get("cookies").cloned().unwrap_or_else(|| json!([]))),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("Unknown") || err_str.contains("not found") {
                    // Fallback to in-page document.cookie
                    let js_cookies = self
                        .evaluate_js("document.cookie")
                        .await
                        .unwrap_or_default();
                    let mut cookie_list = Vec::new();
                    for item in js_cookies.split(';') {
                        let parts: Vec<&str> = item.trim().splitn(2, '=').collect();
                        if parts.len() == 2 {
                            cookie_list.push(json!({
                                "name": parts[0],
                                "value": parts[1]
                            }));
                        }
                    }
                    Ok(json!(cookie_list))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Injects cookies into the active browser session
    pub async fn set_cookies(&self, cookies: serde_json::Value) -> Result<()> {
        if let Some(arr) = cookies.as_array() {
            let res = self
                .send_command("Network.setCookies", json!({ "cookies": arr }))
                .await;
            if let Err(e) = res {
                let err_str = e.to_string();
                if err_str.contains("Unknown") || err_str.contains("not found") {
                    for cookie in arr {
                        if let (Some(name), Some(val)) = (
                            cookie.get("name").and_then(|n| n.as_str()),
                            cookie.get("value").and_then(|v| v.as_str()),
                        ) {
                            let script = format!("document.cookie = '{}={}; path=/';", name, val);
                            let _ = self.evaluate_js(&script).await;
                        }
                    }
                    return Ok(());
                }
                return Err(e);
            }
        }
        Ok(())
    }

    /// Prints the active page to a PDF document as raw bytes
    pub async fn print_to_pdf(&self, landscape: bool, print_background: bool) -> Result<Vec<u8>> {
        let res = self
            .send_command(
                "Page.printToPDF",
                json!({
                    "landscape": landscape,
                    "printBackground": print_background,
                }),
            )
            .await;

        match res {
            Ok(val) => {
                let base64_str = val.get("data").and_then(|d| d.as_str()).ok_or_else(|| {
                    ToolError::CommandExec("Missing PDF data in CDP response".to_string())
                })?;
                let decoded = general_base64_decode(base64_str)?;
                Ok(decoded)
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("Unknown")
                    || err_str.contains("not found")
                    || err_str.contains("not supported")
                {
                    tracing::warn!("Engine does not support Page.printToPDF; generating structured PDF document fallback");
                    let title = self
                        .evaluate_js("document.title")
                        .await
                        .unwrap_or_else(|_| "Page Export".to_string());
                    let url = self
                        .evaluate_js("window.location.href")
                        .await
                        .unwrap_or_else(|_| "http://localhost".to_string());
                    let text = self
                        .evaluate_js("document.body ? document.body.innerText : ''")
                        .await
                        .unwrap_or_default();
                    Ok(generate_minimal_pdf(&title, &url, &text))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Lists all available targets (browser tabs, iframes, workers)
    pub async fn list_targets(&self) -> Result<Vec<serde_json::Value>> {
        let res = self.send_command("Target.getTargets", json!({})).await?;
        let targets = res
            .get("targetInfos")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(targets)
    }

    /// Creates a new browser tab/target and optionally attaches to it
    pub async fn create_tab(&self, url: &str, switch_to: bool) -> Result<String> {
        let created = self
            .send_command("Target.createTarget", json!({ "url": url }))
            .await?;
        let target_id = created
            .get("targetId")
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                ToolError::CommandExec("CDP createTarget returned no targetId".to_string())
            })?
            .to_string();

        if switch_to {
            self.switch_tab(&target_id).await?;
        }

        Ok(target_id)
    }

    /// Closes a browser tab by its targetId
    pub async fn close_tab(&self, target_id: &str) -> Result<()> {
        self.send_command("Target.closeTarget", json!({ "targetId": target_id }))
            .await?;
        Ok(())
    }

    /// Switches active CDP focus to a specific target tab
    pub async fn switch_tab(&self, target_id: &str) -> Result<()> {
        let attached = self
            .send_command(
                "Target.attachToTarget",
                json!({ "targetId": target_id, "flatten": true }),
            )
            .await?;
        let session = attached
            .get("sessionId")
            .and_then(|s| s.as_str())
            .ok_or_else(|| {
                ToolError::CommandExec("CDP attachToTarget returned no sessionId".to_string())
            })?
            .to_string();

        *self.session_id.lock().await = Some(session);
        self.enable_core_domains().await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn page_ws_url(&self) -> &str {
        &self.page_ws_url
    }
}

/// Wraps arbitrary JavaScript expressions/statements so Chrome CDP Runtime.evaluate
/// can evaluate multi-statement scripts, assignments, and returns without syntax errors.
pub fn prepare_cdp_script(script: &str) -> String {
    let trimmed = script.trim();
    if trimmed.starts_with("(()")
        || trimmed.starts_with("(function")
        || trimmed.starts_with("(async")
    {
        return script.to_string();
    }
    let has_await = trimmed.contains("await ") || trimmed.starts_with("await ");
    // If it's a simple one-liner with no semicolons, no await, and no return:
    if !trimmed.contains(';')
        && !trimmed.contains('\n')
        && !trimmed.contains("return ")
        && !has_await
    {
        return script.to_string();
    }
    // If it explicitly uses `return` or contains `await`:
    if has_await {
        if trimmed.contains("return ") || trimmed.contains("return;") {
            format!("(async () => {{\n{}\n}})()", script)
        } else {
            format!(
                "(async () => {{\n  try {{\n    return await eval({});\n  }} catch (_) {{\n    {}\n  }}\n}})()",
                serde_json::to_string(script).unwrap_or_else(|_| format!("{:?}", script)),
                script
            )
        }
    } else if trimmed.contains("return ") || trimmed.contains("return;") {
        format!("(() => {{\n{}\n}})()", script)
    } else {
        // Multi-statement script without return: wrap so the completion value is returned
        format!(
            "(() => {{\n  try {{\n    return eval({});\n  }} catch (_) {{\n    {}\n  }}\n}})()",
            serde_json::to_string(script).unwrap_or_else(|_| format!("{:?}", script)),
            script
        )
    }
}

/// Minimal base64 decoder without adding heavy extra dependencies
fn general_base64_decode(input: &str) -> Result<Vec<u8>> {
    let clean = input.trim().replace(['\r', '\n'], "");
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let chars: Vec<char> = clean.chars().collect();

    let decode_char = |c: char| -> Option<u8> {
        match c {
            'A'..='Z' => Some(c as u8 - b'A'),
            'a'..='z' => Some(c as u8 - b'a' + 26),
            '0'..='9' => Some(c as u8 - b'0' + 52),
            '+' | '-' => Some(62),
            '/' | '_' => Some(63),
            '=' => None,
            _ => None,
        }
    };

    let mut i = 0;
    while i < chars.len() {
        if i + 3 >= chars.len() {
            break;
        }
        let b0 = decode_char(chars[i]).unwrap_or(0);
        let b1 = decode_char(chars[i + 1]).unwrap_or(0);
        let b2 = decode_char(chars[i + 2]).unwrap_or(0);
        let b3 = decode_char(chars[i + 3]).unwrap_or(0);

        out.push((b0 << 2) | (b1 >> 4));
        if chars[i + 2] != '=' {
            out.push((b1 << 4) | (b2 >> 2));
        }
        if chars[i + 3] != '=' {
            out.push((b2 << 6) | b3);
        }
        i += 4;
    }

    Ok(out)
}

/// Minimal base64 encoder without adding heavy extra dependencies
pub fn general_base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        out.push(CHARS[(b0 >> 2) as usize] as char);
        out.push(CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if chunk.len() > 1 {
            out.push(CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }

        if chunk.len() > 2 {
            out.push(CHARS[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Helper to match URLs against glob/wildcard patterns (e.g. "*/api/user*", "*.google.com")
pub fn matches_pattern(pattern: &str, text: &str) -> bool {
    if pattern == "*" || pattern == text {
        return true;
    }
    if let Ok(re) = regex::Regex::new(&wildcard_to_regex(pattern)) {
        re.is_match(text)
    } else {
        text.contains(pattern)
    }
}

fn wildcard_to_regex(pat: &str) -> String {
    let mut s = String::from("^");
    for c in pat.chars() {
        match c {
            '*' => s.push_str(".*"),
            '?' => s.push('.'),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                s.push('\\');
                s.push(c);
            }
            _ => s.push(c),
        }
    }
    s.push('$');
    s
}

/// Fallback minimal PDF document generator for browser engines lacking native Page.printToPDF
fn generate_minimal_pdf(title: &str, url: &str, text: &str) -> Vec<u8> {
    let mut clean_text = String::new();
    clean_text.push_str(&format!("{}\\n\\nSource: {}\\n\\n", title, url));
    for line in text.lines().take(40) {
        let escaped = line
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        clean_text.push_str(&escaped);
        clean_text.push_str("\\n");
    }

    let stream = format!(
        "BT /F1 12 Tf 50 750 Td 14 TL ({}) Tj ET",
        clean_text.replace('\n', " ")
    );
    let stream_len = stream.len();

    let pdf = format!(
        "%PDF-1.4\n\
        1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
        2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
        3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj\n\
        4 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n\
        5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n\
        xref\n0 6\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \n0000000115 00000 n \n0000000244 00000 n \n0000000305 00000 n \n\
        trailer << /Size 6 /Root 1 0 R >>\nstartxref\n380\n%%EOF\n",
        stream_len, stream
    );
    pdf.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode_and_decode() {
        // "Hello" in base64 is "SGVsbG8="
        let encoded = general_base64_encode(b"Hello");
        assert_eq!(encoded, "SGVsbG8=");
        let decoded = general_base64_decode(&encoded).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello");

        // "Minicode" in base64 is "TWluaWNvZGU="
        let encoded2 = general_base64_encode(b"Minicode");
        assert_eq!(encoded2, "TWluaWNvZGU=");
        let decoded2 = general_base64_decode(&encoded2).unwrap();
        assert_eq!(String::from_utf8(decoded2).unwrap(), "Minicode");

        // Empty bytes
        assert_eq!(general_base64_encode(b""), "");
        assert_eq!(general_base64_decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("*", "https://example.com/api/test"));
        assert!(matches_pattern(
            "*/api/*",
            "http://localhost:3000/api/users"
        ));
        assert!(matches_pattern(
            "*.google-analytics.com*",
            "https://ssl.google-analytics.com/ga.js"
        ));
        assert!(!matches_pattern(
            "*/api/v2/*",
            "http://localhost:3000/api/v1/users"
        ));
    }

    #[test]
    fn test_prepare_cdp_script() {
        // Single expression passthrough
        assert_eq!(prepare_cdp_script("document.title"), "document.title");

        // Already wrapped in IIFE
        let wrapped = "(() => { return 42; })()";
        assert_eq!(prepare_cdp_script(wrapped), wrapped);

        // Explicit return script
        let with_return = "const x = 10;\nreturn x * 2;";
        let prep1 = prepare_cdp_script(with_return);
        assert!(prep1.starts_with("(() => {"));
        assert!(prep1.contains("return x * 2;"));

        // Multi-statement script without return
        let multi = "document.getElementById('email').value = 'test@example.com'; 'Done'";
        let prep2 = prepare_cdp_script(multi);
        assert!(prep2.starts_with("(() => {"));
        assert!(prep2.contains("return eval("));
    }
}
