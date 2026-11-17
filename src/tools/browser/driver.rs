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

/// Console-capture and instant-scroll automation shim:
/// 1. Wraps `console.*` and `window.onerror` into an in-page buffer (`window.__minicode_console`).
/// 2. Overrides CSS `scroll-behavior: smooth` with `auto !important` and normalizes
///    `scrollIntoView` / `scrollTo` / `scroll` / `scrollBy` options to `behavior: 'instant'`
///    so chained scroll commands and screenshot captures execute synchronously in `<1ms`.
const CONSOLE_SHIM_JS: &str = r#"(function(){var forceInstantScroll=function(){try{if(document.documentElement){document.documentElement.style.setProperty('scroll-behavior','auto','important')}if(document.body){document.body.style.setProperty('scroll-behavior','auto','important')}if(document.head&&!document.getElementById('__minicode_scroll_override')){var st=document.createElement('style');st.id='__minicode_scroll_override';st.textContent='html,body,* { scroll-behavior: auto !important; }';document.head.appendChild(st)}}catch(e){}};forceInstantScroll();if(window.__minicode_console)return;window.__minicode_console=[];['log','info','warn','error'].forEach(function(m){var orig=console[m]?console[m].bind(console):function(){};console[m]=function(){try{window.__minicode_console.push('['+m+'] '+Array.prototype.map.call(arguments,function(a){try{return typeof a==='object'?JSON.stringify(a):String(a)}catch(e){return String(a)}}).join(' '))}catch(e){}orig.apply(null,arguments)}});window.addEventListener('error',function(e){window.__minicode_console.push('[error] '+(e.message||'unknown'))});window.addEventListener('DOMContentLoaded',forceInstantScroll);if(typeof Element!=='undefined'&&Element.prototype.scrollIntoView){var origSIV=Element.prototype.scrollIntoView;Element.prototype.scrollIntoView=function(arg){forceInstantScroll();if(arg&&typeof arg==='object'){return origSIV.call(this,Object.assign({},arg,{behavior:'instant'}))}return origSIV.call(this,{behavior:'instant',block:arg===false?'end':'start',inline:'nearest'})}}['scrollTo','scroll','scrollBy'].forEach(function(fn){if(typeof window[fn]==='function'){var origFn=window[fn].bind(window);window[fn]=function(a,b){forceInstantScroll();if(a&&typeof a==='object'){return origFn(Object.assign({},a,{behavior:'instant'}))}return origFn.apply(null,arguments)}}})})()"#;

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
    next_id: Arc<AtomicU64>,
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
    /// Optional W3C WebDriver HTTP / WebDriver BiDi adapter for Safari (`safaridriver`) and Firefox 141+.
    webdriver: Option<Arc<super::webdriver::WebDriverAdapter>>,
}

impl CdpClient {
    /// Connects to a running browser engine, selecting native CDP (`MiniBrowse`, `Chrome`, `Obscura`,
    /// Firefox <= 140), W3C WebDriver BiDi (`Firefox 141+` `--remote-debugging-port`), or W3C WebDriver HTTP
    /// (`Safari` `safaridriver` / `geckodriver`).
    pub async fn connect_for_engine(
        cdp_http_url: &str,
        engine: super::engine::BrowserEngine,
        mode: super::engine::BrowserMode,
    ) -> Result<Self> {
        match engine {
            super::engine::BrowserEngine::Safari => {
                Self::connect_w3c_webdriver(cdp_http_url, engine, mode).await
            }
            super::engine::BrowserEngine::Firefox => {
                if let Ok(cdp) = Self::connect(cdp_http_url).await {
                    return Ok(cdp);
                }
                if let Ok(bidi) = Self::connect_webdriver_bidi(cdp_http_url).await {
                    return Ok(bidi);
                }
                Self::connect_w3c_webdriver(cdp_http_url, engine, mode).await
            }
            _ => Self::connect(cdp_http_url).await,
        }
    }

    /// Connects explicitly to a W3C WebDriver HTTP endpoint (`safaridriver` / `geckodriver`).
    pub async fn connect_w3c_webdriver(
        http_base: &str,
        engine: super::engine::BrowserEngine,
        mode: super::engine::BrowserMode,
    ) -> Result<Self> {
        let (lifecycle_tx, _) = broadcast::channel(128);
        let adapter = super::webdriver::WebDriverAdapter::connect_w3c_http(
            http_base,
            engine,
            mode,
            lifecycle_tx.clone(),
        )
        .await?;
        Self::from_webdriver_adapter(
            http_base,
            adapter,
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(DebugCollector::new()),
            lifecycle_tx,
        )
        .await
    }

    /// Connects explicitly to a W3C WebDriver BiDi WebSocket endpoint (`ws://127.0.0.1:<port>/session`).
    pub async fn connect_webdriver_bidi(http_base: &str) -> Result<Self> {
        let (lifecycle_tx, _) = broadcast::channel(128);
        let console_log = Arc::new(Mutex::new(Vec::new()));
        let debug_collector = Arc::new(DebugCollector::new());
        let adapter = super::webdriver::WebDriverAdapter::connect_bidi_ws(
            http_base,
            Arc::clone(&console_log),
            Arc::clone(&debug_collector),
            lifecycle_tx.clone(),
        )
        .await?;
        Self::from_webdriver_adapter(
            http_base,
            adapter,
            console_log,
            debug_collector,
            lifecycle_tx,
        )
        .await
    }

    async fn from_webdriver_adapter(
        endpoint: &str,
        adapter: super::webdriver::WebDriverAdapter,
        console_log: Arc<Mutex<Vec<String>>>,
        debug_collector: Arc<DebugCollector>,
        lifecycle_tx: broadcast::Sender<String>,
    ) -> Result<Self> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let client = Self {
            tx,
            next_id: Arc::new(AtomicU64::new(1)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            page_ws_url: endpoint.to_string(),
            session_id: Mutex::new(None),
            console_log,
            debug_collector,
            mock_routes: Arc::new(Mutex::new(Vec::new())),
            blocked_urls: Arc::new(Mutex::new(Vec::new())),
            lifecycle_tx,
            webdriver: Some(Arc::new(adapter)),
        };
        let _ = client.enable_core_domains().await;
        let _ = client.evaluate_js(CONSOLE_SHIM_JS).await;
        Ok(client)
    }

    /// Connects to a running browser engine via its HTTP base URL (e.g. "http://127.0.0.1:9222")
    pub async fn connect(cdp_http_url: &str) -> Result<Self> {
        // Prefer the browser-level endpoint + explicit target attachment:
        // works on Chrome/Firefox and is REQUIRED by Obscura.
        let (page_ws_url, wants_session) =
            match Self::resolve_browser_websocket_url(cdp_http_url).await {
                Ok(url) => (url, true),
                Err(_) => (Self::resolve_page_websocket_url(cdp_http_url).await?, false),
            };
        Self::connect_ws(&page_ws_url, wants_session, true).await
    }

    /// Attaches directly to a browser-level WebSocket URL (e.g. from Chrome/Brave `DevToolsActivePort`)
    /// opening a fresh background tab without activating or stealing focus from the user's active tab.
    pub async fn connect_browser_ws_background(ws_url: &str) -> Result<Self> {
        Self::connect_ws(ws_url, true, false).await
    }

    async fn connect_ws(
        page_ws_url: &str,
        wants_session: bool,
        activate_on_attach: bool,
    ) -> Result<Self> {
        tracing::info!(ws_url = %page_ws_url, "Connecting to browser CDP WebSocket");

        let (ws_stream, _) = tokio_tungstenite::connect_async(page_ws_url)
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
                                    let _ = sender.send(val);
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
                                            if super::debug::DebugCollector::is_hmr_hot_update_message(
                                                &msg,
                                            ) {
                                                debug_collector_clone
                                                    .evict_stale_console_errors_before_hmr(
                                                        std::time::Duration::from_millis(750),
                                                    );
                                            }
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
                                                send_cdp_event_reply(
                                                    &tx_clone,
                                                    999_998,
                                                    "Fetch.failRequest",
                                                    json!({
                                                        "requestId": req_id,
                                                        "errorReason": "BlockedByClient"
                                                    }),
                                                    session_id_opt,
                                                );
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
                                                send_cdp_event_reply(
                                                    &tx_clone,
                                                    999_998,
                                                    "Fetch.fulfillRequest",
                                                    json!({
                                                        "requestId": req_id,
                                                        "responseCode": rule.status,
                                                        "responsePhrase": phrase,
                                                        "responseHeaders": [
                                                            { "name": "Content-Type", "value": content_type },
                                                            { "name": "Access-Control-Allow-Origin", "value": "*" }
                                                        ],
                                                        "body": encoded_body
                                                    }),
                                                    session_id_opt,
                                                );
                                            } else {
                                                send_cdp_event_reply(
                                                    &tx_clone,
                                                    999_998,
                                                    "Fetch.continueRequest",
                                                    json!({ "requestId": req_id }),
                                                    session_id_opt,
                                                );
                                            }
                                        }
                                    }
                                    // Auto-handle modal alerts and dialogs
                                    "Page.javascriptDialogOpening" => {
                                        tracing::info!("Auto-dismissing browser JavaScript dialog");
                                        let session_id_opt =
                                            val.get("sessionId").and_then(|s| s.as_str());
                                        send_cdp_event_reply(
                                            &tx_clone,
                                            999_999,
                                            "Page.handleJavaScriptDialog",
                                            json!({ "accept": true }),
                                            session_id_opt,
                                        );
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
            next_id: Arc::new(AtomicU64::new(1)),
            pending,
            page_ws_url: page_ws_url.to_string(),
            session_id: Mutex::new(None),
            console_log,
            debug_collector,
            mock_routes,
            blocked_urls,
            lifecycle_tx,
            webdriver: None,
        };

        if wants_session {
            client.attach_fresh_page(activate_on_attach).await?;
        }

        // Enable core domains
        client.enable_core_domains().await?;

        // Wrap console.* into an in-page buffer: some engines (Obscura) do not
        // emit Runtime.consoleAPICalled events, so capture at the source.
        let _ = client.evaluate_js(CONSOLE_SHIM_JS).await;

        Ok(client)
    }

    /// Creates a blank page target, attaches with a flatten session, and optionally activates it.
    /// When `activate == false` (local daily-driver browser attach), opens in background so user focus is untouched.
    async fn attach_fresh_page(&self, activate: bool) -> Result<()> {
        let target_id = self.create_tab("about:blank", false).await?;
        self.attach_to_target(&target_id).await?;
        if activate {
            let _ = self
                .send_command_with_timeout(
                    "Target.activateTarget",
                    json!({ "targetId": target_id }),
                    Duration::from_millis(1500),
                )
                .await;
            let _ = self
                .send_command_with_timeout(
                    "Page.bringToFront",
                    json!({}),
                    Duration::from_millis(1500),
                )
                .await;
        }
        Ok(())
    }

    async fn attach_to_target(&self, target_id: &str) -> Result<()> {
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
                    let target_url = target.get("url").and_then(|u| u.as_str()).unwrap_or("");
                    if (target_type == "page" || target_type.is_empty())
                        && !is_internal_browser_target_url(target_url)
                    {
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
        if let Ok(ws_url) = Self::resolve_browser_websocket_url(http_base).await {
            return Ok(ws_url);
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
        self.send_command_with_timeout(
            method,
            params,
            Duration::from_millis(BROWSER_NAVIGATE_TIMEOUT_MS),
        )
        .await
    }

    /// Sends a JSON-RPC command to the browser with an explicit response timeout,
    /// automatically healing stale target sessions (`ensure_real_tab` recovery) once if a tab closed mid-flow.
    pub async fn send_command_with_timeout(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout_dur: Duration,
    ) -> Result<serde_json::Value> {
        match self
            .send_command_once(method, params.clone(), timeout_dur)
            .await
        {
            Ok(val) => Ok(val),
            Err(e) => {
                let err_str = e.to_string();
                let is_stale_session = !method.starts_with("Target.")
                    && (err_str.contains("Session with given id not found")
                        || err_str.contains("Target closed")
                        || err_str.contains("No target with given id"));
                if is_stale_session && self.ensure_real_tab().await.is_ok() {
                    return self.send_command_once(method, params, timeout_dur).await;
                }
                Err(e)
            }
        }
    }

    async fn send_command_once(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout_dur: Duration,
    ) -> Result<serde_json::Value> {
        if let Some(ref wd) = self.webdriver {
            let sid = self.session_id.lock().await.clone();
            return wd
                .execute_cdp_command(method, params, sid.as_deref(), timeout_dur)
                .await;
        }

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

    /// Enables standard DevTools domains and initializes a standard 1440x900 desktop viewport
    pub async fn enable_core_domains(&self) -> Result<()> {
        let _ = self.send_command("Page.enable", json!({})).await;
        let _ = self
            .send_command("Page.setLifecycleEventsEnabled", json!({ "enabled": true }))
            .await;
        let _ = self
            .send_command(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": CONSOLE_SHIM_JS }),
            )
            .await;
        // Pre-register PageAgent probe at document-start so `window.__minicode_page_agent` is
        // immediately live on every navigation in MiniBrowse & Chrome without repeated 45KB transfers.
        let _ = self
            .send_command(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": super::page_agent::PAGE_PROBE_JS }),
            )
            .await;
        let _ = self.send_command("Runtime.enable", json!({})).await;
        let _ = self.send_command("DOM.enable", json!({})).await;
        let _ = self.send_command("Network.enable", json!({})).await;
        let _ = self
            .send_command("Network.setCacheDisabled", json!({ "cacheDisabled": true }))
            .await;
        let _ = self.send_command("Log.enable", json!({})).await;
        let _ = self
            .send_command(
                "Emulation.setDeviceMetricsOverride",
                json!({
                    "width": 1440,
                    "height": 900,
                    "deviceScaleFactor": 1.0,
                    "mobile": false
                }),
            )
            .await;
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

    /// Registers a mock route rule and activates Fetch domain + MiniBrowse native interception
    pub async fn add_mock_route(&self, rule: MockRouteRule) -> Result<()> {
        let snapshot = {
            let mut routes = self.mock_routes.lock().await;
            routes.retain(|r| r.pattern != rule.pattern);
            routes.push(rule);
            routes.clone()
        };
        let _ = self.enable_fetch_interception().await;
        let _ = self
            .send_command_with_timeout(
                "MiniBrowse.setMockRoutes",
                json!({ "routes": snapshot }),
                Duration::from_millis(800),
            )
            .await;
        Ok(())
    }

    /// Clears all registered mock route rules
    pub async fn clear_mock_routes(&self) -> Result<()> {
        {
            let mut routes = self.mock_routes.lock().await;
            routes.clear();
        }
        let _ = self
            .send_command_with_timeout(
                "MiniBrowse.setMockRoutes",
                json!({ "routes": [] }),
                Duration::from_millis(800),
            )
            .await;
        let blocked = self.blocked_urls.lock().await;
        if blocked.is_empty() {
            let _ = self.disable_fetch_interception().await;
        }
        Ok(())
    }

    /// Blocks a URL pattern (e.g. ad networks, analytics)
    #[allow(dead_code)]
    pub async fn block_url_pattern(&self, pattern: &str) -> Result<()> {
        let snapshot = {
            let mut blocked = self.blocked_urls.lock().await;
            if !blocked.iter().any(|p| p == pattern) {
                blocked.push(pattern.to_string());
            }
            blocked.clone()
        };
        let _ = self.enable_fetch_interception().await;
        let _ = self
            .send_command_with_timeout(
                "MiniBrowse.setBlockedUrls",
                json!({ "patterns": snapshot }),
                Duration::from_millis(800),
            )
            .await;
        Ok(())
    }

    /// Clears all blocked URL patterns
    #[allow(dead_code)]
    pub async fn clear_blocked_urls(&self) -> Result<()> {
        {
            let mut blocked = self.blocked_urls.lock().await;
            blocked.clear();
        }
        let _ = self
            .send_command_with_timeout(
                "MiniBrowse.setBlockedUrls",
                json!({ "patterns": [] }),
                Duration::from_millis(800),
            )
            .await;
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

    /// Waits until the page's network and DOM mutations have settled (`MiniBrowse.waitForIdle`
    /// event-driven fast path, falling back to CDP `networkIdle` lifecycle events on Chrome/Firefox).
    pub async fn wait_for_network_idle(&self, timeout_dur: Duration) -> Result<()> {
        let timeout_ms = (timeout_dur.as_millis() as u64).max(40);
        let fast_idle = self
            .send_command_with_timeout(
                "MiniBrowse.waitForIdle",
                json!({
                    "quietMs": 25,
                    "timeoutMs": timeout_ms,
                }),
                timeout_dur + Duration::from_millis(500),
            )
            .await;
        match fast_idle {
            Ok(_) => Ok(()),
            Err(e) if is_unsupported_cdp_error(&e) => {
                self.wait_for_lifecycle_event("networkIdle", timeout_dur)
                    .await
            }
            Err(_) => Ok(()),
        }
    }

    /// Configures `MiniBrowse` resource loading profile (`"research"` skips heavy image/media/font
    /// downloads; `"full"` loads all visual assets for screenshots & layout QA). No-op on other engines.
    pub async fn set_minibrowse_resource_profile(&self, profile: &str) {
        let _ = self
            .send_command_with_timeout(
                "MiniBrowse.setResourceProfile",
                json!({ "profile": profile }),
                Duration::from_millis(800),
            )
            .await;
    }

    /// Waits dynamically until a CSS selector appears in the DOM or times out
    /// (uses zero-polling `MiniBrowse.waitForSelector` `MutationObserver` fast path when available).
    pub async fn wait_for_selector(&self, selector: &str, timeout_dur: Duration) -> Result<()> {
        let timeout_ms = (timeout_dur.as_millis() as u64).max(50);
        let mb_res = self
            .send_command_with_timeout(
                "MiniBrowse.waitForSelector",
                json!({
                    "selector": selector,
                    "state": "attached",
                    "timeoutMs": timeout_ms,
                }),
                timeout_dur + Duration::from_millis(800),
            )
            .await;
        match mb_res {
            Ok(val) => {
                if val.get("matched").and_then(|v| v.as_bool()) == Some(true) {
                    return Ok(());
                }
                return Err(ToolError::CommandExec(format!(
                    "Timed out after {:?} waiting for selector '{}'",
                    timeout_dur, selector
                ))
                .into());
            }
            Err(e) if is_unsupported_cdp_error(&e) => {}
            Err(_) => {}
        }

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
        // Keep `mobile: false` in `Emulation.setDeviceMetricsOverride` so Chromium headless
        // does not apply Android layout-viewport autoscaling (which expands `window.innerWidth`
        // from 375px to 642px/980px whenever any element overflows horizontally), while
        // enabling touch emulation separately when `mobile` is true.
        self.send_command(
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": device_scale_factor,
                "mobile": false,
                "screenWidth": width,
                "screenHeight": height,
            }),
        )
        .await?;
        let _ = self
            .send_command(
                "Emulation.setTouchEmulationEnabled",
                json!({
                    "enabled": mobile,
                }),
            )
            .await;
        self.reset_scroll_top().await;
        Ok(())
    }

    /// Resets the page scroll position to (0, 0) immediately, bypassing smooth scroll CSS.
    pub async fn reset_scroll_top(&self) {
        let _ = self
            .evaluate_js(
                "(() => { try { if ('scrollRestoration' in history) history.scrollRestoration = 'manual'; if (document.documentElement) document.documentElement.style.scrollBehavior = 'auto'; if (document.body) document.body.style.scrollBehavior = 'auto'; window.scrollTo(0, 0); if (document.documentElement) document.documentElement.scrollTop = 0; if (document.body) document.body.scrollTop = 0; } catch (_) {} return true; })()",
            )
            .await;
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
        let normalized_url = super::normalize_loopback_url(url);
        let target_url = normalized_url.as_str();

        let current_url = self
            .evaluate_js("window.location.href")
            .await
            .unwrap_or_default();
        let clean_current = current_url.trim_matches('"').trim_end_matches('/');
        let clean_target = target_url.trim().trim_end_matches('/');

        // Clear stale console logs and network errors from prior navigations before loading/reloading
        self.debug_collector.clear();

        let _ = self
            .send_command("Network.setCacheDisabled", json!({ "cacheDisabled": true }))
            .await;

        if !clean_current.is_empty() && clean_current == clean_target {
            let _ = self
                .send_command("Page.reload", json!({ "ignoreCache": true }))
                .await;
        } else {
            self.send_command("Page.navigate", json!({ "url": target_url }))
                .await?;
        }
        // Dynamically wait for network idle with fallback deadline
        let _ = self.wait_for_network_idle(Duration::from_millis(800)).await;
        // On cold Vite/Next/SPA dev servers, ES module compilation can finish a few hundred ms
        // after initial network idle while `#root`/`#app`/`#__next` still has 0 children.
        let spa_hydrate_js = r#"(async () => {
            const root = document.querySelector('#root, #app, #__next');
            if (!root || root.childElementCount > 0 || (root.textContent || '').trim().length > 0) {
                return true;
            }
            for (let i = 0; i < 25; i++) {
                await new Promise(r => setTimeout(r, 60));
                if (root.childElementCount > 0 || (root.textContent || '').trim().length > 0) {
                    await new Promise(r => requestAnimationFrame(() => r()));
                    return true;
                }
            }
            return false;
        })()"#;
        let _ = self.evaluate_js(spa_hydrate_js).await;
        if let Some((_, raw_frag)) = target_url.split_once('#') {
            let frag = raw_frag.trim();
            if !frag.is_empty() && !frag.starts_with('/') {
                if let Ok(frag_json) = serde_json::to_string(frag) {
                    let scroll_frag_js = format!(
                        r#"(async () => {{
                            const id = {frag_json};
                            for (let attempt = 0; attempt < 6; attempt++) {{
                                const el = document.getElementById(id) || document.querySelector('[name=' + JSON.stringify(id) + ']');
                                if (el) {{
                                    el.scrollIntoView({{ behavior: 'instant', block: 'start' }});
                                    return true;
                                }}
                                await new Promise(r => setTimeout(r, 50));
                            }}
                            return false;
                        }})()"#
                    );
                    let _ = self.evaluate_js(&scroll_frag_js).await;
                }
            }
        } else {
            self.reset_scroll_top().await;
        }
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

    /// Evaluates arbitrary JavaScript in the page context and returns `Err` if V8 throws an uncaught exception.
    pub async fn evaluate_js_strict(&self, script: &str) -> Result<String> {
        self.evaluate_js_inner(script, true).await
    }

    /// Evaluates arbitrary JavaScript in the page context and returns the stringified result
    pub async fn evaluate_js(&self, script: &str) -> Result<String> {
        self.evaluate_js_inner(script, false).await
    }

    async fn evaluate_js_inner(&self, script: &str, strict_exceptions: bool) -> Result<String> {
        let uses_console = script.contains("console.");
        let console_start_idx: usize = if uses_console {
            self.send_command(
                "Runtime.evaluate",
                json!({
                    "expression": "(window.__minicode_console || []).length",
                    "returnByValue": true
                }),
            )
            .await
            .ok()
            .and_then(|r| {
                r.get("result")
                    .and_then(|v| v.get("value"))
                    .and_then(|v| v.as_u64())
            })
            .unwrap_or(0) as usize
        } else {
            0
        };

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

        if let Some(exc) = res.get("exceptionDetails") {
            let desc = exc
                .get("exception")
                .and_then(|e| {
                    e.get("description")
                        .and_then(|d| d.as_str())
                        .or_else(|| e.get("value").and_then(|v| v.as_str()))
                })
                .or_else(|| {
                    res.get("result")
                        .and_then(|r| r.get("description"))
                        .and_then(|d| d.as_str())
                })
                .or_else(|| exc.get("text").and_then(|t| t.as_str()))
                .unwrap_or("Uncaught JavaScript exception");
            let first_line = desc.lines().next().unwrap_or(desc).trim();
            if strict_exceptions {
                return Err(crate::error::ToolError::ExecutionFailed(format!(
                    "JavaScript evaluation failed: {}",
                    first_line
                ))
                .into());
            }
            return Ok(format!("Error: {}", first_line));
        }

        let val = res.get("result").and_then(|r| r.get("value"));
        let is_empty_or_undefined = match &val {
            None => true,
            Some(serde_json::Value::Null) => true,
            Some(serde_json::Value::String(s)) => s == "undefined" || s.trim().is_empty(),
            _ => false,
        };

        if is_empty_or_undefined && uses_console {
            let delta_expr = format!(
                "(window.__minicode_console || []).slice({}).join('\\n')",
                console_start_idx
            );
            if let Ok(console_res) = self
                .send_command(
                    "Runtime.evaluate",
                    json!({
                        "expression": delta_expr,
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

    /// Captures a viewport screenshot as PNG bytes with automatic GUI compositor wake-up fallback.
    pub async fn take_screenshot(&self) -> Result<Vec<u8>> {
        // Temporarily hide any in-page simulator aura overlay so screenshots are always clean
        let _ = tokio::time::timeout(
            Duration::from_millis(800),
            self.evaluate_js(
                "(() => { const el = document.getElementById('minicode-simulator-container'); if (el) { el.dataset.prevDisplay = el.style.display || ''; el.style.setProperty('display', 'none', 'important'); } return true; })()",
            ),
        )
        .await;
        // Bound font-readiness wait so a hung font promise never blocks screenshot capture
        let _ = tokio::time::timeout(
            Duration::from_millis(1500),
            self.evaluate_js("document.fonts ? document.fonts.ready.then(() => true) : true"),
        )
        .await;
        // Ensure active tab is foregrounded so GUI compositors produce frames even after deep scroll
        let _ = self
            .send_command_with_timeout("Page.bringToFront", json!({}), Duration::from_millis(1200))
            .await;
        tokio::time::sleep(Duration::from_millis(250)).await;

        let primary = self
            .send_command_with_timeout(
                "Page.captureScreenshot",
                json!({ "format": "png", "fromSurface": true }),
                Duration::from_millis(5000),
            )
            .await;

        let res = match primary {
            Ok(val) => Ok(val),
            Err(e) if is_unsupported_cdp_error(&e) => {
                self.send_command_with_timeout(
                    "Page.captureScreenshot",
                    json!({ "format": "png" }),
                    Duration::from_millis(5000),
                )
                .await
            }
            Err(_) => {
                // GUI compositor occlusion / deep-scroll stall recovery: force focus emulation,
                // bring page to front, read current viewport bounds, and capture explicit viewport clip.
                let _ = self
                    .send_command_with_timeout(
                        "Emulation.setFocusEmulationEnabled",
                        json!({ "enabled": true }),
                        Duration::from_millis(1200),
                    )
                    .await;
                let _ = self
                    .send_command_with_timeout(
                        "Page.bringToFront",
                        json!({}),
                        Duration::from_millis(1200),
                    )
                    .await;
                let vp_raw = tokio::time::timeout(
                    Duration::from_millis(1500),
                    self.evaluate_js(
                        "JSON.stringify({ x: Math.max(0, window.scrollX || 0), y: Math.max(0, window.scrollY || 0), w: Math.max(320, window.innerWidth || 1280), h: Math.max(240, window.innerHeight || 800) })",
                    ),
                )
                .await
                .ok()
                .and_then(|r| r.ok())
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

                let mut retry_params = json!({
                    "format": "png",
                    "fromSurface": true,
                    "captureBeyondViewport": false,
                });
                if let Some(vp) = vp_raw {
                    let x = vp.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = vp.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let w = vp.get("w").and_then(|v| v.as_f64()).unwrap_or(1280.0);
                    let h = vp.get("h").and_then(|v| v.as_f64()).unwrap_or(800.0);
                    retry_params["clip"] = json!({
                        "x": x,
                        "y": y,
                        "width": w,
                        "height": h,
                        "scale": 1.0
                    });
                }
                self.send_command_with_timeout(
                    "Page.captureScreenshot",
                    retry_params,
                    Duration::from_millis(6000),
                )
                .await
            }
        };

        let _ = tokio::time::timeout(
            Duration::from_millis(600),
            self.evaluate_js(
                "(() => { const el = document.getElementById('minicode-simulator-container'); if (el) { const prev = el.dataset.prevDisplay || ''; delete el.dataset.prevDisplay; if (prev) el.style.display = prev; else el.style.removeProperty('display'); } return true; })()",
            ),
        )
        .await;

        let res = res?;
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
        self.reset_scroll_top().await;
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

    /// Dispatches a native CDP `keyDown` + `keyUp` pair (`Input.dispatchKeyEvent`)
    pub async fn dispatch_key_press(&self, key: &str, modifiers: u32) -> Result<()> {
        let _ = self
            .send_command(
                "Input.dispatchKeyEvent",
                json!({
                    "type": "keyDown",
                    "key": key,
                    "code": key,
                    "modifiers": modifiers,
                }),
            )
            .await;
        let _ = self
            .send_command(
                "Input.dispatchKeyEvent",
                json!({
                    "type": "keyUp",
                    "key": key,
                    "code": key,
                    "modifiers": modifiers,
                }),
            )
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
            Err(e) if is_unsupported_cdp_error(&e) => {
                tracing::warn!("Engine does not support Network.emulateNetworkConditions; proceeding with emulation");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Retrieves all browser cookies for the current session
    pub async fn get_cookies(&self) -> Result<serde_json::Value> {
        let res = self.send_command("Network.getCookies", json!({})).await;
        match res {
            Ok(val) => Ok(val.get("cookies").cloned().unwrap_or_else(|| json!([]))),
            Err(e) if is_unsupported_cdp_error(&e) => {
                // Fallback to in-page document.cookie
                let js_cookies = self
                    .evaluate_js("document.cookie")
                    .await
                    .unwrap_or_default();
                let mut cookie_list = Vec::new();
                for item in js_cookies.split(';') {
                    if let Some((name, val)) = item.trim().split_once('=') {
                        cookie_list.push(json!({
                            "name": name,
                            "value": val
                        }));
                    }
                }
                Ok(json!(cookie_list))
            }
            Err(e) => Err(e),
        }
    }

    /// Injects cookies into the active browser session
    pub async fn set_cookies(&self, cookies: serde_json::Value) -> Result<()> {
        if let Some(arr) = cookies.as_array() {
            let res = self
                .send_command("Network.setCookies", json!({ "cookies": arr }))
                .await;
            if let Err(e) = res {
                if is_unsupported_cdp_error(&e) {
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
            Err(e) if is_unsupported_cdp_error(&e) => {
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
            }
            Err(e) => Err(e),
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
            .send_command(
                "Target.createTarget",
                json!({ "url": url, "background": false }),
            )
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
        self.attach_to_target(target_id).await?;
        let _ = self
            .send_command_with_timeout(
                "Target.activateTarget",
                json!({ "targetId": target_id }),
                Duration::from_millis(1500),
            )
            .await;
        let _ = self
            .send_command_with_timeout("Page.bringToFront", json!({}), Duration::from_millis(1500))
            .await;
        self.enable_core_domains().await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn page_ws_url(&self) -> &str {
        &self.page_ws_url
    }

    /// Executes Prune4Web information-density research extraction via `MiniBrowse.researchExtract`
    /// (with automatic pure-JS fallback when connected to Chrome/Firefox).
    pub async fn minibrowse_research(
        &self,
        query: Option<&str>,
        max_chars: usize,
    ) -> Result<serde_json::Value> {
        self.minibrowse_research_with_format(query, max_chars, None)
            .await
    }

    /// Executes `MiniBrowse.researchExtract` supporting both `"markdown"` and `"semantic_tree"` formats.
    pub async fn minibrowse_research_with_format(
        &self,
        query: Option<&str>,
        max_chars: usize,
        format: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut params = json!({
            "query": query.unwrap_or(""),
            "maxChars": max_chars,
        });
        if let Some(fmt) = format.filter(|s| !s.trim().is_empty()) {
            params["format"] = json!(fmt);
        }
        let res = self
            .send_command_with_timeout(
                "MiniBrowse.researchExtract",
                params,
                Duration::from_secs(10),
            )
            .await;

        match res {
            Ok(val) => Ok(val),
            Err(e) if is_unsupported_cdp_error(&e) => {
                let html = self.get_document_html().await?;
                let title = self.evaluate_js("document.title").await.unwrap_or_default();
                let cur_url = self
                    .evaluate_js("window.location.href")
                    .await
                    .unwrap_or_default();
                let mut md =
                    super::markdown::SmartMarkdownExtractor::extract_fit_markdown(&html, query);
                if md.len() > max_chars {
                    md.truncate(md.floor_char_boundary(max_chars));
                }
                Ok(json!({
                    "url": cur_url,
                    "title": title,
                    "engine": "cdp-fallback",
                    "markdown": md,
                    "citations": [],
                }))
            }
            Err(e) => Err(e),
        }
    }

    /// Runs deep Web & framework root-cause diagnostics (`MiniBrowse.webDebugAudit`)
    /// piercing Vite `<vite-error-overlay>` / Next.js `<nextjs-portal>` Shadow DOM,
    /// computed CSS overflow culprits, WCAG 2.1 contrast ratios, and fixed-header H1 occlusion.
    pub async fn minibrowse_web_diagnose(&self) -> Result<serde_json::Value> {
        let res = self
            .send_command_with_timeout(
                "MiniBrowse.webDebugAudit",
                json!({}),
                Duration::from_secs(8),
            )
            .await;

        match res {
            Ok(val) => Ok(val),
            Err(e) if is_unsupported_cdp_error(&e) => {
                let fallback_js = r#"(() => {
                    const overlays = [];
                    const vite = document.querySelector('vite-error-overlay');
                    if (vite) {
                        const msg = (vite.shadowRoot ? vite.shadowRoot.textContent : vite.textContent) || '';
                        if (msg.trim()) overlays.push({ framework: 'vite', error: msg.trim().slice(0, 1200) });
                    }
                    const next = document.querySelector('nextjs-portal');
                    if (next && next.shadowRoot) {
                        const msg = next.shadowRoot.textContent || '';
                        if (msg.trim()) overlays.push({ framework: 'nextjs', error: msg.trim().slice(0, 1200) });
                    }
                    const vw = window.innerWidth || 1280;
                    const docW = document.documentElement ? document.documentElement.scrollWidth : vw;
                    const culprits = [];
                    for (const el of document.querySelectorAll('body *')) {
                        if (culprits.length >= 10) break;
                        const r = el.getBoundingClientRect();
                        if (r.width > 0 && r.height > 0 && r.right > vw + 2) {
                            const cs = getComputedStyle(el);
                            culprits.push({
                                selector: el.id ? '#' + el.id : el.tagName.toLowerCase(),
                                overflow_px: Math.round(r.right - vw),
                                computed: { display: cs.display, width: cs.width, minWidth: cs.minWidth, overflowX: cs.overflowX }
                            });
                        }
                    }
                    return JSON.stringify({
                        url: location.href,
                        title: document.title || '',
                        viewport: { width: vw, height: window.innerHeight || 800 },
                        page_horizontal_overflow_px: Math.max(0, docW - vw),
                        framework_overlays: overlays,
                        overflow_culprits: culprits,
                        contrast_issues: [],
                        fixed_header_occlusion: null
                    });
                })()"#;
                let raw = self.evaluate_js(fallback_js).await?;
                let parsed = serde_json::from_str::<serde_json::Value>(&raw)
                    .unwrap_or_else(|_| json!({ "raw": raw }));
                Ok(parsed)
            }
            Err(e) => Err(e),
        }
    }

    /// Inspects Flutter Web Semantics (`<flt-semantics-placeholder>`) + Differential UI Tree +
    /// Dart VM Service JSON-RPC (`MiniBrowse.flutterInspect`).
    pub async fn minibrowse_flutter_inspect(
        &self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let res = self
            .send_command_with_timeout("MiniBrowse.flutterInspect", params, Duration::from_secs(10))
            .await;

        match res {
            Ok(val) => Ok(val),
            Err(e) if is_unsupported_cdp_error(&e) => {
                let flt_js = r#"(() => {
                    const ph = document.querySelector('flt-semantics-placeholder');
                    if (ph) { try { ph.click(); } catch (_) {} }
                    const nodes = Array.from(document.querySelectorAll('flt-semantics[role], [role], button, a, input')).slice(0, 80);
                    return JSON.stringify({
                        semantics_ui: {
                            flutter_semantics_active: Boolean(document.querySelector('flt-semantics, flt-semantics-host')),
                            total_elements: nodes.length,
                            elements: nodes.map((el, i) => ({
                                index: i,
                                role: el.getAttribute('role') || el.tagName.toLowerCase(),
                                label: (el.getAttribute('aria-label') || el.innerText || '').trim().slice(0, 80)
                            }))
                        },
                        dart_vm: { connected: false, engine: 'cdp-fallback' }
                    });
                })()"#;
                let raw = self.evaluate_js(flt_js).await?;
                Ok(serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "raw": raw })))
            }
            Err(e) => Err(e),
        }
    }

    /// Retrieves full network request waterfall & navigation performance timings (`MiniBrowse.networkWaterfall`)
    pub async fn minibrowse_network_waterfall(&self) -> Result<serde_json::Value> {
        let res = self
            .send_command_with_timeout(
                "MiniBrowse.networkWaterfall",
                json!({}),
                Duration::from_secs(8),
            )
            .await;

        match res {
            Ok(val) => Ok(val),
            Err(e) if is_unsupported_cdp_error(&e) => {
                let perf_js = r#"(() => {
                    const navs = performance.getEntriesByType ? performance.getEntriesByType('navigation') : [];
                    const n = navs && navs[0] ? {
                        ttfb_ms: Math.round(Math.max(0, (navs[0].responseStart || 0) - (navs[0].requestStart || 0))),
                        dom_content_loaded_ms: Math.round(navs[0].domContentLoadedEventEnd || 0),
                        load_ms: Math.round(navs[0].loadEventEnd || 0)
                    } : null;
                    const res = performance.getEntriesByType ? Array.from(performance.getEntriesByType('resource'))
                        .sort((a, b) => (b.duration || 0) - (a.duration || 0))
                        .slice(0, 15)
                        .map(r => ({ name: r.name, initiator: r.initiatorType, duration_ms: Math.round(r.duration || 0) })) : [];
                    return JSON.stringify({ url: location.href, navigation_timing: n, slowest_resources: res });
                })()"#;
                let raw = self.evaluate_js(perf_js).await?;
                Ok(serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "raw": raw })))
            }
            Err(e) => Err(e),
        }
    }

    /// Inspects `localStorage`, `sessionStorage`, and cookie names (`MiniBrowse.storageInspect`, masking secrets)
    pub async fn minibrowse_storage_inspect(&self) -> Result<serde_json::Value> {
        let res = self
            .send_command_with_timeout(
                "MiniBrowse.storageInspect",
                json!({}),
                Duration::from_secs(8),
            )
            .await;

        match res {
            Ok(val) => Ok(val),
            Err(e) if is_unsupported_cdp_error(&e) => {
                let st_js = r#"(() => {
                    const mask = k => ['token','secret','password','apikey','api_key','auth','jwt'].some(w => k.toLowerCase().includes(w));
                    const dump = s => { const o = {}; try { for (let i = 0; i < s.length && i < 50; i++) { const k = s.key(i); o[k] = mask(k) ? '[MASKED]' : String(s.getItem(k) || '').slice(0, 160); } } catch (_) {} return o; };
                    return JSON.stringify({ url: location.href, local_storage: dump(localStorage), session_storage: dump(sessionStorage) });
                })()"#;
                let raw = self.evaluate_js(st_js).await?;
                Ok(serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "raw": raw })))
            }
            Err(e) => Err(e),
        }
    }

    /// Navigates backward (`delta = -1`) or forward (`delta = 1`) in session history,
    /// using `MiniBrowse.goBack`/`goForward` with `Page.getNavigationHistory` fallback.
    pub async fn navigate_history_step(&self, forward: bool) -> Result<bool> {
        let mb_method = if forward {
            "MiniBrowse.goForward"
        } else {
            "MiniBrowse.goBack"
        };
        match self
            .send_command_with_timeout(mb_method, json!({}), Duration::from_secs(8))
            .await
        {
            Ok(val) => {
                let moved = val
                    .get("navigated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let _ = self.wait_for_network_idle(Duration::from_millis(600)).await;
                Ok(moved)
            }
            Err(e) if is_unsupported_cdp_error(&e) => {
                let hist = self
                    .send_command("Page.getNavigationHistory", json!({}))
                    .await?;
                let cur_idx = hist
                    .get("currentIndex")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let entries = hist
                    .get("entries")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let next_idx = if forward { cur_idx + 1 } else { cur_idx - 1 };
                if next_idx < 0 || (next_idx as usize) >= entries.len() {
                    return Ok(false);
                }
                let entry_id = entries[next_idx as usize]
                    .get("id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(next_idx);
                self.send_command(
                    "Page.navigateToHistoryEntry",
                    json!({ "entryId": entry_id }),
                )
                .await?;
                let _ = self.wait_for_network_idle(Duration::from_millis(600)).await;
                Ok(true)
            }
            Err(e) => Err(e),
        }
    }

    /// Populates `<input type="file">` with local file paths via `DOM.setFileInputFiles`.
    pub async fn set_file_input_files(
        &self,
        selector: Option<&str>,
        files: &[String],
    ) -> Result<serde_json::Value> {
        let mut params = json!({ "files": files });
        if let Some(sel) = selector {
            params["selector"] = json!(sel);
        }
        self.send_command_with_timeout("DOM.setFileInputFiles", params, Duration::from_secs(10))
            .await
    }

    /// Configures automatic non-blocking `confirm()` / `prompt()` dialog policy.
    pub async fn configure_dialog(&self, accept: bool, prompt_text: Option<&str>) -> Result<()> {
        let mut params = json!({ "accept": accept });
        if let Some(txt) = prompt_text {
            params["promptText"] = json!(txt);
        }
        let _ = self
            .send_command_with_timeout(
                "Page.handleJavaScriptDialog",
                params,
                Duration::from_secs(3),
            )
            .await?;
        Ok(())
    }

    /// Captures a screenshot with optional perceptual `--if-changed` gating and Set-of-Mark `--annotate`
    /// overlays. Returns `Ok(None)` when `if_changed == true` and the viewport has not visually changed.
    pub async fn take_screenshot_extended(
        &self,
        if_changed: bool,
        annotate: bool,
    ) -> Result<Option<(Vec<u8>, Option<u32>)>> {
        if !if_changed && !annotate {
            let bytes = self.take_screenshot().await?;
            return Ok(Some((bytes, None)));
        }
        let mb_res = self
            .send_command_with_timeout(
                "MiniBrowse.captureScreenshot",
                json!({
                    "ifChanged": if_changed,
                    "annotate": annotate,
                    "thresholdBits": 1,
                }),
                Duration::from_secs(10),
            )
            .await;
        match mb_res {
            Ok(val) => {
                if val.get("unchanged").and_then(|v| v.as_bool()) == Some(true) {
                    return Ok(None);
                }
                let dist = val
                    .get("hamming_distance")
                    .and_then(|v| v.as_u64())
                    .map(|d| d as u32);
                let b64 = val.get("data").and_then(|d| d.as_str()).ok_or_else(|| {
                    ToolError::CommandExec("Missing PNG data in MiniBrowse screenshot".to_string())
                })?;
                let decoded = general_base64_decode(b64)?;
                Ok(Some((decoded, dist)))
            }
            Err(e) if is_unsupported_cdp_error(&e) => {
                if annotate {
                    let _ = super::PageAgent::inject_visual_badges(self).await;
                }
                let shot_res = self.take_screenshot().await;
                if annotate {
                    let _ = super::PageAgent::clear_visual_badges(self).await;
                }
                Ok(Some((shot_res?, None)))
            }
            Err(e) => Err(e),
        }
    }

    /// Inspects captured `fetch`/`XHR` API request & response bodies (`MiniBrowse.apiTraffic`, secrets redacted).
    pub async fn minibrowse_api_traffic(
        &self,
        filter: Option<&str>,
        only_errors: bool,
    ) -> Result<serde_json::Value> {
        let mut params = json!({ "onlyErrors": only_errors });
        if let Some(f) = filter.filter(|s| !s.trim().is_empty()) {
            params["filter"] = json!(f);
        }
        let res = self
            .send_command_with_timeout("MiniBrowse.apiTraffic", params, Duration::from_secs(8))
            .await;
        match res {
            Ok(val) => Ok(val),
            Err(e) if is_unsupported_cdp_error(&e) => {
                // Fallback to networkWaterfall on Chrome/Firefox
                self.minibrowse_network_waterfall().await
            }
            Err(e) => Err(e),
        }
    }

    /// Runs pixel-by-pixel visual regression diffing (`MiniBrowse.visualDiff`), comparing against a named
    /// baseline or comparing two live URLs (`url_a` vs `url_b`).
    pub async fn minibrowse_visual_diff(
        &self,
        baseline: Option<&str>,
        save_only: bool,
        url_a: Option<&str>,
        url_b: Option<&str>,
    ) -> Result<(serde_json::Value, Option<Vec<u8>>)> {
        let mut params = json!({
            "baseline": baseline.unwrap_or("default"),
            "saveOnly": save_only,
        });
        if let (Some(ua), Some(ub)) = (
            url_a.filter(|s| !s.trim().is_empty()),
            url_b.filter(|s| !s.trim().is_empty()),
        ) {
            params["urlA"] = json!(ua);
            params["urlB"] = json!(ub);
        }
        let mut val = self
            .send_command_with_timeout("MiniBrowse.visualDiff", params, Duration::from_secs(20))
            .await?;
        let diff_png = val
            .as_object_mut()
            .and_then(|obj| obj.remove("diff_png_base64"))
            .and_then(|v| v.as_str().and_then(|s| general_base64_decode(s).ok()));
        Ok((val, diff_png))
    }

    /// Clones a lightweight multiplexed `CdpClient` handle sharing the underlying WebSocket
    /// and atomic request-ID sequence, scoped to a specific target `sessionId` (or browser root `None`).
    pub fn fork_with_session(&self, session_id: Option<String>) -> Self {
        Self {
            tx: self.tx.clone(),
            next_id: Arc::clone(&self.next_id),
            pending: Arc::clone(&self.pending),
            page_ws_url: self.page_ws_url.clone(),
            session_id: Mutex::new(session_id),
            console_log: Arc::clone(&self.console_log),
            debug_collector: Arc::clone(&self.debug_collector),
            mock_routes: Arc::clone(&self.mock_routes),
            blocked_urls: Arc::clone(&self.blocked_urls),
            lifecycle_tx: self.lifecycle_tx.clone(),
            webdriver: self.webdriver.clone(),
        }
    }

    /// Spawns an `ego-lite`-style isolated background Space (`BrowserContext` + non-activated `Target` + `sessionId`)
    /// without stealing active tab focus or popping up extra GUI windows.
    pub async fn attach_isolated_space(
        &self,
        proxy_server: Option<&str>,
    ) -> Result<(Self, Option<String>, String)> {
        let root = self.fork_with_session(None);
        let mut ctx_params = json!({});
        if let Some(p) = proxy_server.filter(|s| !s.trim().is_empty()) {
            ctx_params["proxyServer"] = json!(p);
        }
        let context_id = root
            .send_command_with_timeout(
                "Target.createBrowserContext",
                ctx_params,
                Duration::from_secs(4),
            )
            .await
            .ok()
            .and_then(|v| {
                v.get("browserContextId")
                    .and_then(|s| s.as_str())
                    .map(str::to_string)
            });

        let mut create_params = json!({ "url": "about:blank", "background": true });
        if let Some(ref cid) = context_id {
            create_params["browserContextId"] = json!(cid);
        }
        let created = root
            .send_command_with_timeout("Target.createTarget", create_params, Duration::from_secs(6))
            .await?;
        let target_id = created
            .get("targetId")
            .and_then(|s| s.as_str())
            .ok_or_else(|| {
                ToolError::CommandExec("Target.createTarget returned no targetId".to_string())
            })?
            .to_string();

        let attached = root
            .send_command_with_timeout(
                "Target.attachToTarget",
                json!({ "targetId": target_id, "flatten": true }),
                Duration::from_secs(5),
            )
            .await?;
        let session_id = attached
            .get("sessionId")
            .and_then(|s| s.as_str())
            .ok_or_else(|| {
                ToolError::CommandExec("Target.attachToTarget returned no sessionId".to_string())
            })?
            .to_string();

        let space_client = self.fork_with_session(Some(session_id));
        let _ = space_client.enable_core_domains().await;
        let _ = space_client.evaluate_js(CONSOLE_SHIM_JS).await;
        Ok((space_client, context_id, target_id))
    }

    /// Disposes an isolated background Space's `BrowserContext` or closes its target tab cleanly.
    pub async fn dispose_isolated_space(&self, context_id: Option<&str>, target_id: &str) {
        let root = self.fork_with_session(None);
        if let Some(cid) = context_id {
            let _ = root
                .send_command_with_timeout(
                    "Target.disposeBrowserContext",
                    json!({ "browserContextId": cid }),
                    Duration::from_secs(3),
                )
                .await;
        } else {
            let _ = root
                .send_command_with_timeout(
                    "Target.closeTarget",
                    json!({ "targetId": target_id }),
                    Duration::from_secs(3),
                )
                .await;
        }
    }

    /// Promotes or demotes the live `MiniBrowse` GTK window between `headless` and `gui` in-place
    /// (`<5ms`, zero process restart, zero cookie/DOM/OAuth state loss).
    pub async fn set_window_mode(
        &self,
        gui: bool,
        steal_focus: bool,
        title: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut params = json!({
            "mode": if gui { "gui" } else { "headless" },
            "stealFocus": steal_focus,
        });
        if let Some(t) = title.filter(|s| !s.trim().is_empty()) {
            params["title"] = json!(t);
        }
        self.send_command_with_timeout("MiniBrowse.setWindowMode", params, Duration::from_secs(8))
            .await
    }

    /// Opens `url` or reuses an existing matching/`about:blank` tab inside `context_id` (`ego-lite` pattern).
    pub async fn open_or_reuse_tab(
        &self,
        url: &str,
        context_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        let root = self.fork_with_session(None);
        let mut params = json!({ "url": url });
        if let Some(cid) = context_id {
            params["browserContextId"] = json!(cid);
        }
        match root
            .send_command_with_timeout("MiniBrowse.openOrReuseTab", params, Duration::from_secs(15))
            .await
        {
            Ok(val) => Ok(val),
            Err(e) if is_unsupported_cdp_error(&e) => {
                let tid = self.create_tab(url, true).await?;
                Ok(json!({ "targetId": tid, "reused": false, "url": url }))
            }
            Err(e) => Err(e),
        }
    }

    /// Extracts structured JSON tables (`<table>` + ARIA grids) via `MiniBrowse.extractTables` (or JS fallback).
    pub async fn minibrowse_extract_tables(
        &self,
        selector: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut params = json!({});
        if let Some(sel) = selector.filter(|s| !s.trim().is_empty()) {
            params["selector"] = json!(sel);
        }
        match self
            .send_command_with_timeout("MiniBrowse.extractTables", params, Duration::from_secs(10))
            .await
        {
            Ok(val) => Ok(val),
            Err(e) if is_unsupported_cdp_error(&e) => {
                let sel_json = serde_json::to_string(&selector.unwrap_or("table"))
                    .unwrap_or_else(|_| "\"table\"".to_string());
                let raw = self
                    .evaluate_js(&format!(
                        "JSON.stringify((() => {{ const tables = Array.from(document.querySelectorAll({sel_json})).slice(0, 10).map((t, idx) => {{ const hs = Array.from(t.querySelectorAll('th')).map(h => h.innerText.trim()); const rows = Array.from(t.querySelectorAll('tr')).map(tr => Array.from(tr.querySelectorAll('td')).map(td => td.innerText.trim())).filter(r => r.length > 0); return {{ index: idx, headers: hs, row_count: rows.length, rows }}; }}); return {{ url: location.href, table_count: tables.length, tables }}; }})())"
                    ))
                    .await?;
                serde_json::from_str(&raw).map_err(|err| {
                    ToolError::CommandExec(format!("Failed parsing extracted tables JSON: {err}"))
                        .into()
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Re-attaches to a real open page target (skipping `chrome://omnibox-popup*` / `devtools://*` internals)
    /// or creates a fresh `about:blank` page if the previous target closed (`browser-harness` `ensure_real_tab`).
    pub async fn ensure_real_tab(&self) -> Result<()> {
        let root = self.fork_with_session(None);
        let targets_val = root
            .send_command_once("Target.getTargets", json!({}), Duration::from_secs(3))
            .await?;
        let real_tid = targets_val
            .get("targetInfos")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter().find_map(|t| {
                    let kind = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let url = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    if kind == "page" && !is_internal_browser_target_url(url) {
                        t.get("targetId")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                    } else {
                        None
                    }
                })
            });

        let tid = match real_tid {
            Some(id) => id,
            None => {
                let created = root
                    .send_command_once(
                        "Target.createTarget",
                        json!({ "url": "about:blank" }),
                        Duration::from_secs(4),
                    )
                    .await?;
                created
                    .get("targetId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::CommandExec("Failed creating recovery tab".to_string())
                    })?
                    .to_string()
            }
        };

        let attached = root
            .send_command_once(
                "Target.attachToTarget",
                json!({ "targetId": tid, "flatten": true }),
                Duration::from_secs(4),
            )
            .await?;
        if let Some(sid) = attached.get("sessionId").and_then(|v| v.as_str()) {
            *self.session_id.lock().await = Some(sid.to_string());
            let _ = self
                .send_command_once("Page.enable", json!({}), Duration::from_secs(2))
                .await;
            let _ = self
                .send_command_once("Runtime.enable", json!({}), Duration::from_secs(2))
                .await;
            let _ = self
                .send_command_once("Network.enable", json!({}), Duration::from_secs(2))
                .await;
            return Ok(());
        }
        Err(ToolError::CommandExec("ensure_real_tab returned no sessionId".to_string()).into())
    }

    /// Dispatches a compositor-level coordinate click (`mouseMoved` -> `mousePressed` -> `mouseReleased`)
    /// at `(x, y)` (`browser-harness` `click_at_xy`, piercing cross-origin iframes, canvas, and closed Shadow DOM).
    pub async fn click_at_xy(&self, x: f64, y: f64) -> Result<()> {
        let _ = self
            .send_command(
                "Input.dispatchMouseEvent",
                json!({ "type": "mouseMoved", "x": x, "y": y, "button": "none", "buttons": 0 }),
            )
            .await;
        self.send_command(
            "Input.dispatchMouseEvent",
            json!({ "type": "mousePressed", "x": x, "y": y, "button": "left", "buttons": 1, "clickCount": 1 }),
        )
        .await?;
        self.send_command(
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseReleased", "x": x, "y": y, "button": "left", "buttons": 0, "clickCount": 1 }),
        )
        .await?;
        Ok(())
    }

    /// Toggles MiniBrowse native content-blocker / tracker rules (`agweb` adblock toggle).
    pub async fn set_adblock(&self, enabled: bool) -> Result<bool> {
        let val = self
            .send_command_with_timeout(
                "MiniBrowse.setAdBlock",
                json!({ "enabled": enabled }),
                Duration::from_secs(4),
            )
            .await?;
        Ok(val
            .get("adblock_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(enabled))
    }

    /// Suspends idle background Space tabs (`MiniBrowse.suspendIdleTabs`).
    pub async fn suspend_idle_tabs(&self, idle_secs: u64) -> Result<serde_json::Value> {
        let root = self.fork_with_session(None);
        root.send_command_with_timeout(
            "MiniBrowse.suspendIdleTabs",
            json!({ "idleSecs": idle_secs }),
            Duration::from_secs(6),
        )
        .await
    }

    /// Configures MiniBrowse smart media diet (`enabled`: steer MSE away from software VP9/AV1 to H.264 <= 720p30;
    /// `audio_only`: hide `<video>` paint box while keeping `<audio>` playing uninterrupted).
    pub async fn set_media_diet(
        &self,
        enabled: Option<bool>,
        audio_only: Option<bool>,
    ) -> Result<serde_json::Value> {
        let mut params = json!({});
        if let Some(en) = enabled {
            params["enabled"] = json!(en);
        }
        if let Some(ao) = audio_only {
            params["audioOnly"] = json!(ao);
        }
        self.send_command_with_timeout("MiniBrowse.setMediaDiet", params, Duration::from_secs(6))
            .await
    }

    /// Queries full process-tree memory (`pss_mb`, `rss_mb`, `worker_rss_mb`, `renderer_rss_mb`, `network_rss_mb`, `tree_threads`)
    /// and optionally triggers a soft or hard memory trim (`MiniBrowse.trimMemory`).
    pub async fn status_and_trim_memory(
        &self,
        trim: bool,
        hard: bool,
    ) -> Result<serde_json::Value> {
        let root = self.fork_with_session(None);
        let trim_val = if trim {
            root.send_command_with_timeout(
                "MiniBrowse.trimMemory",
                json!({ "hard": hard }),
                Duration::from_secs(8),
            )
            .await
            .ok()
        } else {
            None
        };
        let mut status_val = root
            .send_command_with_timeout("MiniBrowse.status", json!({}), Duration::from_secs(4))
            .await?;
        if let (Some(obj), Some(tv)) = (status_val.as_object_mut(), trim_val) {
            obj.insert("trim_report".to_string(), tv);
        }
        Ok(status_val)
    }

    /// Injects raw HTML directly into the active frame (`Page.setDocumentContent` / `MiniBrowse.setContent`).
    pub async fn set_document_content(&self, html: &str) -> Result<()> {
        if self
            .send_command_with_timeout(
                "MiniBrowse.setContent",
                json!({ "html": html }),
                Duration::from_secs(10),
            )
            .await
            .is_ok()
        {
            return Ok(());
        }
        let frame_id = self
            .send_command_with_timeout("Page.getFrameTree", json!({}), Duration::from_secs(4))
            .await
            .ok()
            .and_then(|v| {
                v.get("frameTree")?
                    .get("frame")?
                    .get("id")?
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "main".to_string());
        let _ = self
            .send_command_with_timeout(
                "Page.setDocumentContent",
                json!({ "frameId": frame_id, "html": html }),
                Duration::from_secs(10),
            )
            .await?;
        Ok(())
    }

    /// Toggles WebGL / GPU hardware compositing in `MiniBrowse` (default is `false` = Zero-LLVM 2D Cairo diet).
    pub async fn set_webgl(&self, enabled: bool) -> Result<serde_json::Value> {
        let root = self.fork_with_session(None);
        root.send_command_with_timeout(
            "MiniBrowse.setWebGL",
            json!({ "enabled": enabled }),
            Duration::from_secs(6),
        )
        .await
    }

    /// Freezes (`freeze=true`) or thaws (`freeze=false`) an open `MiniBrowse` GUI window in-place,
    /// keeping the desktop window visible via a Cairo snapshot while freeing `WebKitWebProcess` RAM.
    pub async fn freeze_gui(&self, freeze: bool) -> Result<serde_json::Value> {
        let root = self.fork_with_session(None);
        root.send_command_with_timeout(
            "MiniBrowse.freezeGui",
            json!({ "freeze": freeze }),
            Duration::from_secs(6),
        )
        .await
    }
}

fn is_internal_browser_target_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("chrome://omnibox-popup")
        || lower.starts_with("devtools://")
        || lower.starts_with("chrome-extension://")
        || lower.starts_with("chrome://new-tab-page-third-party")
}

fn send_cdp_event_reply(
    tx: &tokio::sync::mpsc::UnboundedSender<Message>,
    id: u64,
    method: &str,
    params: serde_json::Value,
    session_id: Option<&str>,
) {
    let mut cmd = json!({
        "id": id,
        "method": method,
        "params": params,
    });
    if let Some(sid) = session_id {
        cmd["sessionId"] = json!(sid);
    }
    let _ = tx.send(Message::Text(cmd.to_string()));
}

fn is_unsupported_cdp_error(e: &crate::error::MinicodeError) -> bool {
    let err_str = e.to_string();
    err_str.contains("Unknown")
        || err_str.contains("not found")
        || err_str.contains("not supported")
}

/// Wraps arbitrary JavaScript expressions/statements so Chrome CDP Runtime.evaluate
/// can evaluate multi-statement scripts, assignments, and returns without syntax errors.
pub fn prepare_cdp_script(script: &str) -> String {
    let trimmed = script.trim();
    let mut trimmed_no_semi = trimmed;
    while trimmed_no_semi.ends_with(';') {
        trimmed_no_semi = trimmed_no_semi[..trimmed_no_semi.len() - 1].trim_end();
    }

    // Unwrap `JSON.stringify(<async_expr>)` so the inner Promise is awaited before stringifying
    // (otherwise `JSON.stringify(Promise)` synchronously returns `"{}"`).
    if let Some(inner_async) = extract_json_stringify_async_arg(trimmed_no_semi) {
        let prepared_inner = prepare_cdp_script(inner_async);
        return format!("(async () => JSON.stringify(await ({})))()", prepared_inner);
    }

    if trimmed_no_semi.starts_with("(()")
        || trimmed_no_semi.starts_with("(function")
        || trimmed_no_semi.starts_with("(async")
    {
        if trimmed_no_semi.ends_with("()") {
            return script.to_string();
        } else if trimmed_no_semi.ends_with(')') && is_single_balanced_group(trimmed_no_semi) {
            return format!("{}()", trimmed_no_semi);
        }
        return script.to_string();
    }

    // Auto-invoke Playwright/Puppeteer-style uninvoked arrow or function expressions:
    // e.g. `() => { ... }`, `async () => { ... }`, `function() { ... }`, `async function() { ... }`
    // Also repair unparenthesized `() => { ... }()` where the caller appended `()` without outer `(...)`.
    if is_uninvoked_function_expression(trimmed_no_semi) {
        return format!("({})()", trimmed_no_semi);
    }
    if let Some(stripped) = trimmed_no_semi.strip_suffix("()") {
        let without_call = stripped.trim_end();
        if is_uninvoked_function_expression(without_call) {
            return format!("({})()", without_call);
        }
    }

    let has_await = trimmed.contains("await ") || trimmed.starts_with("await ");
    let top_level_return = has_top_level_return(trimmed);
    // If it's a simple one-liner with no semicolons, no await, and no return:
    if !trimmed.contains(';') && !trimmed.contains('\n') && !top_level_return && !has_await {
        return script.to_string();
    }
    // If it explicitly uses top-level `return` or contains `await`:
    if has_await {
        if top_level_return {
            let body =
                inject_return_on_final_expression(trimmed).unwrap_or_else(|| script.to_string());
            format!("(async () => {{\n{}\n}})()", body)
        } else if let Some(rewritten) = inject_return_on_final_expression(trimmed) {
            format!("(async () => {{\n{}\n}})()", rewritten)
        } else {
            format!(
                "(async () => {{\n  try {{\n    return await eval({});\n  }} catch (_) {{\n    {}\n  }}\n}})()",
                serde_json::to_string(script).unwrap_or_else(|_| format!("{:?}", script)),
                script
            )
        }
    } else if top_level_return {
        let body = inject_return_on_final_expression(trimmed).unwrap_or_else(|| script.to_string());
        format!("(() => {{\n{}\n}})()", body)
    } else {
        // Multi-statement script without top-level return: wrap so the completion value is returned
        let fallback =
            inject_return_on_final_expression(trimmed).unwrap_or_else(|| script.to_string());
        format!(
            "(() => {{\n  try {{\n    return eval({});\n  }} catch (_) {{\n    {}\n  }}\n}})()",
            serde_json::to_string(script).unwrap_or_else(|_| format!("{:?}", script)),
            fallback
        )
    }
}

/// Checks if `expr` is `JSON.stringify(<inner>)` where `<inner>` contains `async` or `await`
/// and the outer `JSON.stringify(...)` call spans the entire expression.
fn extract_json_stringify_async_arg(expr: &str) -> Option<&str> {
    const PREFIX: &str = "JSON.stringify(";
    if !expr.starts_with(PREFIX) || !expr.ends_with(')') {
        return None;
    }
    let open_paren_idx = PREFIX.len() - 1;
    if !is_single_balanced_group(&expr[open_paren_idx..]) {
        return None;
    }
    let inner = expr[PREFIX.len()..expr.len() - 1].trim();
    // Only unwrap when there is a single argument to JSON.stringify (no top-level comma)
    // and the inner expression uses async/await.
    if has_top_level_delimiter(inner, b',') {
        return None;
    }
    let uses_async = inner.contains("async ")
        || inner.contains("async(")
        || inner.contains("await ")
        || inner.starts_with("await ");
    if uses_async {
        Some(inner)
    } else {
        None
    }
}

/// Returns true if `s` starts with `(` and its matching closing `)` is the very last byte of `s`.
fn is_single_balanced_group(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
        return false;
    }
    let len = bytes.len();
    let mut i = 0;
    let mut depth = 0usize;
    while i < len {
        let b = bytes[i];
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(len);
            continue;
        }
        if b == b'\'' || b == b'"' || b == b'`' {
            let quote = b;
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return i == len - 1;
            }
        }
        i += 1;
    }
    false
}

/// Returns true if `s` contains `delim` (e.g. `,` or `;`) at top level (outside `()`, `[]`, `{}`, strings, comments).
fn has_top_level_delimiter(s: &str, delim: u8) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    while i < len {
        let b = bytes[i];
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(len);
            continue;
        }
        if b == b'\'' || b == b'"' || b == b'`' {
            let quote = b;
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        match b {
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'{' => brace += 1,
            b'}' => brace = brace.saturating_sub(1),
            d if d == delim && paren == 0 && bracket == 0 && brace == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Detects Playwright/Puppeteer-style uninvoked arrow or function expressions passed to `browser_eval`,
/// such as `() => { ... }`, `async () => { ... }`, `function() { ... }`, or `async function() { ... }`.
fn is_uninvoked_function_expression(s: &str) -> bool {
    if s.ends_with("()") || has_top_level_delimiter(s, b';') {
        return false;
    }
    let rest = s.strip_prefix("async").map(str::trim_start).unwrap_or(s);
    if let Some(after_fn) = rest.strip_prefix("function") {
        let after_fn = after_fn.trim_start();
        // Anonymous `function(...) { ... }` or named `function foo(...) { ... }` ending in `}`
        if (after_fn.starts_with('(')
            || after_fn
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_' || b == b'$'))
            && rest.ends_with('}')
        {
            return true;
        }
    }
    if rest.starts_with('(') {
        if let Some(close_idx) = rest.find(')') {
            let params = &rest[1..close_idx];
            let valid_params = params.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || b.is_ascii_whitespace()
                    || matches!(
                        b,
                        b'_' | b'$' | b',' | b'=' | b'.' | b'{' | b'}' | b'[' | b']'
                    )
            });
            let after_params = rest[close_idx + 1..].trim_start();
            if valid_params && after_params.starts_with("=>") {
                return true;
            }
        }
    }
    false
}

/// Checks whether `script` contains a `return` statement at the outer script scope
/// (ignoring `return` statements inside nested callbacks such as `.forEach(el => { return; })`
/// or nested `function(...) { return ... }` declarations, as well as string/comment literals).
fn has_top_level_return(script: &str) -> bool {
    let bytes = script.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_stack: Vec<bool> = Vec::new(); // true = nested function/callback brace
    let mut pending_fn_brace = false;

    while i < len {
        let b = bytes[i];
        // Line comment
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(len);
            continue;
        }
        // String literals
        if b == b'\'' || b == b'"' || b == b'`' {
            let quote = b;
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        if b == b'=' && i + 1 < len && bytes[i + 1] == b'>' {
            pending_fn_brace = true;
            i += 2;
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            let start = i;
            while i < len
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            let word = &script[start..i];
            let prev_is_dot = start > 0 && bytes[start - 1] == b'.';
            if !prev_is_dot {
                if word == "function" {
                    pending_fn_brace = true;
                } else if word == "return" {
                    let next_ok = i == len
                        || bytes[i].is_ascii_whitespace()
                        || bytes[i] == b';'
                        || bytes[i] == b'('
                        || bytes[i] == b'{'
                        || bytes[i] == b'['
                        || bytes[i] == b'\''
                        || bytes[i] == b'"'
                        || bytes[i] == b'`';
                    let in_nested_fn =
                        paren_depth > 0 || bracket_depth > 0 || brace_stack.iter().any(|&f| f);
                    if next_ok && !in_nested_fn {
                        return true;
                    }
                }
            }
            continue;
        }

        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = bracket_depth.saturating_sub(1),
            b'{' => {
                let is_fn = pending_fn_brace || paren_depth > 0;
                brace_stack.push(is_fn);
                pending_fn_brace = false;
            }
            b'}' => {
                brace_stack.pop();
                pending_fn_brace = false;
            }
            b';' => {
                pending_fn_brace = false;
            }
            _ => {}
        }
        i += 1;
    }

    false
}

/// If the final top-level statement in `script` is an expression (not already starting with `return`
/// or a declaration/control-flow keyword), prefixes that final statement with `return ` so an IIFE
/// wrapper returns its completion value.
fn inject_return_on_final_expression(script: &str) -> Option<String> {
    let mut trimmed = script.trim();
    while trimmed.ends_with(';') {
        trimmed = trimmed[..trimmed.len() - 1].trim_end();
    }
    if trimmed.is_empty() {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut last_stmt_start = 0usize;

    while i < len {
        let b = bytes[i];
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(len);
            continue;
        }
        if b == b'\'' || b == b'"' || b == b'`' {
            let quote = b;
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = bracket_depth.saturating_sub(1),
            b'{' => brace_depth += 1,
            b'}' => brace_depth = brace_depth.saturating_sub(1),
            b';' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                let mut next = i + 1;
                while next < len && bytes[next].is_ascii_whitespace() {
                    next += 1;
                }
                if next < len {
                    last_stmt_start = next;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let last_stmt = trimmed[last_stmt_start..].trim();
    if last_stmt.is_empty() || last_stmt.starts_with('{') {
        return None;
    }

    let first_word_end = last_stmt
        .bytes()
        .position(|c| !(c.is_ascii_alphanumeric() || c == b'_' || c == b'$'))
        .unwrap_or(last_stmt.len());
    let first_word = &last_stmt[..first_word_end];

    const NON_EXPR_KEYWORDS: &[&str] = &[
        "return", "const", "let", "var", "if", "for", "while", "do", "switch", "try", "throw",
        "function", "class", "break", "continue", "debugger", "import", "export",
    ];
    if NON_EXPR_KEYWORDS.contains(&first_word) || last_stmt.starts_with("async function") {
        return None;
    }

    let prefix = &trimmed[..last_stmt_start];
    Some(format!("{}return {};", prefix, last_stmt))
}

/// Minimal zero-intermediate-allocation base64 decoder without adding heavy extra dependencies
fn general_base64_decode(input: &str) -> Result<Vec<u8>> {
    let decode_byte = |b: u8| -> u8 {
        match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => 0,
        }
    };

    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut iter = input
        .as_bytes()
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace());

    while let (Some(c0), Some(c1), Some(c2), Some(c3)) =
        (iter.next(), iter.next(), iter.next(), iter.next())
    {
        let b0 = decode_byte(c0);
        let b1 = decode_byte(c1);
        let b2 = decode_byte(c2);
        let b3 = decode_byte(c3);

        out.push((b0 << 2) | (b1 >> 4));
        if c2 != b'=' {
            out.push((b1 << 4) | (b2 >> 2));
        }
        if c3 != b'=' {
            out.push((b2 << 6) | b3);
        }
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
    crate::utils::strings::wildcard_match(text, pattern)
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
        assert!(prep2.contains("return 'Done';"));

        // Nested callback with `return;` followed by final expression (minitest19 #078/#079)
        let nested_for_each = "const bad=[]; document.querySelectorAll('p').forEach(el=>{if(el.closest('.cmp-scroll')) return; bad.push(el.tagName);}); JSON.stringify({count:bad.length})";
        assert!(!has_top_level_return(nested_for_each));
        let prep_nested = prepare_cdp_script(nested_for_each);
        assert!(prep_nested.contains("return eval("));
        assert!(prep_nested.contains("return JSON.stringify({count:bad.length});"));

        // Top-level guard return followed by final expression without return
        let guard_plus_expr =
            "const el = document.querySelector('#hero'); if (!el) return 'missing'; el.tagName";
        assert!(has_top_level_return(guard_plus_expr));
        let prep_guard = prepare_cdp_script(guard_plus_expr);
        assert!(prep_guard.contains("return el.tagName;"));

        // Playwright-style uninvoked arrow function (minitest22 #55)
        let pw_arrow = "() => {\n  const h1 = document.querySelector('h1');\n  return { h1Text: h1 ? h1.innerText : null };\n}";
        let prep_pw = prepare_cdp_script(pw_arrow);
        assert!(prep_pw.starts_with("(() => {"));
        assert!(prep_pw.ends_with("})()"));

        // Uninvoked async arrow function
        let pw_async = "async () => { await new Promise(r => setTimeout(r, 10)); return 'ok'; }";
        let prep_pw_async = prepare_cdp_script(pw_async);
        assert_eq!(prep_pw_async, format!("({})()", pw_async));

        // Unparenthesized arrow function with trailing `()` (minitest49 #3323)
        let unparen_iife =
            "() => { const el = document.getElementById('swarm'); return { top: 96 }; }()";
        let prep_unparen = prepare_cdp_script(unparen_iife);
        assert_eq!(
            prep_unparen,
            "(() => { const el = document.getElementById('swarm'); return { top: 96 }; })()"
        );

        // JSON.stringify wrapping an async IIFE (minitest22 #148)
        let stringify_async = "JSON.stringify((async () => { const errs = []; await new Promise(r => setTimeout(r, 400)); return errs; })())";
        let prep_stringify_async = prepare_cdp_script(stringify_async);
        assert_eq!(
            prep_stringify_async,
            "(async () => JSON.stringify(await ((async () => { const errs = []; await new Promise(r => setTimeout(r, 400)); return errs; })())))()"
        );
    }
}
