use crate::constants::{
    DEFAULT_MAP_TOKENS, JSONRPC_INVALID_PARAMS, JSONRPC_INVALID_REQUEST, JSONRPC_METHOD_NOT_FOUND,
    JSONRPC_PARSE_ERROR, JSONRPC_SERVER_ERROR, JSONRPC_VERSION, MCP_PROTOCOL_VERSION,
};
use crate::context::graph::CodeGraph;
use crate::tools::{exec, fs, parse_u64_param, search};
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
                        let _ = self.execute_tool(name, &arguments).await;
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
                    message: format!(
                        "Invalid JSON-RPC version '{}', expected '{}'",
                        req.jsonrpc, JSONRPC_VERSION
                    ),
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
                    "serverInfo": {
                        "name": env!("CARGO_PKG_NAME"),
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        }
                    }
                })),
                error: None,
            }),

            "notifications/initialized" => None,

            "tools/list" => {
                let tools = vec![
                    json!({
                        "name": "read_file",
                        "description": "Read file contents in workspace with optional line range",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Relative path to file" },
                                "start_line": { "type": "integer", "description": "1-indexed start line" },
                                "end_line": { "type": "integer", "description": "1-indexed end line" }
                            },
                            "required": ["path"]
                        }
                    }),
                    json!({
                        "name": "write_file",
                        "description": "Create or overwrite file in workspace",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Relative path to file" },
                                "content": { "type": "string", "description": "Complete text content" }
                            },
                            "required": ["path", "content"]
                        }
                    }),
                    json!({
                        "name": "patch_file",
                        "description": "Patch file using exact substring search and replace blocks",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Relative path to file" },
                                "search_block": { "type": "string", "description": "Exact text to find" },
                                "replace_block": { "type": "string", "description": "Replacement text" }
                            },
                            "required": ["path", "search_block", "replace_block"]
                        }
                    }),
                    json!({
                        "name": "exec_cmd",
                        "description": "Execute command in workspace sandbox with token output compaction",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "command": { "type": "string", "description": "Command string to run" },
                                "timeout_secs": { "type": "integer", "description": "Optional timeout in seconds" }
                            },
                            "required": ["command"]
                        }
                    }),
                    json!({
                        "name": "grep_search",
                        "description": "Search workspace with regex respecting .gitignore",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": { "type": "string", "description": "Regex or text pattern" },
                                "is_regex": { "type": "boolean", "description": "Whether query is regex" },
                                "file_pattern": { "type": "string", "description": "Glob filter" }
                            },
                            "required": ["query"]
                        }
                    }),
                    json!({
                        "name": "repo_map",
                        "description": "Get AST-ranked skeleton map of codebase symbols",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "max_tokens": { "type": "integer", "description": "Max tokens for skeleton map" }
                            }
                        }
                    }),
                    json!({
                        "name": "impact_analysis",
                        "description": "Analyze blast radius and architectural dependencies for a symbol or file",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "target": { "type": "string", "description": "Symbol name or file path to analyze" }
                            },
                            "required": ["target"]
                        }
                    }),
                    json!({
                        "name": "locate_symbol",
                        "description": "Instantly locate symbol declarations and signatures across codebase",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string", "description": "Symbol name to locate" },
                                "limit": { "type": "integer", "description": "Max results" }
                            },
                            "required": ["name"]
                        }
                    }),
                ];

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

                let result = self.execute_tool(name, &arguments).await;
                match result {
                    Ok(text) => Some(JsonRpcResponse {
                        jsonrpc: JSONRPC_VERSION.to_string(),
                        id,
                        result: Some(json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": text
                                }
                            ]
                        })),
                        error: None,
                    }),
                    Err(err_msg) => Some(JsonRpcResponse {
                        jsonrpc: JSONRPC_VERSION.to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: JSONRPC_SERVER_ERROR,
                            message: err_msg,
                            data: None,
                        }),
                    }),
                }
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

    async fn execute_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> std::result::Result<String, String> {
        match name {
            "read_file" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'path'".to_string())?;
                let start_line =
                    parse_u64_param(args.get("start_line")).and_then(|v| usize::try_from(v).ok());
                let end_line =
                    parse_u64_param(args.get("end_line")).and_then(|v| usize::try_from(v).ok());
                fs::read_file(&self.workspace_root, path, start_line, end_line)
                    .map_err(|e| e.to_string())
            }
            "write_file" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'path'".to_string())?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'content'".to_string())?;
                fs::write_file(&self.workspace_root, path, content).map_err(|e| e.to_string())
            }
            "patch_file" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'path'".to_string())?;
                let search_block = args
                    .get("search_block")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'search_block'".to_string())?;
                let replace_block = args
                    .get("replace_block")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'replace_block'".to_string())?;
                fs::patch_file(&self.workspace_root, path, search_block, replace_block)
                    .map_err(|e| e.to_string())
            }
            "exec_cmd" => {
                let command = args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'command'".to_string())?;
                tracing::info!(command = %command, "MCP client invoking exec_cmd");
                let timeout = parse_u64_param(args.get("timeout_secs"));
                exec::exec_cmd(&self.workspace_root, command, timeout)
                    .await
                    .map_err(|e| e.to_string())
            }
            "grep_search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'query'".to_string())?;
                let is_regex = args
                    .get("is_regex")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let pattern = args.get("file_pattern").and_then(|v| v.as_str());
                search::grep_search(&self.workspace_root, query, is_regex, pattern)
                    .map_err(|e| e.to_string())
            }
            "repo_map" => {
                let max_tokens = parse_u64_param(args.get("max_tokens"))
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(DEFAULT_MAP_TOKENS);
                let mut graph = CodeGraph::new();
                graph
                    .build_graph(&self.workspace_root)
                    .map_err(|e| e.to_string())?;
                Ok(graph.format_repomap(&self.workspace_root, &[], max_tokens))
            }
            "impact_analysis" => {
                let target = args
                    .get("target")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'target'".to_string())?;
                let mut graph = CodeGraph::new();
                graph
                    .build_graph(&self.workspace_root)
                    .map_err(|e| e.to_string())?;
                let report = graph
                    .get_blast_radius(target, &self.workspace_root)
                    .map_err(|e| e.to_string())?;
                Ok(report.summary)
            }
            "locate_symbol" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter 'name'".to_string())?;
                let limit = parse_u64_param(args.get("limit"))
                    .unwrap_or(crate::constants::DEFAULT_LOCATE_SYMBOL_LIMIT as u64)
                    as usize;
                let mut index = crate::context::index::SymbolIndex::new();
                index
                    .build_index(&self.workspace_root)
                    .map_err(|e| e.to_string())?;
                let matches = if name.contains(' ') {
                    index.search_symbols(name, limit)
                } else {
                    let mut res = index.locate_symbol(name);
                    if res.is_empty() {
                        res = index.search_symbols(name, limit);
                    }
                    res.truncate(limit);
                    res
                };
                Ok(index.format_matches(&matches, &self.workspace_root))
            }
            unknown => Err(format!("Unknown tool '{}'", unknown)),
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
        assert_eq!(tools.len(), 8);
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
