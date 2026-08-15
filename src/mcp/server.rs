use crate::constants::{
    JSONRPC_INVALID_PARAMS, JSONRPC_INVALID_REQUEST, JSONRPC_METHOD_NOT_FOUND, JSONRPC_PARSE_ERROR,
    JSONRPC_VERSION, MCP_PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Standalone MCP Server exposing minicode's coding tools over stdio
pub struct MinicodeMcpServer {
    workspace_root: PathBuf,
}

impl MinicodeMcpServer {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
        }
    }

    /// Runs the MCP stdio JSON-RPC server loop
    pub async fn run_stdio(workspace_root: &Path) -> anyhow::Result<()> {
        let server = Self::new(workspace_root);
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();

        while let Some(line) = reader.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<JsonRpcRequest>(trimmed) {
                Ok(req) => {
                    let resp = server.handle_request(req).await;
                    if let Some(r) = resp {
                        let resp_str = format!("{}\n", serde_json::to_string(&r)?);
                        stdout.write_all(resp_str.as_bytes()).await?;
                        stdout.flush().await?;
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Malformed JSON-RPC request received over stdio");
                    let err_resp = JsonRpcResponse {
                        jsonrpc: JSONRPC_VERSION.to_string(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: JSONRPC_PARSE_ERROR,
                            message: format!("Parse error: {}", err),
                            data: None,
                        }),
                    };
                    let resp_str = format!("{}\n", serde_json::to_string(&err_resp)?);
                    stdout.write_all(resp_str.as_bytes()).await?;
                    stdout.flush().await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_request(&self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = req.id;

        // JSON-RPC 2.0: Notifications (id == None) MUST NOT receive a response
        if id.is_none() {
            if req.jsonrpc == JSONRPC_VERSION && req.method == "tools/call" {
                if let Some(params) = req.params {
                    if let Some(name) = params.get("name").and_then(|v| v.as_str()) {
                        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                        let res = crate::tools::ToolRegistry::dispatch(
                            &self.workspace_root,
                            "mcp_notification",
                            name,
                            &arguments,
                            None,
                            1,
                        )
                        .await;
                        if !res.success {
                            tracing::warn!(
                                tool = %name,
                                error = %res.output,
                                "MCP notification tool execution failed"
                            );
                        }
                    }
                }
            }
            return None;
        }

        if req.jsonrpc != JSONRPC_VERSION {
            return Some(JsonRpcResponse {
                jsonrpc: JSONRPC_VERSION.to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: JSONRPC_INVALID_REQUEST,
                    message: "Invalid Request: expected jsonrpc: \"2.0\"".to_string(),
                    data: None,
                }),
            });
        }

        match req.method.as_str() {
            "initialize" => Some(JsonRpcResponse {
                jsonrpc: JSONRPC_VERSION.to_string(),
                id,
                result: Some(json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        }
                    },
                    "serverInfo": {
                        "name": env!("CARGO_PKG_NAME"),
                        "version": env!("CARGO_PKG_VERSION")
                    }
                })),
                error: None,
            }),

            "notifications/initialized" => None,

            "tools/list" => {
                let tools: Vec<serde_json::Value> = crate::tools::ToolRegistry::get_tool_schemas()
                    .into_iter()
                    .map(|s| {
                        json!({
                            "name": s.name,
                            "description": s.description,
                            "inputSchema": s.parameters,
                        })
                    })
                    .collect();

                Some(JsonRpcResponse {
                    jsonrpc: JSONRPC_VERSION.to_string(),
                    id,
                    result: Some(json!({ "tools": tools })),
                    error: None,
                })
            }

            "tools/call" => {
                let Some(params) = req.params.as_ref().and_then(|p| p.as_object()) else {
                    return Some(JsonRpcResponse {
                        jsonrpc: JSONRPC_VERSION.to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: JSONRPC_INVALID_PARAMS,
                            message: "Missing or non-object 'params' field in tools/call request"
                                .into(),
                            data: None,
                        }),
                    });
                };
                let Some(name) = params.get("name").and_then(|v| v.as_str()) else {
                    return Some(JsonRpcResponse {
                        jsonrpc: JSONRPC_VERSION.to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: JSONRPC_INVALID_PARAMS,
                            message: "Missing 'name' field in tools/call params".into(),
                            data: None,
                        }),
                    });
                };
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                if !arguments.is_object() {
                    return Some(JsonRpcResponse {
                        jsonrpc: JSONRPC_VERSION.to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: JSONRPC_INVALID_PARAMS,
                            message: "'arguments' field in tools/call params must be an object"
                                .into(),
                            data: None,
                        }),
                    });
                }

                let tool_res = crate::tools::ToolRegistry::dispatch(
                    &self.workspace_root,
                    "mcp_call",
                    name,
                    &arguments,
                    None,
                    1,
                )
                .await;

                Some(JsonRpcResponse {
                    jsonrpc: JSONRPC_VERSION.to_string(),
                    id,
                    result: Some(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": tool_res.output
                            }
                        ],
                        "isError": !tool_res.success
                    })),
                    error: None,
                })
            }

            _ => Some(JsonRpcResponse {
                jsonrpc: JSONRPC_VERSION.to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: JSONRPC_METHOD_NOT_FOUND,
                    message: format!("Method '{}' not found", req.method),
                    data: None,
                }),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_server_initialize() {
        let server = MinicodeMcpServer::new(Path::new("."));
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "initialize".to_string(),
            params: None,
        };
        let resp = server.handle_request(req).await.unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[tokio::test]
    async fn test_mcp_server_tools_list() {
        let server = MinicodeMcpServer::new(Path::new("."));
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(2)),
            method: "tools/list".to_string(),
            params: None,
        };
        let resp = server.handle_request(req).await.unwrap();
        assert!(resp.result.is_some());
        let result = resp.result.unwrap();
        let tools = result.get("tools").unwrap().as_array().unwrap();
        assert_eq!(
            tools.len(),
            crate::tools::ToolRegistry::get_tool_schemas().len()
        );
    }

    #[tokio::test]
    async fn test_mcp_server_notification_returns_none() {
        let server = MinicodeMcpServer::new(Path::new("."));
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "notifications/initialized".to_string(),
            params: None,
        };
        let resp = server.handle_request(req).await;
        assert!(resp.is_none());
    }

    #[tokio::test]
    async fn test_mcp_server_rejects_invalid_params_shape() {
        let server = MinicodeMcpServer::new(Path::new("."));
        // params is an array instead of object
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(3)),
            method: "tools/call".to_string(),
            params: Some(json!(["invalid", "array"])),
        };
        let resp = server.handle_request(req).await.unwrap();
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, JSONRPC_INVALID_PARAMS);
    }
}
