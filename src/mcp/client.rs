use crate::agent::provider::ToolSchema;
use crate::config::{McpConfig, McpServerConfig, McpTransport};
use crate::constants::{DEFAULT_MCP_TIMEOUT_SECS, JSONRPC_VERSION, MCP_METHOD_TOOLS_CALL};
use crate::error::{McpError, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

/// Information about a discovered MCP tool from an external server
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct McpToolInfo {
    pub server_name: String,
    pub original_name: String,
    pub namespaced_name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Client manager handling connections and tool dispatching for multiple MCP servers
#[derive(Clone)]
pub struct McpClientManager {
    servers: Arc<RwLock<HashMap<String, McpServerConfig>>>,
    tools: Arc<RwLock<Vec<McpToolInfo>>>,
    http_client: reqwest::Client,
    request_id: Arc<AtomicU64>,
    initialized: Arc<AtomicBool>,
}

impl Default for McpClientManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpClientManager {
    pub fn new() -> Self {
        let http_client = match reqwest::Client::builder()
            .tcp_keepalive(Duration::from_secs(60))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "Failed to build custom reqwest HTTP client, using default");
                reqwest::Client::new()
            }
        };

        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            tools: Arc::new(RwLock::new(Vec::new())),
            http_client,
            request_id: Arc::new(AtomicU64::new(1)),
            initialized: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns whether the client manager has already been initialized from config
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }

    /// Initializes and registers all enabled servers in configuration (idempotent)
    pub async fn init_from_config(&self, config: &McpConfig) -> Result<()> {
        if self.initialized.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let mut servers_guard = self.servers.write().await;
        for (name, server_cfg) in &config.servers {
            if server_cfg.enabled {
                if let Err(e) = server_cfg.validate(name) {
                    tracing::warn!(server = %name, error = %e, "Invalid MCP server config; skipping");
                    continue;
                }
                servers_guard.insert(name.clone(), server_cfg.clone());
                tracing::info!(server = %name, "Registered MCP server configuration");

                // Discover tools from this server
                match self.discover_server_tools(name, server_cfg).await {
                    Ok(discovered) => {
                        let count = discovered.len();
                        let mut tools_guard = self.tools.write().await;
                        tools_guard.extend(discovered);
                        tracing::info!(server = %name, tools_count = count, "Discovered MCP tools");
                    }
                    Err(e) => {
                        tracing::warn!(server = %name, error = %e, "Failed to discover tools from MCP server");
                    }
                }
            }
        }
        Ok(())
    }

    /// Discovers tool schemas from an external MCP server
    pub async fn discover_server_tools(
        &self,
        server_name: &str,
        server_cfg: &McpServerConfig,
    ) -> Result<Vec<McpToolInfo>> {
        let req_id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let timeout_secs = server_cfg.timeout_secs.unwrap_or(DEFAULT_MCP_TIMEOUT_SECS);

        match server_cfg.transport {
            McpTransport::Stdio => {
                let Some(cmd) = &server_cfg.command else {
                    return Err(McpError::ConnectionFailed {
                        server: server_name.into(),
                        reason: "Missing 'command' in Stdio server configuration".into(),
                    }
                    .into());
                };

                let mut child_cmd = tokio::process::Command::new("sh");
                child_cmd
                    .arg("-c")
                    .arg(cmd)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(true);

                #[cfg(unix)]
                {
                    child_cmd.process_group(0);
                }

                let mut child = child_cmd.spawn().map_err(|e| McpError::ConnectionFailed {
                    server: server_name.into(),
                    reason: format!("Failed to spawn process '{}': {}", cmd, e),
                })?;

                let init_req = serde_json::json!({
                    "jsonrpc": JSONRPC_VERSION,
                    "id": req_id,
                    "method": crate::constants::MCP_METHOD_INITIALIZE,
                    "params": {
                        "protocolVersion": crate::constants::MCP_PROTOCOL_VERSION,
                        "capabilities": { "tools": {} },
                        "clientInfo": {
                            "name": env!("CARGO_PKG_NAME"),
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                });

                let initialized_notif = serde_json::json!({
                    "jsonrpc": JSONRPC_VERSION,
                    "method": crate::constants::MCP_METHOD_INITIALIZED
                });

                let list_id = self.request_id.fetch_add(1, Ordering::SeqCst);
                let list_req = serde_json::json!({
                    "jsonrpc": JSONRPC_VERSION,
                    "id": list_id,
                    "method": crate::constants::MCP_METHOD_TOOLS_LIST
                });

                let payload = format!(
                    "{}\n{}\n{}\n",
                    serde_json::to_string(&init_req)?,
                    serde_json::to_string(&initialized_notif)?,
                    serde_json::to_string(&list_req)?
                );

                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(payload.as_bytes()).await;
                    let _ = stdin.flush().await;
                }

                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| McpError::Transport("Failed to capture child stdout".into()))?;

                use tokio::io::AsyncBufReadExt;
                let mut reader = tokio::io::BufReader::new(stdout).lines();
                let mut tools = Vec::new();

                let read_fut = async {
                    while let Ok(Some(line)) = reader.next_line().await {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(trimmed) {
                            if resp.get("id").and_then(|id| id.as_u64()) == Some(list_id) {
                                if let Some(tools_arr) = resp
                                    .get("result")
                                    .and_then(|r| r.get("tools"))
                                    .and_then(|t| t.as_array())
                                {
                                    for t in tools_arr {
                                        if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                                            let desc = t
                                                .get("description")
                                                .and_then(|d| d.as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            let params =
                                                t.get("inputSchema").cloned().unwrap_or_else(
                                                    || serde_json::json!({"type": "object"}),
                                                );
                                            tools.push(McpToolInfo {
                                                server_name: server_name.to_string(),
                                                original_name: name.to_string(),
                                                namespaced_name: format!(
                                                    "{}{}__{}",
                                                    crate::constants::MCP_TOOL_PREFIX,
                                                    server_name,
                                                    name
                                                ),
                                                description: desc,
                                                parameters: params,
                                            });
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                    Ok::<Vec<McpToolInfo>, crate::error::MinicodeError>(tools)
                };

                match tokio::time::timeout(Duration::from_secs(timeout_secs), read_fut).await {
                    Ok(Ok(t)) => Ok(t),
                    Ok(Err(e)) => Err(e),
                    Err(_) => {
                        #[cfg(unix)]
                        if let Some(pid) = child.id() {
                            let _ = std::process::Command::new("kill")
                                .arg("-9")
                                .arg(format!("-{}", pid))
                                .output();
                        }
                        let _ = child.kill().await;
                        tracing::warn!(server = %server_name, "Timeout discovering tools from stdio server");
                        Ok(Vec::new())
                    }
                }
            }
            McpTransport::Http | McpTransport::Sse => {
                let Some(url) = &server_cfg.url else {
                    return Ok(Vec::new());
                };
                let list_req = serde_json::json!({
                    "jsonrpc": JSONRPC_VERSION,
                    "id": req_id,
                    "method": crate::constants::MCP_METHOD_TOOLS_LIST
                });
                let req = self.http_client.post(url).json(&list_req);
                let resp = match req.send().await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(server = %server_name, error = %e, "Failed to connect to HTTP MCP server for discovery");
                        return Ok(Vec::new());
                    }
                };
                let body: serde_json::Value = match resp.json().await {
                    Ok(b) => b,
                    Err(_) => return Ok(Vec::new()),
                };
                let mut tools = Vec::new();
                if let Some(tools_arr) = body
                    .get("result")
                    .and_then(|r| r.get("tools"))
                    .and_then(|t| t.as_array())
                {
                    for t in tools_arr {
                        if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                            let desc = t
                                .get("description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("")
                                .to_string();
                            let params = t
                                .get("inputSchema")
                                .cloned()
                                .unwrap_or_else(|| serde_json::json!({"type": "object"}));
                            tools.push(McpToolInfo {
                                server_name: server_name.to_string(),
                                original_name: name.to_string(),
                                namespaced_name: format!(
                                    "{}{}__{}",
                                    crate::constants::MCP_TOOL_PREFIX,
                                    server_name,
                                    name
                                ),
                                description: desc,
                                parameters: params,
                            });
                        }
                    }
                }
                Ok(tools)
            }
        }
    }

    /// Returns the number of registered active MCP servers
    #[allow(dead_code)]
    pub async fn active_servers_count(&self) -> usize {
        self.servers.read().await.len()
    }

    /// Returns list of discovered tool schemas formatted as ToolSchema for LLM injection
    pub async fn get_tool_schemas(&self) -> Vec<ToolSchema> {
        let tools_guard = self.tools.read().await;
        tools_guard
            .iter()
            .map(|t| ToolSchema {
                name: t.namespaced_name.clone(),
                description: format!("[MCP: {}] {}", t.server_name, t.description),
                parameters: t.parameters.clone(),
            })
            .collect()
    }

    /// Helper to parse standard JSON-RPC 2.0 response and extract text output
    fn parse_jsonrpc_response(
        server_name: &str,
        tool_name: &str,
        body: &str,
        req_id: Option<u64>,
    ) -> Result<String> {
        // First try to parse entire body as a single JSON object (for formatted/pretty JSON)
        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(body.trim()) {
            if let Some(target_id) = req_id {
                if resp.get("id").and_then(|id| id.as_u64()) == Some(target_id) {
                    return Self::extract_tool_output(server_name, tool_name, &resp);
                }
            } else {
                return Self::extract_tool_output(server_name, tool_name, &resp);
            }
        }

        // Handle newline-delimited JSON-RPC messages (e.g. initialize response + tools/call response)
        for line in body.lines().rev() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(resp) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(target_id) = req_id {
                    if resp.get("id").and_then(|id| id.as_u64()) != Some(target_id) {
                        continue;
                    }
                }
                return Self::extract_tool_output(server_name, tool_name, &resp);
            }
        }

        Ok(body.to_string())
    }

    fn extract_tool_output(
        server_name: &str,
        tool_name: &str,
        resp: &serde_json::Value,
    ) -> Result<String> {
        if let Some(error) = resp.get("error") {
            let error_msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown JSON-RPC error");
            return Err(McpError::ToolCallFailed {
                server: server_name.to_string(),
                tool: tool_name.to_string(),
                reason: error_msg.to_string(),
            }
            .into());
        }

        if let Some(result) = resp.get("result") {
            if let Some(is_error) = result.get("isError").and_then(|e| e.as_bool()) {
                if is_error {
                    let text = result
                        .get("content")
                        .and_then(|c| c.as_array())
                        .and_then(|a| a.first())
                        .and_then(|item| item.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("Tool execution failed on MCP server");
                    return Err(McpError::ToolCallFailed {
                        server: server_name.to_string(),
                        tool: tool_name.to_string(),
                        reason: text.to_string(),
                    }
                    .into());
                }
            }

            if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
                let mut texts = Vec::new();
                for item in content {
                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                        texts.push(text);
                    }
                }
                if !texts.is_empty() {
                    return Ok(texts.join("\n"));
                }
            }

            return Ok(result.to_string());
        }

        Ok(resp.to_string())
    }

    /// Dispatches a tool call if it is namespaced as `mcp__<server>__<tool>`
    pub async fn call_tool(
        &self,
        namespaced_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String> {
        let parts: Vec<&str> = namespaced_name.splitn(3, "__").collect();
        if parts.len() < 3 || parts[0] != "mcp" {
            return Err(McpError::Protocol(format!(
                "Invalid MCP namespacing format '{}', expected 'mcp__<server>__<tool>'",
                namespaced_name
            ))
            .into());
        }

        let server_name = parts[1];
        let original_tool = parts[2];

        let servers = self.servers.read().await;
        let server_cfg = servers
            .get(server_name)
            .ok_or_else(|| McpError::ServerNotFound(server_name.to_string()))?;

        let timeout_secs = server_cfg.timeout_secs.unwrap_or(DEFAULT_MCP_TIMEOUT_SECS);
        let req_id = self.request_id.fetch_add(1, Ordering::Relaxed);

        match server_cfg.transport {
            McpTransport::Stdio => {
                let Some(cmd) = &server_cfg.command else {
                    return Err(McpError::ConnectionFailed {
                        server: server_name.into(),
                        reason: "Missing 'command' in stdio server configuration".into(),
                    }
                    .into());
                };

                let mut child_cmd = tokio::process::Command::new("sh");
                child_cmd
                    .arg("-c")
                    .arg(cmd)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(true);

                if let Some(args) = &server_cfg.args {
                    child_cmd.args(args);
                }
                if let Some(env_vars) = &server_cfg.env {
                    for (k, v) in env_vars {
                        child_cmd.env(k, v);
                    }
                }

                #[cfg(unix)]
                {
                    child_cmd.process_group(0);
                }

                let mut child = child_cmd.spawn().map_err(|e| McpError::ConnectionFailed {
                    server: server_name.into(),
                    reason: format!("Failed to spawn process '{}': {}", cmd, e),
                })?;

                let init_req = serde_json::json!({
                    "jsonrpc": JSONRPC_VERSION,
                    "id": 1,
                    "method": crate::constants::MCP_METHOD_INITIALIZE,
                    "params": {
                        "protocolVersion": crate::constants::MCP_PROTOCOL_VERSION,
                        "capabilities": { "tools": {} },
                        "clientInfo": {
                            "name": env!("CARGO_PKG_NAME"),
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                });

                let initialized_notif = serde_json::json!({
                    "jsonrpc": JSONRPC_VERSION,
                    "method": crate::constants::MCP_METHOD_INITIALIZED
                });

                let tool_req = serde_json::json!({
                    "jsonrpc": JSONRPC_VERSION,
                    "id": req_id,
                    "method": MCP_METHOD_TOOLS_CALL,
                    "params": {
                        "name": original_tool,
                        "arguments": arguments,
                        "_meta": {
                            "io.modelcontextprotocol/clientInfo": {
                                "name": env!("CARGO_PKG_NAME"),
                                "version": env!("CARGO_PKG_VERSION")
                            }
                        }
                    }
                });

                let payload = format!(
                    "{}\n{}\n{}\n",
                    serde_json::to_string(&init_req)?,
                    serde_json::to_string(&initialized_notif)?,
                    serde_json::to_string(&tool_req)?
                );

                if let Some(mut stdin) = child.stdin.take() {
                    let write_res = async {
                        stdin.write_all(payload.as_bytes()).await?;
                        stdin.flush().await?;
                        Ok::<(), std::io::Error>(())
                    }
                    .await;

                    if let Err(e) = write_res {
                        return Err(McpError::Transport(e.to_string()).into());
                    }
                }

                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| McpError::Transport("Failed to capture child stdout".into()))?;

                use tokio::io::AsyncBufReadExt;
                let mut reader = tokio::io::BufReader::new(stdout).lines();

                let read_fut = async {
                    while let Ok(Some(line)) = reader.next_line().await {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(trimmed) {
                            if resp.get("id").and_then(|id| id.as_u64()) == Some(req_id) {
                                return Self::parse_jsonrpc_response(
                                    server_name,
                                    original_tool,
                                    trimmed,
                                    Some(req_id),
                                );
                            }
                        }
                    }

                    // Fallback to reading stderr if stdout stream closed without result
                    let mut stderr_str = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        use tokio::io::AsyncReadExt;
                        let mut buf = Vec::new();
                        let _ = stderr.read_to_end(&mut buf).await;
                        stderr_str = String::from_utf8_lossy(&buf).to_string();
                    }

                    if !stderr_str.trim().is_empty() {
                        Err(McpError::ToolCallFailed {
                            server: server_name.into(),
                            tool: original_tool.into(),
                            reason: stderr_str,
                        }
                        .into())
                    } else {
                        Ok("✔ MCP tool executed (no output)".to_string())
                    }
                };

                match tokio::time::timeout(Duration::from_secs(timeout_secs), read_fut).await {
                    Ok(res) => res,
                    Err(_) => {
                        #[cfg(unix)]
                        if let Some(pid) = child.id() {
                            let _ = std::process::Command::new("kill")
                                .arg("-9")
                                .arg(format!("-{}", pid))
                                .output();
                        }
                        let _ = child.kill().await;
                        Err(McpError::Timeout {
                            server: server_name.to_string(),
                            timeout_secs,
                        }
                        .into())
                    }
                }
            }
            McpTransport::Sse | McpTransport::Http => {
                let Some(url) = &server_cfg.url else {
                    return Err(McpError::ConnectionFailed {
                        server: server_name.into(),
                        reason: "Missing 'url' in HTTP/SSE server configuration".into(),
                    }
                    .into());
                };

                let request = serde_json::json!({
                    "jsonrpc": JSONRPC_VERSION,
                    "id": req_id,
                    "method": MCP_METHOD_TOOLS_CALL,
                    "params": {
                        "name": original_tool,
                        "arguments": arguments,
                        "_meta": {
                            "io.modelcontextprotocol/clientInfo": {
                                "name": env!("CARGO_PKG_NAME"),
                                "version": env!("CARGO_PKG_VERSION")
                            }
                        }
                    }
                });

                let send_fut = self
                    .http_client
                    .post(url)
                    .timeout(Duration::from_secs(timeout_secs))
                    .json(&request)
                    .send();

                let resp =
                    match tokio::time::timeout(Duration::from_secs(timeout_secs), send_fut).await {
                        Ok(Ok(r)) => r,
                        Ok(Err(e)) => return Err(McpError::Transport(e.to_string()).into()),
                        Err(_) => {
                            return Err(McpError::Timeout {
                                server: server_name.to_string(),
                                timeout_secs,
                            }
                            .into());
                        }
                    };

                let resp_body = resp
                    .text()
                    .await
                    .map_err(|e| McpError::Transport(e.to_string()))?;

                Self::parse_jsonrpc_response(server_name, original_tool, &resp_body, Some(req_id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_client_manager_empty() {
        let manager = McpClientManager::new();
        assert_eq!(manager.active_servers_count().await, 0);
        let schemas = manager.get_tool_schemas().await;
        assert!(schemas.is_empty());
    }

    #[tokio::test]
    async fn test_mcp_client_invalid_namespace() {
        let manager = McpClientManager::new();
        let res = manager
            .call_tool("invalid_tool_name", &serde_json::json!({}))
            .await;
        assert!(res.is_err());
    }
}
