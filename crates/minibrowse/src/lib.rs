//! `minibrowse` — Fast, low-RAM agentic browser, research extractor, Web debugger,
//! and Flutter VM Service inspector built natively for `minicode`.

pub mod fallback;
pub mod ffi;
pub mod flutter_vm;
pub mod net_policy;
pub mod server;
pub mod webview;

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{SinkExt, StreamExt};
    use serde_json::{json, Value};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    #[test]
    fn test_base64_and_png_signature_valid() {
        let png = fallback::encode_solid_aura_png(64, 36);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png
            .ends_with(&[0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]));
        let b64 = ffi::base64_encode(&png);
        assert!(!b64.is_empty());
    }

    #[test]
    fn test_webkit_gtk_dynamic_ffi_symbol_resolution() {
        // On hosts with libwebkit2gtk installed, every symbol in WebKitGtkApi resolves cleanly;
        // on minimal containers without it, try_load returns a descriptive Err without panicking.
        if let Ok(_api) = ffi::WebKitGtkApi::try_load() {
            ffi::trim_process_heap();
        }
    }

    #[test]
    fn test_vm_service_url_normalization() {
        assert_eq!(
            flutter_vm::normalize_vm_service_ws_url("8181").as_deref(),
            Some("ws://127.0.0.1:8181/ws")
        );
        assert_eq!(
            flutter_vm::normalize_vm_service_ws_url("http://127.0.0.1:54321/abcDef=/").as_deref(),
            Some("ws://127.0.0.1:54321/abcDef=/ws")
        );
        assert_eq!(
            flutter_vm::normalize_vm_service_ws_url("ws://localhost:9100/token/ws").as_deref(),
            Some("ws://localhost:9100/token/ws")
        );
    }

    #[tokio::test]
    async fn test_minibrowse_server_cdp_research_and_mock_e2e() {
        // 1. Start a tiny local HTTP test origin serving a rich documentation + Vite overlay page
        let origin_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin_port = origin_listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = origin_listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                let html = r#"<!doctype html>
<html>
<head><title>MiniBrowse Docs</title><meta name="viewport" content="width=device-width, initial-scale=1"></head>
<body>
  <nav role="navigation"><a href="https://noise.example.com">Skip Nav Noise</a></nav>
  <main>
    <h1>Zero-Copy Rust Channels</h1>
    <p>Bounded async channels guarantee backpressure under heavy agentic workloads.</p>
    <pre>let (tx, rx) = tokio::sync::mpsc::channel(64);</pre>
    <p><a href="https://docs.rs/tokio">Tokio Reference</a></p>
    <button aria-label="Run Benchmark">Run Benchmark</button>
  </main>
  <vite-error-overlay>SyntaxError: Unexpected token in App.tsx:14:2</vite-error-overlay>
</body>
</html>"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    html.len(),
                    html
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });

        // 2. Spawn MiniBrowse engine (force Pure-Rust fallback so test is deterministic on any CI/sandbox)
        std::env::set_var("MINIBROWSE_FORCE_FALLBACK", "1");
        let engine = webview::spawn_browser_backend(webview::LaunchOptions {
            headless: true,
            width: 1280,
            height: 800,
            initial_url: None,
            adblock: true,
        })
        .unwrap();

        let srv = server::MiniBrowseServer::bind(0, engine, 1280, 800)
            .await
            .unwrap();
        let mb_port = srv.port();
        tokio::spawn(async move {
            let _ = srv.run().await;
        });

        // 3. Probe HTTP /json/version exactly like minicode's BrowserManager & CdpClient
        let http = reqwest::Client::new();
        let ver: Value = http
            .get(format!("http://127.0.0.1:{mb_port}/json/version"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let ws_url = ver["webSocketDebuggerUrl"].as_str().unwrap();
        assert!(ws_url.starts_with(&format!("ws://127.0.0.1:{mb_port}/devtools/")));

        // 4. Connect over WebSocket and run CDP + MiniBrowse.* commands
        let (mut ws, _) = tokio_tungstenite::connect_async(ws_url).await.unwrap();

        let mut call_cdp = async |id: u64, method: &str, params: Value| -> Value {
            ws.send(Message::Text(
                json!({ "id": id, "method": method, "params": params }).to_string(),
            ))
            .await
            .unwrap();
            while let Some(Ok(Message::Text(txt))) = ws.next().await {
                let parsed: Value = serde_json::from_str(&txt).unwrap();
                if parsed.get("id").and_then(Value::as_u64) == Some(id) {
                    return parsed;
                }
            }
            panic!("WebSocket closed before response for {method}");
        };

        let nav_res = call_cdp(
            1,
            "Page.navigate",
            json!({ "url": format!("http://127.0.0.1:{origin_port}/") }),
        )
        .await;
        assert_eq!(nav_res["result"]["frameId"], "main");

        let title_res = call_cdp(
            2,
            "Runtime.evaluate",
            json!({ "expression": "document.title", "returnByValue": true }),
        )
        .await;
        assert_eq!(title_res["result"]["result"]["value"], "MiniBrowse Docs");

        // Research extraction strips <nav> noise and scores query hits
        let research = call_cdp(
            3,
            "MiniBrowse.researchExtract",
            json!({ "query": "backpressure channel", "maxChars": 4000 }),
        )
        .await;
        let md = research["result"]["markdown"].as_str().unwrap();
        assert!(md.contains("Zero-Copy Rust Channels"));
        assert!(md.contains("tokio::sync::mpsc::channel(64)"));
        assert!(!md.contains("Skip Nav Noise"));

        // Web Debug Audit detects <vite-error-overlay>
        let debug_audit = call_cdp(4, "MiniBrowse.webDebugAudit", json!({})).await;
        let overlays = debug_audit["result"]["framework_overlays"]
            .as_array()
            .unwrap();
        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0]["framework"], "vite");
        assert!(overlays[0]["error"]
            .as_str()
            .unwrap()
            .contains("App.tsx:14:2"));

        // Screenshot returns valid Base64 PNG
        let shot = call_cdp(5, "Page.captureScreenshot", json!({ "format": "png" })).await;
        assert!(!shot["result"]["data"].as_str().unwrap().is_empty());

        // Declarative Mock Route works
        let _ = call_cdp(
            6,
            "MiniBrowse.setMockRoutes",
            json!({
                "routes": [{
                    "pattern": "*/mocked-endpoint*",
                    "status": 200,
                    "body": "<html><head><title>Mocked Page</title></head><body><h1>Mocked OK</h1></body></html>"
                }]
            }),
        )
        .await;
        let _ = call_cdp(
            7,
            "Page.navigate",
            json!({ "url": "http://127.0.0.1:9/mocked-endpoint" }),
        )
        .await;
        let mocked_title = call_cdp(
            8,
            "Runtime.evaluate",
            json!({ "expression": "document.title", "returnByValue": true }),
        )
        .await;
        assert_eq!(mocked_title["result"]["result"]["value"], "Mocked Page");

        // 5. Verify isolated BrowserContext + multi-tab cookie isolation + waitForIdle
        let idle = call_cdp(9, "MiniBrowse.waitForIdle", json!({ "quietMs": 10 })).await;
        assert_eq!(idle["result"]["idle"], true);

        let _ = call_cdp(
            10,
            "Network.setCookies",
            json!({ "cookies": [{ "name": "session_a", "value": "alpha" }] }),
        )
        .await;

        let ctx_res = call_cdp(11, "Target.createBrowserContext", json!({})).await;
        let ctx_id = ctx_res["result"]["browserContextId"]
            .as_str()
            .unwrap()
            .to_string();

        let tab2_res = call_cdp(
            12,
            "Target.createTarget",
            json!({
                "url": format!("http://127.0.0.1:{origin_port}/"),
                "browserContextId": ctx_id
            }),
        )
        .await;
        let tab2_id = tab2_res["result"]["targetId"].as_str().unwrap().to_string();
        assert_ne!(tab2_id, "main");

        // Isolated tab-2 must NOT inherit main tab's cookies!
        let tab2_cookies = call_cdp(13, "Network.getCookies", json!({})).await;
        assert_eq!(
            tab2_cookies["result"]["cookies"].as_array().unwrap().len(),
            0
        );

        // Switch back to main tab and verify session_a cookie is intact
        let _ = call_cdp(14, "Target.activateTarget", json!({ "targetId": "main" })).await;
        let main_cookies = call_cdp(15, "Network.getCookies", json!({})).await;
        assert_eq!(
            main_cookies["result"]["cookies"].as_array().unwrap().len(),
            1
        );
        assert_eq!(main_cookies["result"]["cookies"][0]["name"], "session_a");

        // Dispose isolated context and verify tab count returns to 1
        let _ = call_cdp(
            16,
            "Target.disposeBrowserContext",
            json!({ "browserContextId": ctx_id }),
        )
        .await;
        let targets = call_cdp(17, "Target.getTargets", json!({})).await;
        assert_eq!(
            targets["result"]["targetInfos"].as_array().unwrap().len(),
            1
        );
    }
}
