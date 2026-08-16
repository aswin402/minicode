use minicode::tools::browser::BrowserController;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn test_browser_aria_accessibility_tree_extraction() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Admin Portal</title></head>
        <body>
            <header><h1>Welcome Admin</h1></header>
            <nav>
                <a href="/dashboard">Dashboard</a>
                <a href="/settings">Settings</a>
            </nav>
            <main>
                <form action="/save" method="post">
                    <input type="text" name="server_name" placeholder="Server Name" />
                    <button type="submit">Save Changes</button>
                </form>
            </main>
        </body>
        </html>
    "#;

    let snapshot =
        BrowserController::parse_html_to_aria_snapshot("http://localhost:8080/admin", html);
    assert_eq!(snapshot.title, "Admin Portal");
    assert!(snapshot.interactive_elements.len() >= 3);

    let button = snapshot
        .interactive_elements
        .iter()
        .find(|e| e.role == "Button")
        .unwrap();
    assert!(button.name.contains("Save Changes"));

    let report = BrowserController::format_snapshot_report(&snapshot);
    assert!(report.contains("Admin Portal"));
    assert!(report.contains("@e"));
    assert!(report.contains("Save Changes"));
}

#[tokio::test]
async fn test_browser_snapshot_tool_dispatch() {
    let ws = PathBuf::from("/workspace");
    let sample_html =
        "<html><head><title>Test App</title></head><body><button>Click Me</button></body></html>";

    let res = ToolRegistry::dispatch(
        &ws,
        "call_browser_snap",
        "browser_snapshot",
        &json!({
            "url": "http://localhost:3000",
            "html": sample_html
        }),
        None,
        1,
    )
    .await;

    assert!(res.success);
    assert!(res.output.contains("Test App"));
    assert!(res.output.contains("Click Me"));
    assert!(res.output.contains("@e1"));
}
