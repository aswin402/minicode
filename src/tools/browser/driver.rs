use crate::constants::BROWSER_NAVIGATE_TIMEOUT_MS;
use crate::error::{Result, ToolError};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;

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

        // Writer task
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
                                // Auto-handle modal alerts and dialogs
                                if method == "Page.javascriptDialogOpening" {
                                    tracing::info!("Auto-dismissing browser JavaScript dialog");
                                    let dismiss_cmd = json!({
                                        "id": 999_999,
                                        "method": "Page.handleJavaScriptDialog",
                                        "params": { "accept": true }
                                    });
                                    let _ = tx_clone.send(Message::Text(dismiss_cmd.to_string()));
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
        };

        if wants_session {
            client.attach_fresh_page().await?;
        }

        // Enable core domains
        client.enable_core_domains().await?;

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
        let res = tokio::time::timeout(timeout_dur, resp_rx)
            .await
            .map_err(|_| {
                ToolError::CommandExec(format!("Timeout waiting for CDP method '{}'", method))
            })?
            .map_err(|_| {
                ToolError::CommandExec(format!("CDP connection dropped during method '{}'", method))
            })?;

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
    pub async fn enable_core_domains(&self) -> Result<()> {
        let _ = self.send_command("Page.enable", json!({})).await;
        let _ = self.send_command("Runtime.enable", json!({})).await;
        let _ = self.send_command("DOM.enable", json!({})).await;
        let _ = self.send_command("Network.enable", json!({})).await;
        let _ = self.send_command("Log.enable", json!({})).await;
        Ok(())
    }

    /// Navigates to the specified URL and waits for basic DOM settling
    pub async fn navigate(&self, url: &str) -> Result<()> {
        self.send_command("Page.navigate", json!({ "url": url }))
            .await?;
        // Allow time for client-side JavaScript / SPA hydration to settle
        tokio::time::sleep(Duration::from_millis(600)).await;
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
        let res = self
            .send_command(
                "Runtime.evaluate",
                json!({
                    "expression": script,
                    "returnByValue": true
                }),
            )
            .await?;

        let val = res.get("result").and_then(|r| r.get("value"));
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

    #[allow(dead_code)]
    pub fn page_ws_url(&self) -> &str {
        &self.page_ws_url
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_decode() {
        // "Hello" in base64 is "SGVsbG8="
        let decoded = general_base64_decode("SGVsbG8=").unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello");

        // "Minicode" in base64 is "TWluaWNvZGU="
        let decoded2 = general_base64_decode("TWluaWNvZGU=").unwrap();
        assert_eq!(String::from_utf8(decoded2).unwrap(), "Minicode");
    }
}
