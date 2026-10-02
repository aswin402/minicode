use minicode::tools::browser::BrowserManager;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

static FIXTURE_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <title>Minicode E-Commerce Checkout</title>
    <!-- Missing viewport intentionally to test QA audit warning -->
    <style>
        body { font-family: sans-serif; margin: 20px; }
        .hidden { display: none; }
        .alert-error { color: #d32f2f; background: #ffebee; padding: 10px; border-radius: 4px; }
        .alert-success { color: #388e3c; background: #e8f5e9; padding: 10px; border-radius: 4px; }
    </style>
</head>
<body>
    <h1>Real-World Checkout Experience</h1>

    <!-- QA Audit Test Case: Broken Image -->
    <img src="/broken-asset.png" alt="Broken Item Thumbnail" />

    <!-- QA Audit Test Case: Dead/Placeholder Links -->
    <a href="#">Dead Anchor Link</a>
    <a href="javascript:void(0)">Void Anchor Link</a>

    <!-- QA Audit Test Case: Form Input Without Label -->
    <input type="text" placeholder="Promo Voucher" id="unlabelled-voucher" />

    <!-- Form with Dynamic Submission -->
    <form id="checkout-form" onsubmit="event.preventDefault(); submitOrder();">
        <label for="cust-email">Customer Email</label>
        <input type="email" id="cust-email" name="email" required />

        <label for="cust-name">Customer Name</label>
        <input type="text" id="cust-name" name="name" required />

        <button type="submit" id="submit-btn" onclick="submitOrder();">Place Order Now</button>
    </form>

    <div id="status-message" class="hidden"></div>

    <!-- Web Component with Shadow DOM -->
    <user-badge id="user-badge"></user-badge>

    <script>
        class UserBadge extends HTMLElement {
            constructor() {
                super();
                const shadow = this.attachShadow({ mode: 'open' });
                const wrapper = document.createElement('div');
                wrapper.innerHTML = `
                    <div style="border: 1px dashed #888; padding: 8px; margin-top: 15px;">
                        <span>Shadow User: Senior Developer</span>
                        <button id="shadow-action-btn">Edit in Shadow</button>
                    </div>
                `;
                shadow.appendChild(wrapper);
            }
        }
        customElements.define('user-badge', UserBadge);

        async function submitOrder() {
            const statusDiv = document.getElementById('status-message');
            statusDiv.className = '';
            statusDiv.innerText = 'Processing order...';

            try {
                const res = await fetch('/api/checkout', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        email: document.getElementById('cust-email').value,
                        name: document.getElementById('cust-name').value
                    })
                });

                if (!res.ok) {
                    const err = await res.json().catch(() => ({ error: 'Unknown Error' }));
                    statusDiv.className = 'alert-error';
                    statusDiv.innerText = 'Checkout Failed: ' + (err.error || res.statusText);
                    console.error('API Error: Server returned ' + res.status);
                    return;
                }

                const data = await res.json();
                statusDiv.className = 'alert-success';
                statusDiv.innerText = 'Order Placed Successfully: ' + data.order_id;
            } catch (e) {
                statusDiv.className = 'alert-error';
                statusDiv.innerText = 'Network Exception: ' + e.message;
                console.error('Network Exception:', e);
            }
        }
    </script>
</body>
</html>"##;

async fn start_test_server() -> (u16, oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind ephemeral TCP port");
    let port = listener.local_addr().unwrap().port();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                Ok((mut stream, _)) = listener.accept() => {
                    tokio::spawn(async move {
                        let mut buf = [0u8; 2048];
                        let n = match stream.read(&mut buf).await {
                            Ok(n) if n > 0 => n,
                            _ => return,
                        };
                        let req_str = String::from_utf8_lossy(&buf[..n]);

                        let response = if req_str.contains("/api/checkout") {
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"order_id\": \"live_srv_456\"}".to_string()
                        } else if req_str.contains("/broken-asset.png") {
                            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                        } else {
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                FIXTURE_HTML.len(),
                                FIXTURE_HTML
                            )
                        };

                        let _ = stream.write_all(response.as_bytes()).await;
                        let _ = stream.flush().await;
                    });
                }
            }
        }
    });

    (port, shutdown_tx)
}

#[tokio::test]
async fn test_realworld_browser_gsd_and_page_agent_pipeline() {
    // Force Chrome engine if available for standard CDP support
    std::env::set_var("MINICODE_BROWSER", "chrome");

    let (port, shutdown_server) = start_test_server().await;
    let base_url = format!("http://127.0.0.1:{}/", port);
    let ws = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // Give the local server a few milliseconds to spin up
    tokio::time::sleep(Duration::from_millis(50)).await;

    // ─────────────────────────────────────────────────────────────────────────
    // Phase 1: QA Audit (`browser_qa_audit`)
    // ─────────────────────────────────────────────────────────────────────────
    println!("▶ Testing browser_qa_audit on {}", base_url);
    let qa_res = ToolRegistry::dispatch(
        &ws,
        "call_qa_audit",
        "browser_qa_audit",
        &json!({
            "url": base_url,
            "mode": "headless"
        }),
        None,
        1,
    )
    .await;

    assert!(qa_res.success, "browser_qa_audit failed: {}", qa_res.output);
    println!("✔ QA Audit Output:\n{}", qa_res.output);

    // Verify QA audit detected the issues in the fixture
    assert!(
        qa_res.output.contains("Website QA Audit Report"),
        "QA report header missing"
    );
    assert!(
        qa_res.output.contains("Broken Images") || qa_res.output.contains("broken-asset.png"),
        "Expected broken image detection"
    );
    assert!(
        qa_res.output.contains("Empty / Placeholder Links")
            || qa_res.output.contains("Dead Anchor"),
        "Expected dead/placeholder links detection"
    );
    assert!(
        qa_res.output.contains("viewport"),
        "Expected missing viewport detection"
    );

    // ─────────────────────────────────────────────────────────────────────────
    // Phase 2: Deep Grounding Inspection (`browser_inspect_dom`)
    // ─────────────────────────────────────────────────────────────────────────
    println!("▶ Testing browser_inspect_dom (Shadow DOM piercing & Geometry)");
    let dom_res = ToolRegistry::dispatch(
        &ws,
        "call_inspect_dom",
        "browser_inspect_dom",
        &json!({
            "mode": "headless"
        }),
        None,
        2,
    )
    .await;

    assert!(
        dom_res.success,
        "browser_inspect_dom failed: {}",
        dom_res.output
    );
    println!("✔ Inspect DOM Output:\n{}", dom_res.output);

    // Verify DOM inspect found interactive elements and pierced Shadow DOM
    assert!(
        dom_res.output.contains("In-Page Visual DOM Tree"),
        "DOM report header missing"
    );
    assert!(
        dom_res.output.contains("submit-btn")
            || dom_res.output.contains("Place Order Now")
            || dom_res.output.contains("cust-email"),
        "Expected form elements in visual tree"
    );
    assert!(
        dom_res.output.contains("shadow-action-btn")
            || dom_res.output.contains("Edit in Shadow")
            || dom_res.output.contains("user-badge"),
        "Expected Shadow DOM pierced elements"
    );

    // ─────────────────────────────────────────────────────────────────────────
    // Phase 3: Route Interception & Fault Mocking (`browser_mock_route`)
    // ─────────────────────────────────────────────────────────────────────────
    println!("▶ Testing browser_mock_route (Mocking 500 error on /api/checkout)");
    let mock_res = ToolRegistry::dispatch(
        &ws,
        "call_mock_500",
        "browser_mock_route",
        &json!({
            "pattern": "*/api/checkout*",
            "status": 500,
            "body": "{\"error\": \"Payment Gateway Timeout\", \"code\": 500}",
            "content_type": "application/json",
            "mode": "headless"
        }),
        None,
        3,
    )
    .await;

    assert!(
        mock_res.success,
        "browser_mock_route failed: {}",
        mock_res.output
    );
    println!("✔ Mock Route Output:\n{}", mock_res.output);
    assert!(
        mock_res
            .output
            .contains("Registered mock route for pattern '*/api/checkout*'"),
        "Mock route confirmation missing"
    );

    // ─────────────────────────────────────────────────────────────────────────
    // Phase 4: Atomic Multi-Action Batch Pipeline (`browser_batch`)
    // ─────────────────────────────────────────────────────────────────────────
    println!("▶ Testing browser_batch (Fill -> Submit -> Wait -> Assert)");
    let batch_res = ToolRegistry::dispatch(
        &ws,
        "call_batch_checkout",
        "browser_batch",
        &json!({
            "actions": [
                {
                    "action": "fill",
                    "selector": "#cust-email",
                    "text": "senior_dev@example.com"
                },
                {
                    "action": "fill",
                    "selector": "#cust-name",
                    "text": "Senior Rust Engineer"
                },
                {
                    "action": "click",
                    "selector": "#submit-btn"
                },
                {
                    "action": "wait_for_selector",
                    "selector": "#status-message.alert-error",
                    "timeout_ms": 4000
                },
                {
                    "action": "assert_text",
                    "selector": "#status-message",
                    "text": "Payment Gateway Timeout"
                }
            ],
            "mode": "headless"
        }),
        None,
        4,
    )
    .await;

    assert!(
        batch_res.success,
        "browser_batch failed: {}",
        batch_res.output
    );
    println!("✔ Batch Pipeline Output:\n{}", batch_res.output);
    assert!(
        batch_res
            .output
            .contains("All 5 step(s) executed successfully."),
        "Batch pipeline did not complete all 5 steps successfully"
    );
    assert!(
        batch_res.output.contains("fill"),
        "Batch output missing fill step"
    );
    assert!(
        batch_res.output.contains("click"),
        "Batch output missing click step"
    );
    assert!(
        batch_res.output.contains("assert_text"),
        "Batch output missing assert_text step"
    );

    // ─────────────────────────────────────────────────────────────────────────
    // Phase 5: Diagnostics Aggregation (`browser_debug_bundle`)
    // ─────────────────────────────────────────────────────────────────────────
    println!("▶ Testing browser_debug_bundle");
    let bundle_res = ToolRegistry::dispatch(
        &ws,
        "call_debug_bundle",
        "browser_debug_bundle",
        &json!({
            "mode": "headless"
        }),
        None,
        5,
    )
    .await;

    assert!(
        bundle_res.success,
        "browser_debug_bundle failed: {}",
        bundle_res.output
    );
    println!("✔ Debug Bundle Output:\n{}", bundle_res.output);
    assert!(
        bundle_res.output.contains("Browser Diagnostic Bundle"),
        "Debug bundle title missing"
    );

    // ─────────────────────────────────────────────────────────────────────────
    // Phase 6: Clean Teardown
    // ─────────────────────────────────────────────────────────────────────────
    let shutdown_success = BrowserManager::shutdown_live_engine()
        .await
        .unwrap_or(false);
    println!("✔ Browser engine shutdown: {}", shutdown_success);
    let _ = shutdown_server.send(());
}
