//! `minibrowse` — Fast, low-RAM agentic browser, research extractor, Web debugger,
//! and Flutter VM Service inspector built natively for `minicode`.

pub mod fallback;
pub mod ffi;
pub mod flutter_vm;
pub mod net_policy;
pub mod server;
pub mod vision;
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
            webgl: false,
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

        // 6. Concurrent `sessionId` ScopedTarget routing WITHOUT switching global active tab!
        // Active tab is currently "main" (at Mocked Page). Query tab-2 via sessionId while main stays active.
        let attach_tab2 = call_cdp(
            16,
            "Target.attachToTarget",
            json!({ "targetId": tab2_id, "flatten": true }),
        )
        .await;
        let sid_tab2 = attach_tab2["result"]["sessionId"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(sid_tab2, format!("mb-session-{tab2_id}"));

        let (mut ws2, _) = tokio_tungstenite::connect_async(ws_url).await.unwrap();
        ws2.send(Message::Text(
            json!({
                "id": 17,
                "sessionId": sid_tab2,
                "method": "Runtime.evaluate",
                "params": { "expression": "document.title", "returnByValue": true }
            })
            .to_string(),
        ))
        .await
        .unwrap();
        let scoped_resp = loop {
            if let Some(Ok(Message::Text(txt))) = ws2.next().await {
                let parsed: Value = serde_json::from_str(&txt).unwrap();
                if parsed.get("id").and_then(Value::as_u64) == Some(17) {
                    break parsed;
                }
            }
        };
        assert_eq!(scoped_resp["sessionId"], sid_tab2);
        assert_eq!(
            scoped_resp["result"]["result"]["value"], "MiniBrowse Docs",
            "Scoped sessionId call must read tab-2 without mutating main tab"
        );
        // Main tab is STILL "Mocked Page"!
        let still_main = call_cdp(
            18,
            "Runtime.evaluate",
            json!({ "expression": "document.title", "returnByValue": true }),
        )
        .await;
        assert_eq!(still_main["result"]["result"]["value"], "Mocked Page");

        // 7. History navigation (`Page.getNavigationHistory`, `MiniBrowse.goBack`, `MiniBrowse.goForward`)
        let hist = call_cdp(19, "Page.getNavigationHistory", json!({})).await;
        assert!(
            hist["result"]["entries"].as_array().unwrap().len() >= 2,
            "Expected at least 2 history entries on main tab"
        );
        let back = call_cdp(20, "MiniBrowse.goBack", json!({})).await;
        assert_eq!(back["result"]["navigated"], true);
        let after_back = call_cdp(
            21,
            "Runtime.evaluate",
            json!({ "expression": "document.title", "returnByValue": true }),
        )
        .await;
        assert_eq!(after_back["result"]["result"]["value"], "MiniBrowse Docs");

        let fwd = call_cdp(22, "MiniBrowse.goForward", json!({})).await;
        assert_eq!(fwd["result"]["navigated"], true);

        // 8. Native `Page.printToPDF` returns valid `%PDF-1.4` bytes
        let pdf_res = call_cdp(23, "Page.printToPDF", json!({ "landscape": false })).await;
        let pdf_b64 = pdf_res["result"]["data"].as_str().unwrap();
        assert!(
            pdf_b64.starts_with("JVBERi0xLj"),
            "Base64 PDF must start with %PDF-1.x header, got prefix: {}",
            &pdf_b64[..pdf_b64.len().min(16)]
        );

        // 9. `DOM.setFileInputFiles` reads local file and succeeds
        let tmp_upload = std::env::temp_dir().join("minibrowse_upload_test.txt");
        tokio::fs::write(&tmp_upload, b"hello agent upload")
            .await
            .unwrap();
        let upload_res = call_cdp(
            24,
            "DOM.setFileInputFiles",
            json!({ "files": [tmp_upload.to_string_lossy()] }),
        )
        .await;
        assert_eq!(upload_res["result"]["ok"], true);
        let _ = tokio::fs::remove_file(&tmp_upload).await;

        // 10. Event-driven `MiniBrowse.waitForSelector` + `semantic_tree` dump (`moli` inspirations)
        let _ = call_cdp(
            27,
            "Page.navigate",
            json!({ "url": format!("http://127.0.0.1:{origin_port}/") }),
        )
        .await;
        let wait_sel = call_cdp(
            28,
            "MiniBrowse.waitForSelector",
            json!({ "selector": "button", "state": "attached", "timeoutMs": 1000 }),
        )
        .await;
        assert_eq!(wait_sel["result"]["matched"], true);

        let sem_tree = call_cdp(
            29,
            "MiniBrowse.researchExtract",
            json!({ "format": "semantic_tree", "maxChars": 4000 }),
        )
        .await;
        assert_eq!(sem_tree["result"]["format"], "semantic_tree");
        assert!(sem_tree["result"]["markdown"]
            .as_str()
            .unwrap()
            .contains("Zero-Copy Rust Channels"));

        // 11. `--if-changed` perceptual dHash gate + `--annotate` Set-of-Mark + `visualDiff` + `apiTraffic`
        let shot_first = call_cdp(
            30,
            "MiniBrowse.captureScreenshot",
            json!({ "ifChanged": true, "annotate": true }),
        )
        .await;
        assert_eq!(shot_first["result"]["unchanged"], false);
        assert_eq!(shot_first["result"]["annotated"], true);

        let shot_second = call_cdp(
            31,
            "MiniBrowse.captureScreenshot",
            json!({ "ifChanged": true, "annotate": true }),
        )
        .await;
        assert_eq!(
            shot_second["result"]["unchanged"], true,
            "Identical consecutive capture with ifChanged=true must skip redundant PNG bytes"
        );

        let vdiff = call_cdp(
            32,
            "MiniBrowse.visualDiff",
            json!({
                "baseline": "e2e_cmp",
                "urlA": format!("http://127.0.0.1:{origin_port}/"),
                "urlB": "http://127.0.0.1:9/mocked-endpoint"
            }),
        )
        .await;
        assert!(
            vdiff["result"]["mismatch_pixels"].as_u64().unwrap_or(0) > 0,
            "Comparing two distinct pages must report non-zero pixel mismatch"
        );
        assert!(!vdiff["result"]["diff_png_base64"]
            .as_str()
            .unwrap()
            .is_empty());

        let api_tr = call_cdp(33, "MiniBrowse.apiTraffic", json!({})).await;
        assert!(api_tr["result"]["total_captured"].as_u64().unwrap_or(0) >= 1);

        // 12. `MiniBrowse.openOrReuseTab` (`ego-lite` inspiration) + `MiniBrowse.extractTables` + `proxyServer` context
        let reuse1 = call_cdp(
            35,
            "MiniBrowse.openOrReuseTab",
            json!({
                "url": format!("http://127.0.0.1:{origin_port}/"),
                "browserContextId": ctx_id
            }),
        )
        .await;
        let tid_a = reuse1["result"]["targetId"].as_str().unwrap().to_string();

        let reuse2 = call_cdp(
            36,
            "MiniBrowse.openOrReuseTab",
            json!({
                "url": format!("http://127.0.0.1:{origin_port}"),
                "browserContextId": ctx_id
            }),
        )
        .await;
        assert_eq!(reuse2["result"]["reused"], true);
        assert_eq!(reuse2["result"]["targetId"].as_str().unwrap(), tid_a);

        let tbl_res = call_cdp(37, "MiniBrowse.extractTables", json!({})).await;
        assert!(tbl_res["result"]["table_count"].is_number());

        let proxy_ctx = call_cdp(
            38,
            "Target.createBrowserContext",
            json!({ "proxyServer": "http://127.0.0.1:8888" }),
        )
        .await;
        let proxy_cid = proxy_ctx["result"]["browserContextId"]
            .as_str()
            .unwrap()
            .to_string();
        let _ = call_cdp(
            39,
            "Target.disposeBrowserContext",
            json!({ "browserContextId": proxy_cid }),
        )
        .await;

        // Dispose isolated context and verify tab count returns to 1
        let _ = call_cdp(
            34,
            "Target.disposeBrowserContext",
            json!({ "browserContextId": ctx_id }),
        )
        .await;
        let targets = call_cdp(35, "Target.getTargets", json!({})).await;
        assert_eq!(
            targets["result"]["targetInfos"].as_array().unwrap().len(),
            1
        );
    }

    #[test]
    fn test_webkit_content_blocker_rules_valid_json() {
        let rules_str = net_policy::build_webkit_content_blocker_json(
            true,
            net_policy::ResourceProfile::ResearchLean,
        )
        .expect("Expected non-empty content blocker JSON");
        let parsed: Value =
            serde_json::from_str(&rules_str).expect("WebKit content blocker JSON must parse");
        let arr = parsed.as_array().unwrap();
        assert!(
            arr.len() > 200,
            "Expected >200 native content blocker rules, got {}",
            arr.len()
        );
        assert_eq!(arr[0]["action"]["type"], "block");
    }

    #[test]
    fn test_kitesurf_gear0_css_box_flex_layout_overflow_and_2d_painter() {
        let mut page = fallback::PureRustPageState::new(640, 400, true);
        page.set_document_content(
            r##"<!doctype html>
            <html><head><title>Flex Layout & Overflow</title>
            <style>
              body { background: #10141f; color: #f8fafc; padding: 16px; }
              .row { display: flex; gap: 16px; }
              .card { width: 200px; padding: 12px; background: #1e293b; border: 1px solid #38bdf8; }
              .secret-btn { display: none; }
              #wide-banner { width: 920px; height: 28px; background: #ef4444; }
            </style></head>
            <body>
              <h1>Kitesurf Gear 0.5</h1>
              <div class="row">
                <div class="card"><button id="b1">Launch</button></div>
                <div class="card"><button id="b2">Inspect</button></div>
              </div>
              <button class="secret-btn">Hidden by stylesheet</button>
              <div id="wide-banner">Overflowing hero banner</div>
            </body></html>"##,
        );

        // 1. Verify stylesheet `display: none` pruning + flex-row distinct X coordinates
        let ui = page.capture_ui_diff(None, 4000);
        assert_eq!(ui["total_elements"], 2);
        let x1 = ui["elements"][0]["x"].as_i64().unwrap();
        let x2 = ui["elements"][1]["x"].as_i64().unwrap();
        let y1 = ui["elements"][0]["y"].as_i64().unwrap();
        let y2 = ui["elements"][1]["y"].as_i64().unwrap();
        assert!(
            x2 > x1 + 150,
            "Expected flex-row side-by-side X layout: x1={x1}, x2={x2}"
        );
        assert_eq!(y1, y2, "Expected flex-row items to share top Y coordinate");

        // 2. Verify Gear-0 horizontal overflow detection (`#wide-banner` 920px > 640px viewport)
        let audit = page.audit_web_debug();
        assert!(audit["page_horizontal_overflow_px"].as_i64().unwrap() > 250);
        assert_eq!(audit["overflow_culprits"][0]["id"], "wide-banner");

        // 3. Verify 2D software painter produces a full-viewport PNG (not a 64x36 solid stub)
        let png = page.render_synthetic_png();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.len() > 2_000);
    }
}
