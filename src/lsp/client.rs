use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::error::{MinicodeError, Result};
use crate::lsp::protocol::{decode_message, encode_message};

/// Represents a definition or reference location returned by LSP.
#[derive(Debug, Clone)]
pub struct LspLocation {
    pub file_path: PathBuf,
    pub line: u32,
    pub character: u32,
}

/// Lightweight, zero-overhead asynchronous LSP client connecting to local language servers over stdio.
pub struct LspClient {
    pub server_name: String,
    pub workspace_root: PathBuf,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout: Arc<Mutex<Option<ChildStdout>>>,
    child: Arc<Mutex<Option<Child>>>,
    next_id: AtomicU64,
}

impl LspClient {
    /// Attempts to auto-detect and spawn the appropriate language server for the workspace.
    pub async fn auto_detect(workspace_root: &Path) -> Option<Self> {
        if workspace_root.join("Cargo.toml").exists() {
            Self::spawn(workspace_root, "rust-analyzer", &["rust-analyzer"])
                .await
                .ok()
        } else if workspace_root.join("tsconfig.json").exists()
            || workspace_root.join("package.json").exists()
        {
            Self::spawn(
                workspace_root,
                "typescript",
                &["typescript-language-server", "--stdio"],
            )
            .await
            .ok()
        } else if workspace_root.join("pyproject.toml").exists()
            || workspace_root.join("requirements.txt").exists()
        {
            Self::spawn(
                workspace_root,
                "pyright",
                &["pyright-langserver", "--stdio"],
            )
            .await
            .ok()
        } else if workspace_root.join("go.mod").exists() {
            Self::spawn(workspace_root, "gopls", &["gopls"]).await.ok()
        } else {
            None
        }
    }

    /// Spawns a language server command in the workspace directory.
    pub async fn spawn(
        workspace_root: &Path,
        server_name: &str,
        cmd_and_args: &[&str],
    ) -> Result<Self> {
        if cmd_and_args.is_empty() {
            return Err(MinicodeError::Lsp(
                "Empty command provided for LSP spawn".to_string(),
            ));
        }

        let prog = cmd_and_args[0];
        let args = &cmd_and_args[1..];

        let mut child = Command::new(prog)
            .args(args)
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                MinicodeError::Lsp(format!("Failed to spawn LSP server '{}': {}", prog, e))
            })?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();

        let client = Self {
            server_name: server_name.to_string(),
            workspace_root: workspace_root.to_path_buf(),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(stdout)),
            child: Arc::new(Mutex::new(Some(child))),
            next_id: AtomicU64::new(1),
        };

        // Initialize handshake
        client.initialize().await?;

        Ok(client)
    }

    /// Performs the standard LSP `initialize` and `initialized` handshake.
    async fn initialize(&self) -> Result<()> {
        let root_uri = format!("file://{}", self.workspace_root.display());
        let req = json!({
            "jsonrpc": "2.0",
            "id": self.next_id.fetch_add(1, Ordering::SeqCst),
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "definition": { "dynamicRegistration": false },
                        "references": { "dynamicRegistration": false }
                    }
                }
            }
        });

        let _ = self
            .send_request_with_timeout(req, Duration::from_secs(5))
            .await?;

        // Send initialized notification
        let notify = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        self.send_notification(notify).await?;

        Ok(())
    }

    /// Sends a JSON-RPC request and awaits the matching response ID.
    pub async fn send_request_with_timeout(
        &self,
        payload: Value,
        timeout_dur: Duration,
    ) -> Result<Value> {
        let req_id = payload.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        let encoded = encode_message(&payload)
            .map_err(|e| MinicodeError::Lsp(format!("Failed to encode LSP message: {}", e)))?;

        // Write to stdin
        {
            let mut stdin_guard = self.stdin.lock().await;
            if let Some(ref mut stdin) = *stdin_guard {
                stdin
                    .write_all(&encoded)
                    .await
                    .map_err(|e| MinicodeError::Lsp(format!("LSP stdin write error: {}", e)))?;
                stdin
                    .flush()
                    .await
                    .map_err(|e| MinicodeError::Lsp(format!("LSP stdin flush error: {}", e)))?;
            } else {
                return Err(MinicodeError::Lsp("LSP stdin is closed".to_string()));
            }
        }

        // Read from stdout until response is found or timeout
        let stdout_arc = self.stdout.clone();
        let read_future = async {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let mut stdout_guard = stdout_arc.lock().await;
                if let Some(ref mut stdout) = *stdout_guard {
                    match stdout.read(&mut chunk).await {
                        Ok(n) if n > 0 => {
                            buf.extend_from_slice(&chunk[..n]);
                            while let Ok(Some((msg, consumed))) = decode_message(&buf) {
                                buf.drain(..consumed);
                                if msg.get("id").and_then(|v| v.as_u64()) == Some(req_id) {
                                    return Ok(msg);
                                }
                            }
                        }
                        Ok(_) => {
                            return Err(MinicodeError::Lsp("LSP stdout reached EOF".to_string()))
                        }
                        Err(e) => {
                            return Err(MinicodeError::Lsp(format!("LSP stdout read error: {}", e)))
                        }
                    }
                } else {
                    return Err(MinicodeError::Lsp("LSP stdout is closed".to_string()));
                }
            }
        };

        tokio::time::timeout(timeout_dur, read_future)
            .await
            .map_err(|_| {
                MinicodeError::Lsp(format!(
                    "LSP request {} timed out after {:?}",
                    req_id, timeout_dur
                ))
            })?
    }

    /// Sends a JSON-RPC notification (no response expected).
    pub async fn send_notification(&self, payload: Value) -> Result<()> {
        let encoded = encode_message(&payload)
            .map_err(|e| MinicodeError::Lsp(format!("Failed to encode LSP notification: {}", e)))?;
        let mut stdin_guard = self.stdin.lock().await;
        if let Some(ref mut stdin) = *stdin_guard {
            stdin
                .write_all(&encoded)
                .await
                .map_err(|e| MinicodeError::Lsp(format!("LSP stdin notify write error: {}", e)))?;
            stdin
                .flush()
                .await
                .map_err(|e| MinicodeError::Lsp(format!("LSP stdin notify flush error: {}", e)))?;
        }
        Ok(())
    }

    /// Queries `textDocument/definition` for the given file, line (0-indexed), and character position.
    pub async fn goto_definition(
        &self,
        file_path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>> {
        let abs_path = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            self.workspace_root.join(file_path)
        };
        let uri = format!("file://{}", abs_path.display());
        let req_id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let req = json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }
        });

        let resp = self
            .send_request_with_timeout(req, Duration::from_secs(4))
            .await?;
        let mut locations = Vec::new();

        if let Some(result) = resp.get("result") {
            if let Some(arr) = result.as_array() {
                for loc in arr {
                    if let Some(loc_parsed) = Self::parse_location(loc) {
                        locations.push(loc_parsed);
                    }
                }
            } else if let Some(loc_parsed) = Self::parse_location(result) {
                locations.push(loc_parsed);
            }
        }

        Ok(locations)
    }

    /// Queries `textDocument/references` for the given file, line (0-indexed), and character position.
    pub async fn find_references(
        &self,
        file_path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>> {
        let abs_path = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            self.workspace_root.join(file_path)
        };
        let uri = format!("file://{}", abs_path.display());
        let req_id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let req = json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": "textDocument/references",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
                "context": { "includeDeclaration": true }
            }
        });

        let resp = self
            .send_request_with_timeout(req, Duration::from_secs(4))
            .await?;
        let mut locations = Vec::new();

        if let Some(result) = resp.get("result").and_then(|r| r.as_array()) {
            for loc in result {
                if let Some(loc_parsed) = Self::parse_location(loc) {
                    locations.push(loc_parsed);
                }
            }
        }

        Ok(locations)
    }

    fn parse_location(val: &Value) -> Option<LspLocation> {
        let uri = val.get("uri").or_else(|| val.get("targetUri"))?.as_str()?;
        let path_str = uri.strip_prefix("file://")?;
        let range = val.get("range").or_else(|| val.get("targetRange"))?;
        let start = range.get("start")?;
        let line = start.get("line")?.as_u64()? as u32;
        let character = start.get("character")?.as_u64()? as u32;

        Some(LspLocation {
            file_path: PathBuf::from(path_str),
            line,
            character,
        })
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if let Ok(mut child_guard) = self.child.try_lock() {
            if let Some(mut child) = child_guard.take() {
                let _ = child.start_kill();
            }
        }
    }
}
