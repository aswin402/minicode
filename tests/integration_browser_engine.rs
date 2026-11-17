use minicode::tools::browser::validate_browser_url;
/// Integration tests for Phase 30: Multi-Engine Browser Core & Process Manager
///
/// Tests engine priorities, launch argument generation, ARIA tree extraction,
/// URL validation with localhost dev server support, and schema registration.
use minicode::tools::browser::{
    BrowserController, BrowserEngine, BrowserManager, BrowserMode, BrowserPurpose, EngineConfig,
    DEBUG_PRIORITY, GUI_PRIORITY, HEADLESS_PRIORITY,
};
use minicode::tools::registry::web_tools;
use std::path::PathBuf;
use std::str::FromStr;

#[test]
fn test_browser_engine_priorities() {
    assert_eq!(HEADLESS_PRIORITY.len(), 5);
    assert_eq!(HEADLESS_PRIORITY[0], BrowserEngine::MiniBrowse);
    assert_eq!(HEADLESS_PRIORITY[1], BrowserEngine::Chrome);
    assert_eq!(HEADLESS_PRIORITY[2], BrowserEngine::Firefox);
    assert_eq!(HEADLESS_PRIORITY[3], BrowserEngine::Safari);
    assert_eq!(HEADLESS_PRIORITY[4], BrowserEngine::Obscura);

    assert_eq!(GUI_PRIORITY.len(), 5);
    assert_eq!(GUI_PRIORITY[0], BrowserEngine::MiniBrowse);
    assert_eq!(GUI_PRIORITY[1], BrowserEngine::Chrome);
    assert_eq!(GUI_PRIORITY[2], BrowserEngine::Firefox);
    assert_eq!(GUI_PRIORITY[3], BrowserEngine::Safari);
    assert_eq!(GUI_PRIORITY[4], BrowserEngine::Obscura);

    assert_eq!(DEBUG_PRIORITY[0], BrowserEngine::MiniBrowse);
}

#[test]
fn test_browser_mode_parsing() {
    assert_eq!(
        BrowserMode::from_str("headless").unwrap(),
        BrowserMode::Headless
    );
    assert_eq!(BrowserMode::from_str("gui").unwrap(), BrowserMode::Gui);
    assert_eq!(BrowserMode::from_str("headed").unwrap(), BrowserMode::Gui);
    assert_eq!(BrowserMode::from_str("window").unwrap(), BrowserMode::Gui);
    assert!(BrowserMode::from_str("invalid_mode").is_err());
}

#[test]
fn test_launch_args_obscura() {
    let config = EngineConfig::new(
        BrowserEngine::Obscura,
        BrowserMode::Headless,
        9222,
        PathBuf::from("/usr/bin/obscura"),
        PathBuf::from("/tmp/minicode_obs_profile"),
    );
    let args = BrowserManager::build_launch_args(&config);
    assert_eq!(
        args,
        vec![
            "--stealth",
            "--allow-private-network",
            "serve",
            "--port",
            "9222"
        ]
    );
}

#[test]
fn test_launch_args_firefox_headless_and_gui() {
    let headless_config = EngineConfig::new(
        BrowserEngine::Firefox,
        BrowserMode::Headless,
        9223,
        PathBuf::from("/usr/bin/firefox"),
        PathBuf::from("/tmp/minicode_ff_profile"),
    );
    let h_args = BrowserManager::build_launch_args(&headless_config);
    assert!(h_args.contains(&"--headless".to_string()));
    assert!(h_args.contains(&"--remote-debugging-port".to_string()));
    assert!(h_args.contains(&"9223".to_string()));
    assert!(h_args.contains(&"--profile".to_string()));
    assert!(h_args.contains(&"/tmp/minicode_ff_profile".to_string()));
    assert!(h_args.contains(&"--no-remote".to_string()));

    let gui_config = EngineConfig::new(
        BrowserEngine::Firefox,
        BrowserMode::Gui,
        9223,
        PathBuf::from("/usr/bin/firefox"),
        PathBuf::from("/tmp/minicode_ff_gui_profile"),
    );
    let g_args = BrowserManager::build_launch_args(&gui_config);
    assert!(!g_args.contains(&"--headless".to_string()));
    assert!(g_args.contains(&"--remote-debugging-port".to_string()));
    assert!(g_args.contains(&"--no-remote".to_string()));
}

#[test]
fn test_launch_args_chrome_headless_and_gui() {
    let headless_config = EngineConfig::new(
        BrowserEngine::Chrome,
        BrowserMode::Headless,
        9224,
        PathBuf::from("/usr/bin/google-chrome"),
        PathBuf::from("/tmp/minicode_cr_profile"),
    );
    let h_args = BrowserManager::build_launch_args(&headless_config);
    assert!(h_args.contains(&"--headless=new".to_string()));
    assert!(h_args.contains(&"--remote-debugging-port=9224".to_string()));
    assert!(h_args.contains(&"--user-data-dir=/tmp/minicode_cr_profile".to_string()));
    assert!(h_args.contains(&"--disable-gpu".to_string()));
    assert!(h_args.contains(&"--no-sandbox".to_string()));

    let gui_config = EngineConfig::new(
        BrowserEngine::Chrome,
        BrowserMode::Gui,
        9224,
        PathBuf::from("/usr/bin/google-chrome"),
        PathBuf::from("/tmp/minicode_cr_gui_profile"),
    );
    let g_args = BrowserManager::build_launch_args(&gui_config);
    assert!(!g_args.contains(&"--headless=new".to_string()));
    assert!(g_args.contains(&"--remote-debugging-port=9224".to_string()));
    assert!(g_args.contains(&"--user-data-dir=/tmp/minicode_cr_gui_profile".to_string()));
}

#[test]
fn test_aria_snapshot_parsing_and_report() {
    let sample_html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Admin Panel - Minicode App</title></head>
        <body>
            <header>
                <h1>User Management</h1>
                <a href="/logout">Log Out</a>
            </header>
            <main>
                <form action="/users/add" method="POST">
                    <input type="text" name="email" placeholder="user@example.com" />
                    <select name="role">
                        <option value="admin">Admin</option>
                        <option value="member">Member</option>
                    </select>
                    <button type="submit">Create Account</button>
                </form>
            </main>
        </body>
        </html>
    "#;

    let snapshot =
        BrowserController::parse_html_to_aria_snapshot("http://localhost:8080/admin", sample_html);
    assert_eq!(snapshot.title, "Admin Panel - Minicode App");
    assert!(!snapshot.interactive_elements.is_empty());

    // Verify versioned element ref
    let first = &snapshot.interactive_elements[0];
    assert!(first.ref_id.starts_with("@v1:e"));

    let report = BrowserController::format_snapshot_report(&snapshot);
    assert!(report.contains("Admin Panel - Minicode App"));
    assert!(report.contains("Create Account"));
    assert!(report.contains("Log Out"));
    assert!(report.contains("@v1:e"));
}

#[test]
fn test_validate_browser_url_permits_localhost_and_file() {
    assert!(validate_browser_url("http://localhost:3000").is_ok());
    assert!(validate_browser_url("http://127.0.0.1:5173/dashboard").is_ok());
    assert!(validate_browser_url("http://0.0.0.0:8000").is_ok());
    assert!(validate_browser_url("https://crates.io/crates/tokio").is_ok());
    assert!(validate_browser_url("file:///tmp/index.html").is_ok());

    assert!(validate_browser_url("ftp://example.com").is_err());
    assert!(validate_browser_url("ssh://example.com").is_err());
}

#[test]
fn test_validate_browser_url_blocks_cloud_metadata() {
    assert!(validate_browser_url("http://169.254.169.254/latest/meta-data/").is_err());
}

#[test]
fn test_web_tools_schemas_registered() {
    let schemas = web_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"webfetch".to_string()));
    assert!(names.contains(&"search_web".to_string()));
    assert!(names.contains(&"browser_navigate".to_string()));
    assert!(names.contains(&"browser_snapshot".to_string()));
    assert!(names.contains(&"browser_eval".to_string()));
    assert!(names.contains(&"browser_screenshot".to_string()));
    assert!(names.contains(&"browser_interact".to_string()));
    assert!(names.contains(&"browser_research".to_string()));
    assert!(names.contains(&"browser_web_diagnose".to_string()));
    assert!(names.contains(&"flutter_debug".to_string()));
}

#[test]
fn test_spa_shell_detection_and_extended_batch_steps() {
    use minicode::tools::browser::{BatchStep, SmartMarkdownExtractor};

    let vite_shell = r#"<!doctype html><html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>"#;
    assert!(SmartMarkdownExtractor::is_client_rendered_shell(
        vite_shell, ""
    ));

    let ssr_article = r#"<!doctype html><html><body><div id="root"><h1>Documentation</h1></div><script src="/app.js"></script></body></html>"#;
    let rich_md = "# Documentation\n\nThis server-rendered guide contains plenty of immediate prose and examples so headless browser escalation is unnecessary for fast static ingestion.";
    assert!(!SmartMarkdownExtractor::is_client_rendered_shell(
        ssr_article,
        rich_md
    ));

    let steps = BatchStep::parse_steps(&serde_json::json!([
        { "action": "press_key", "key": "Enter", "ref": "@v1:e2" },
        { "action": "drag", "source_ref": "@v1:e3", "target_ref": "@v1:e4" },
        { "action": "set_checked", "selector": "#agree", "checked": true },
        { "action": "wait_for_text", "text": "Saved", "timeout_ms": 2000 },
        { "action": "assert_selector", "selector": "#toast" },
        { "action": "assert_url", "url_contains": "/dashboard" }
    ]))
    .unwrap();
    assert_eq!(steps.len(), 6);
}

#[tokio::test]
async fn test_minicode_cdp_client_drives_minibrowse_daemon_end_to_end() {
    use minicode::tools::browser::{CdpClient, MockRouteRule};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Locate compiled `minibrowse` workspace binary
    let exe = std::env::current_exe().unwrap();
    let target_debug_dir = exe.parent().and_then(|d| d.parent()).unwrap();
    let mb_bin = target_debug_dir.join("minibrowse");
    assert!(
        mb_bin.is_file(),
        "Expected workspace binary at {} (build workspace first)",
        mb_bin.display()
    );

    // 1. Start local test HTTP server with HTML, code block, Flutter semantics, and Vite overlay
    let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = origin.accept().await {
            let mut b = [0u8; 1024];
            let _ = s.read(&mut b).await;
            let body = r#"<!doctype html>
<html>
<head><title>Agentic Test App</title></head>
<body>
  <main>
    <h1>Async Stream Architecture</h1>
    <p>Backpressure-safe channels keep agent memory under 50 MiB.</p>
    <pre>let stream = Box::pin(rx);</pre>
    <flt-semantics role="button" aria-label="Increment Counter">Increment</flt-semantics>
  </main>
  <vite-error-overlay>ReferenceError: foo is not defined at Counter.tsx:9:4</vite-error-overlay>
</body>
</html>"#;
            let hdr = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(hdr.as_bytes()).await;
        }
    });

    // Pick an available port for minibrowse
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mb_port = probe.local_addr().unwrap().port();
    drop(probe);

    let cfg = EngineConfig::new(
        BrowserEngine::MiniBrowse,
        BrowserMode::Headless,
        mb_port,
        mb_bin,
        std::env::temp_dir().join(format!("minicode_mb_e2e_{mb_port}")),
    );
    let mut proc = BrowserManager::spawn_engine(&cfg).await.unwrap();

    // 2. Connect minicode's real CdpClient to minibrowse
    let cdp = CdpClient::connect(&proc.cdp_http_url).await.unwrap();
    cdp.navigate(&format!("http://127.0.0.1:{origin_port}/"))
        .await
        .unwrap();

    let html = cdp.get_document_html().await.unwrap();
    assert!(html.contains("Async Stream Architecture"));

    let research = cdp
        .minibrowse_research(Some("backpressure"), 4000)
        .await
        .unwrap();
    let md = research["markdown"].as_str().unwrap();
    assert!(md.contains("Async Stream Architecture"));
    assert!(md.contains("let stream = Box::pin(rx);"));

    let diag = cdp.minibrowse_web_diagnose().await.unwrap();
    let overlays = diag["framework_overlays"].as_array().unwrap();
    assert_eq!(overlays[0]["framework"], "vite");

    // Verify PageAgent::run_qa_audit fuses MiniBrowse Shadow DOM Vite overlay into QA report
    let qa = minicode::tools::browser::PageAgent::run_qa_audit(
        &cdp,
        &format!("http://127.0.0.1:{origin_port}/"),
    )
    .await
    .unwrap();
    assert!(qa.error_count >= 1);
    assert!(qa
        .console_errors
        .iter()
        .any(|e| e.contains("[vite-overlay]") && e.contains("Counter.tsx:9:4")));

    // Verify native keyboard dispatch, network waterfall, and storage inspection over CdpClient
    cdp.dispatch_key_press("Enter", 0).await.unwrap();
    let waterfall = cdp.minibrowse_network_waterfall().await.unwrap();
    assert!(waterfall.get("url").is_some());
    let storage = cdp.minibrowse_storage_inspect().await.unwrap();
    assert!(storage.get("local_storage").is_some());

    let flt = cdp
        .minibrowse_flutter_inspect(serde_json::json!({ "action": "status" }))
        .await
        .unwrap();
    assert!(flt["semantics_ui"]["flutter_semantics_active"]
        .as_bool()
        .unwrap());

    let png_bytes = cdp.take_screenshot().await.unwrap();
    assert!(png_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    // Verify mock route forwarding over CdpClient -> MiniBrowse
    cdp.add_mock_route(MockRouteRule {
        pattern: "*/api/mock-page*".to_string(),
        status: 200,
        body: "<!doctype html><html><head><title>Mocked Title</title></head><body>Mock OK</body></html>".to_string(),
        content_type: Some("text/html".to_string()),
    })
    .await
    .unwrap();
    cdp.navigate(&format!("http://127.0.0.1:{origin_port}/api/mock-page"))
        .await
        .unwrap();
    let title = cdp.evaluate_js("document.title").await.unwrap();
    assert!(title.contains("Mocked Title"));

    proc.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_safari_w3c_webdriver_http_adapter_end_to_end() {
    use minicode::tools::browser::CdpClient;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // 1x1 valid PNG in base64
    let png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let mut current_url = "about:blank".to_string();
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 65536];
            let Ok(n) = stream.read(&mut buf).await else {
                continue;
            };
            if n == 0 {
                continue;
            }
            let req_str = String::from_utf8_lossy(&buf[..n]);
            let first_line = req_str.lines().next().unwrap_or("");
            let body_part = req_str.split("\r\n\r\n").nth(1).unwrap_or("");

            let resp_json = if first_line.starts_with("POST /session ") {
                serde_json::json!({ "value": { "sessionId": "safari-sess-1" } })
            } else if first_line.starts_with("GET /session/safari-sess-1/window ") {
                serde_json::json!({ "value": "safari-win-1" })
            } else if first_line.starts_with("POST /session/safari-sess-1/url ") {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(body_part) {
                    if let Some(u) = v.get("url").and_then(|x| x.as_str()) {
                        current_url = u.to_string();
                    }
                }
                serde_json::json!({ "value": null })
            } else if first_line.starts_with("POST /session/safari-sess-1/execute/async ") {
                let expr = serde_json::from_str::<serde_json::Value>(body_part)
                    .ok()
                    .and_then(|v| {
                        v.pointer("/args/0")
                            .and_then(|s| s.as_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
                if expr.contains("outerHTML") {
                    serde_json::json!({
                        "value": {
                            "ok": true,
                            "type": "string",
                            "value": format!("<html><head><title>Safari Page</title></head><body>Visited {}</body></html>", current_url)
                        }
                    })
                } else if expr.contains("document.title") {
                    serde_json::json!({
                        "value": { "ok": true, "type": "string", "value": "Safari Page" }
                    })
                } else {
                    serde_json::json!({
                        "value": { "ok": true, "type": "number", "value": 2 }
                    })
                }
            } else if first_line.starts_with("GET /session/safari-sess-1/screenshot ") {
                serde_json::json!({ "value": png_b64 })
            } else {
                serde_json::json!({ "value": {} })
            };

            let body_bytes = resp_json.to_string();
            let http_resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body_bytes.len(),
                body_bytes
            );
            let _ = stream.write_all(http_resp.as_bytes()).await;
        }
    });

    let cdp = CdpClient::connect_for_engine(
        &format!("http://127.0.0.1:{port}"),
        BrowserEngine::Safari,
        BrowserMode::Gui,
    )
    .await
    .unwrap();

    cdp.navigate("https://webkit.org/").await.unwrap();
    let html = cdp.get_document_html().await.unwrap();
    assert!(html.contains("Safari Page"));
    assert!(html.contains("https://webkit.org/"));

    let title = cdp.evaluate_js("document.title").await.unwrap();
    assert!(title.contains("Safari Page"));

    let png = cdp.take_screenshot().await.unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[tokio::test]
async fn test_firefox_webdriver_bidi_adapter_end_to_end() {
    use futures::{SinkExt, StreamExt};
    use minicode::tools::browser::CdpClient;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    let png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                continue;
            };
            let (mut write, mut read) = ws.split();
            let mut nav_url = "about:blank".to_string();
            while let Some(Ok(Message::Text(txt))) = read.next().await {
                let Ok(req) = serde_json::from_str::<serde_json::Value>(&txt) else {
                    continue;
                };
                let id = req.get("id").and_then(|v| v.as_u64()).unwrap_or(1);
                let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
                let result = match method {
                    "session.new" => serde_json::json!({ "sessionId": "ff-bidi-1" }),
                    "session.subscribe" => serde_json::json!({}),
                    "browsingContext.getTree" => serde_json::json!({
                        "contexts": [{ "context": "ff-tab-1", "url": nav_url }]
                    }),
                    "browsingContext.navigate" => {
                        nav_url = req
                            .pointer("/params/url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("about:blank")
                            .to_string();
                        // Emit a BiDi console error event to verify DebugCollector capture
                        let ev = serde_json::json!({
                            "type": "event",
                            "method": "log.entryAdded",
                            "params": {
                                "level": "error",
                                "text": "Firefox BiDi ReferenceError: bar is undefined"
                            }
                        });
                        let _ = write.send(Message::Text(ev.to_string())).await;
                        serde_json::json!({ "navigation": "nav-1", "url": nav_url })
                    }
                    "script.evaluate" => {
                        let expr = req
                            .pointer("/params/expression")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let val_str = if expr.contains("outerHTML") {
                            format!(
                                "<html><head><title>Firefox BiDi App</title></head><body>At {nav_url}</body></html>"
                            )
                        } else {
                            "Firefox BiDi App".to_string()
                        };
                        serde_json::json!({
                            "type": "success",
                            "result": { "type": "string", "value": val_str }
                        })
                    }
                    "browsingContext.captureScreenshot" => {
                        serde_json::json!({ "data": png_b64 })
                    }
                    _ => serde_json::json!({}),
                };
                let reply = serde_json::json!({
                    "type": "success",
                    "id": id,
                    "result": result
                });
                let _ = write.send(Message::Text(reply.to_string())).await;
            }
        }
    });

    let cdp = CdpClient::connect_for_engine(
        &format!("http://127.0.0.1:{port}"),
        BrowserEngine::Firefox,
        BrowserMode::Headless,
    )
    .await
    .unwrap();

    cdp.navigate("https://mozilla.org/").await.unwrap();
    let html = cdp.get_document_html().await.unwrap();
    assert!(html.contains("Firefox BiDi App"));
    assert!(html.contains("https://mozilla.org/"));

    let png = cdp.take_screenshot().await.unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    let diag_report = cdp.debug_collector().format_report();
    assert!(diag_report.contains("Firefox BiDi ReferenceError"));
}

#[tokio::test]
async fn test_debug_tools_route_to_minibrowse_even_when_general_browser_configured_to_firefox() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = minicode::config::Config::default();
    cfg.browser.default_engine = Some("firefox".to_string());
    cfg.save(Some(tmp.path())).unwrap();

    // General browser honors configured Firefox primary
    let general = BrowserManager::resolve_priority_chain(
        BrowserPurpose::General,
        BrowserMode::Headless,
        None,
        tmp.path(),
    );
    assert_eq!(general[0], BrowserEngine::Firefox);

    // Debug tools strictly route to MiniBrowse (`minibrowser`) even when Firefox is configured!
    let debug = BrowserManager::resolve_priority_chain(
        BrowserPurpose::Debug,
        BrowserMode::Headless,
        None,
        tmp.path(),
    );
    assert_eq!(debug[0], BrowserEngine::MiniBrowse);

    // Spin up a local page with a Vite Shadow DOM error overlay and verify `diagnose_web_app`
    // launches MiniBrowse (NOT Firefox) and returns MiniBrowse root-cause diagnostics!
    let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = origin.accept().await {
            let mut b = [0u8; 1024];
            let _ = s.read(&mut b).await;
            let body = "<!doctype html><html><head><title>Debug Routing Check</title></head><body><vite-error-overlay>SyntaxError: Unexpected token</vite-error-overlay></body></html>";
            let hdr = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(hdr.as_bytes()).await;
        }
    });

    let report = BrowserController::diagnose_web_app(
        Some(&format!("http://127.0.0.1:{origin_port}/")),
        BrowserMode::Headless,
        tmp.path(),
    )
    .await
    .unwrap();
    assert!(
        report.contains("MiniBrowse Web Root-Cause Diagnostics (`MiniBrowse`)"),
        "Expected MiniBrowse debug engine even when Firefox is configured: {report}"
    );
    assert!(report.contains("SyntaxError: Unexpected token"));

    let _ = BrowserManager::shutdown_live_engine_opt(true).await;
}
