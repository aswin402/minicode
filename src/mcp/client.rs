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
            }
        }
        Ok(())
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
    fn parse_jsonrpc_response(server_name: &str, tool_name: &str, body: &str) -> Result<String> {
        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(result) = resp.get("result") {
                if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
                    let mut combined = String::new();
                    for item in content {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            combined.push_str(text);
                        }
                    }
                    return Ok(combined);
                }
                return Ok(serde_json::to_string_pretty(result)?);
            } else if let Some(error) = resp.get("error") {
                return Err(McpError::ToolCallFailed {
                    server: server_name.into(),
                    tool: tool_name.into(),
                    reason: error.to_string(),
                }
                .into());
            }
        }
        Ok(body.to_string())
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

                let mut child_cmd = tokio::process::Command::new(cmd);
                if let Some(args) = &server_cfg.args {
                    child_cmd.args(args);
                }
                if let Some(env_vars) = &server_cfg.env {
                    for (k, v) in env_vars {
                        child_cmd.env(k, v);
                    }
                }

                child_cmd.stdin(std::process::Stdio::piped());
                child_cmd.stdout(std::process::Stdio::piped());
                child_cmd.stderr(std::process::Stdio::piped());
                child_cmd.kill_on_drop(true);

                #[cfg(unix)]
                {
                    child_cmd.process_group(0);
                }

                let mut child = child_cmd.spawn().map_err(|e| McpError::ConnectionFailed {
                    server: server_name.into(),
                    reason: format!("Failed to spawn process '{}': {}", cmd, e),
                })?;

                let request = serde_json::json!({
                    "jsonrpc": JSONRPC_VERSION,
                    "id": req_id,
                    "method": MCP_METHOD_TOOLS_CALL,
                    "params": {
                        "name": original_tool,
                        "arguments": arguments
                    }
                });

                let request_str = format!("{}\n", serde_json::to_string(&request)?);

                if let Some(mut stdin) = child.stdin.take() {
                    let write_res = async {
                        stdin.write_all(request_str.as_bytes()).await?;
                        stdin.flush().await?;
                        Ok::<(), std::io::Error>(())
                    }
                    .await;

                    if let Err(e) = write_res {
                        return Err(McpError::Transport(e.to_string()).into());
                    }
                }

                let wait_fut = child.wait_with_output();
                let output =
                    match tokio::time::timeout(Duration::from_secs(timeout_secs), wait_fut).await {
                        Ok(Ok(out)) => out,
                        Ok(Err(e)) => {
                            return Err(McpError::Transport(e.to_string()).into());
                        }
                        Err(_) => {
                            return Err(McpError::Timeout {
                                server: server_name.to_string(),
                                timeout_secs,
                            }
                            .into());
                        }
                    };

                let stdout_str = String::from_utf8_lossy(&output.stdout);
                if stdout_str.trim().is_empty() {
                    let stderr_str = String::from_utf8_lossy(&output.stderr);
                    if !stderr_str.trim().is_empty() {
                        return Err(McpError::ToolCallFailed {
                            server: server_name.into(),
                            tool: original_tool.into(),
                            reason: stderr_str.to_string(),
                        }
                        .into());
                    }
                    return Ok("✔ MCP tool executed (no output)".to_string());
                }

                Self::parse_jsonrpc_response(server_name, original_tool, &stdout_str)
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
                        "arguments": arguments
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

                Self::parse_jsonrpc_response(server_name, original_tool, &resp_body)
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
