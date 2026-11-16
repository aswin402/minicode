//! Pure-Rust Dart VM Service JSON-RPC 2.0 Inspector & Zero-Polling `inotify` `.dart` Watcher.
//!
//! Connects directly to a running Flutter app's Dart VM Service WebSocket (`ws://127.0.0.1:<port>/.../ws`)
//! to map rendered Flutter widgets to exact `.dart:line:column` source coordinates, trigger sub-second
//! `reloadSources` hot reload, and toggle `ext.flutter.debugPaint` / `platformBrightnessOverride`.

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlutterVmAction {
    Status,
    WidgetTree,
    HotReload,
    DebugPaint { enabled: bool },
    Brightness { dark: bool },
    TimeDilation { factor: f64 },
}

impl FlutterVmAction {
    #[must_use]
    pub fn from_params(params: &Value) -> Self {
        let action = params
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("status")
            .trim()
            .to_ascii_lowercase();
        match action.as_str() {
            "widget_tree" | "widgets" | "tree" => Self::WidgetTree,
            "hot_reload" | "reload" => Self::HotReload,
            "debug_paint" => Self::DebugPaint {
                enabled: params
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            },
            "brightness" | "theme" => Self::Brightness {
                dark: params
                    .get("dark")
                    .and_then(Value::as_bool)
                    .or_else(|| {
                        params
                            .get("mode")
                            .and_then(Value::as_str)
                            .map(|m| m.eq_ignore_ascii_case("dark"))
                    })
                    .unwrap_or(true),
            },
            "time_dilation" => Self::TimeDilation {
                factor: params.get("factor").and_then(Value::as_f64).unwrap_or(1.0),
            },
            _ => Self::Status,
        }
    }
}

/// Normalizes a Dart VM Observatory / Service HTTP or WS URI (or bare port) into a WebSocket URL.
#[must_use]
pub fn normalize_vm_service_ws_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(port) = trimmed.parse::<u16>() {
        return Some(format!("ws://127.0.0.1:{port}/ws"));
    }
    let candidate = if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("ws://")
        || trimmed.starts_with("wss://")
    {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };

    let mut parsed = url::Url::parse(&candidate).ok()?;
    let target_scheme = match parsed.scheme() {
        "https" | "wss" => "wss",
        _ => "ws",
    };
    let _ = parsed.set_scheme(target_scheme);
    let path = parsed.path().trim_end_matches('/').to_string();
    if !path.ends_with("/ws") {
        let next_path = if path.is_empty() {
            "/ws".to_string()
        } else {
            format!("{path}/ws")
        };
        parsed.set_path(&next_path);
    }
    Some(parsed.to_string())
}

/// Executes a Dart VM Service JSON-RPC 2.0 inspection or control action.
pub async fn execute_flutter_vm_action(
    raw_vm_url: &str,
    action: FlutterVmAction,
) -> Result<Value, String> {
    let ws_url = normalize_vm_service_ws_url(raw_vm_url)
        .ok_or_else(|| format!("Invalid Dart VM Service URL: '{raw_vm_url}'"))?;

    let connect_fut = tokio_tungstenite::connect_async(&ws_url);
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(4), connect_fut)
        .await
        .map_err(|_| format!("Timed out connecting to Dart VM Service at {ws_url}"))?
        .map_err(|e| format!("Failed connecting to Dart VM Service at {ws_url}: {e}"))?;

    let mut req_id = 1u64;
    let vm_info = vm_rpc_call(&mut ws, &mut req_id, "getVM", json!({})).await?;
    let isolates = vm_info
        .get("isolates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let main_isolate_id = isolates
        .iter()
        .find(|iso| {
            iso.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n == "main" || n.contains("main"))
        })
        .or_else(|| isolates.first())
        .and_then(|iso| iso.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let result = match action {
        FlutterVmAction::Status => json!({
            "connected": true,
            "vm_ws_url": ws_url,
            "vm_name": vm_info.get("name").cloned().unwrap_or(Value::Null),
            "dart_version": vm_info.get("version").cloned().unwrap_or(Value::Null),
            "architecture_bits": vm_info.get("architectureBits").cloned().unwrap_or(Value::Null),
            "main_isolate_id": main_isolate_id,
            "isolate_count": isolates.len(),
        }),
        FlutterVmAction::HotReload => {
            let iso_id =
                main_isolate_id.ok_or_else(|| "No active Dart isolate found in VM".to_string())?;
            let report = vm_rpc_call(
                &mut ws,
                &mut req_id,
                "reloadSources",
                json!({ "isolateId": iso_id }),
            )
            .await?;
            json!({
                "action": "hot_reload",
                "isolate_id": iso_id,
                "report": report,
            })
        }
        FlutterVmAction::DebugPaint { enabled } => {
            let iso_id =
                main_isolate_id.ok_or_else(|| "No active Dart isolate found in VM".to_string())?;
            let resp = vm_rpc_call(
                &mut ws,
                &mut req_id,
                "ext.flutter.debugPaint",
                json!({
                    "isolateId": iso_id,
                    "enabled": if enabled { "true" } else { "false" }
                }),
            )
            .await?;
            json!({
                "action": "debug_paint",
                "enabled": enabled,
                "response": resp,
            })
        }
        FlutterVmAction::Brightness { dark } => {
            let iso_id =
                main_isolate_id.ok_or_else(|| "No active Dart isolate found in VM".to_string())?;
            let mode_val = if dark {
                "Brightness.dark"
            } else {
                "Brightness.light"
            };
            let resp = vm_rpc_call(
                &mut ws,
                &mut req_id,
                "ext.flutter.platformBrightnessOverride",
                json!({
                    "isolateId": iso_id,
                    "value": mode_val
                }),
            )
            .await?;
            json!({
                "action": "brightness",
                "value": mode_val,
                "response": resp,
            })
        }
        FlutterVmAction::TimeDilation { factor } => {
            let iso_id =
                main_isolate_id.ok_or_else(|| "No active Dart isolate found in VM".to_string())?;
            let clamped = factor.clamp(0.1, 20.0);
            let resp = vm_rpc_call(
                &mut ws,
                &mut req_id,
                "ext.flutter.timeDilation",
                json!({
                    "isolateId": iso_id,
                    "timeDilation": clamped.to_string()
                }),
            )
            .await?;
            json!({
                "action": "time_dilation",
                "factor": clamped,
                "response": resp,
            })
        }
        FlutterVmAction::WidgetTree => {
            let iso_id =
                main_isolate_id.ok_or_else(|| "No active Dart isolate found in VM".to_string())?;
            let tree_raw = vm_rpc_call(
                &mut ws,
                &mut req_id,
                "ext.flutter.inspector.getRootWidgetSummaryTree",
                json!({
                    "isolateId": iso_id,
                    "objectGroup": "minibrowse_inspector"
                }),
            )
            .await?;
            let root = tree_raw.get("result").unwrap_or(&tree_raw);
            let mut compact_nodes = Vec::new();
            flatten_widget_summary_tree(root, 0, &mut compact_nodes, 160);
            // Best-effort dispose of inspector object group so Flutter heap stays clean
            let _ = vm_rpc_call(
                &mut ws,
                &mut req_id,
                "ext.flutter.inspector.disposeGroup",
                json!({
                    "isolateId": iso_id,
                    "objectGroup": "minibrowse_inspector"
                }),
            )
            .await;
            json!({
                "action": "widget_tree",
                "isolate_id": iso_id,
                "widget_count": compact_nodes.len(),
                "widgets": compact_nodes,
            })
        }
    };

    let _ = ws.close(None).await;
    Ok(result)
}

async fn vm_rpc_call<S>(
    ws: &mut S,
    req_id: &mut u64,
    method: &str,
    params: Value,
) -> Result<Value, String>
where
    S: futures::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
        + futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    let id_str = req_id.to_string();
    *req_id += 1;
    let payload = json!({
        "jsonrpc": "2.0",
        "id": id_str,
        "method": method,
        "params": params,
    });
    ws.send(Message::Text(payload.to_string()))
        .await
        .map_err(|e| format!("Dart VM WS send failed ({method}): {e}"))?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(format!("Timed out waiting for Dart VM method '{method}'"));
        }
        let msg = tokio::time::timeout(remaining, ws.next())
            .await
            .map_err(|_| format!("Timed out waiting for Dart VM method '{method}'"))?
            .ok_or_else(|| format!("Dart VM WebSocket closed during '{method}'"))?
            .map_err(|e| format!("Dart VM WS read error ({method}): {e}"))?;

        if let Message::Text(txt) = msg {
            let Ok(val) = serde_json::from_str::<Value>(&txt) else {
                continue;
            };
            let matches_id = val
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == id_str);
            if !matches_id {
                continue;
            }
            if let Some(err) = val.get("error") {
                let emsg = err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown Dart VM RPC error");
                return Err(format!("Dart VM error in '{method}': {emsg}"));
            }
            return Ok(val.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

fn flatten_widget_summary_tree(node: &Value, depth: usize, out: &mut Vec<Value>, max_nodes: usize) {
    if out.len() >= max_nodes || !node.is_object() {
        return;
    }
    let description = node
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let widget_type = node
        .get("widgetRuntimeType")
        .and_then(Value::as_str)
        .unwrap_or(description);
    if !widget_type.is_empty() {
        let loc = node.get("creationLocation").and_then(|cl| {
            let file = cl.get("file").and_then(Value::as_str)?;
            let line = cl.get("line").and_then(Value::as_u64).unwrap_or(0);
            let col = cl.get("column").and_then(Value::as_u64).unwrap_or(0);
            Some(format!("{file}:{line}:{col}"))
        });
        out.push(json!({
            "depth": depth,
            "widget": widget_type,
            "description": description,
            "source": loc,
        }));
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            flatten_widget_summary_tree(child, depth + 1, out, max_nodes);
            if out.len() >= max_nodes {
                break;
            }
        }
    }
}

/// Optional zero-polling Linux `inotify` `.dart` watcher (adapted from `flutterff-rs`)
/// that triggers automatic hot-reload whenever a `.dart` source file is saved.
#[cfg(target_os = "linux")]
pub fn spawn_dart_inotify_watcher(
    watch_root: PathBuf,
    vm_url: Option<String>,
    engine: crate::webview::BrowserEngineHandle,
) {
    std::thread::Builder::new()
        .name("minibrowse-dart-watch".to_string())
        .spawn(move || {
            let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
            if fd < 0 {
                return;
            }
            register_dir_watches_recursive(fd, &watch_root);
            let mut buf = [0u8; 4096];
            loop {
                let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 {
                    break;
                }
                if contains_dart_event(&buf[..n as usize]) {
                    std::thread::sleep(Duration::from_millis(220));
                    // Drain coalesced save bursts
                    let _ = unsafe {
                        libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK);
                        while libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) > 0 {}
                        libc::fcntl(fd, libc::F_SETFL, 0)
                    };
                    if let Some(ref vm) = vm_url {
                        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            let _ = rt.block_on(execute_flutter_vm_action(
                                vm,
                                FlutterVmAction::HotReload,
                            ));
                        }
                    } else {
                        let (tx, _rx) = tokio::sync::oneshot::channel();
                        let _ = engine.send(crate::webview::BrowserBackendCmd::Reload {
                            ignore_cache: true,
                            reply: tx,
                        });
                    }
                }
            }
            unsafe {
                libc::close(fd);
            }
        })
        .ok();
}

#[cfg(target_os = "linux")]
fn register_dir_watches_recursive(fd: i32, dir: &Path) {
    if let Ok(c_path) = std::ffi::CString::new(dir.to_string_lossy().as_bytes()) {
        unsafe {
            let _ = libc::inotify_add_watch(
                fd,
                c_path.as_ptr(),
                libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_CREATE,
            );
        }
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if !s.starts_with('.') && s != "build" && s != "target" && s != "node_modules" {
                register_dir_watches_recursive(fd, &p);
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn contains_dart_event(bytes: &[u8]) -> bool {
    let hdr_size = std::mem::size_of::<libc::inotify_event>();
    let mut offset = 0usize;
    while offset + hdr_size <= bytes.len() {
        let ev = unsafe { &*(bytes.as_ptr().add(offset) as *const libc::inotify_event) };
        let name_len = ev.len as usize;
        let name_start = offset + hdr_size;
        let name_end = (name_start + name_len).min(bytes.len());
        if name_end > name_start {
            let slice = &bytes[name_start..name_end];
            let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
            if let Ok(name) = std::str::from_utf8(&slice[..end]) {
                if Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dart"))
                {
                    return true;
                }
            }
        }
        offset = name_start + name_len;
    }
    false
}
