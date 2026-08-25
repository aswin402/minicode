use minicode::tools::browser::validate_browser_url;
/// Integration tests for Phase 30: Multi-Engine Browser Core & Process Manager
///
/// Tests engine priorities, launch argument generation, ARIA tree extraction,
/// URL validation with localhost dev server support, and schema registration.
use minicode::tools::browser::{
    BrowserController, BrowserEngine, BrowserManager, BrowserMode, EngineConfig, GUI_PRIORITY,
    HEADLESS_PRIORITY,
};
use minicode::tools::registry::web_tools;
use std::path::PathBuf;
use std::str::FromStr;

#[test]
fn test_browser_engine_priorities() {
    assert_eq!(HEADLESS_PRIORITY.len(), 3);
    assert_eq!(HEADLESS_PRIORITY[0], BrowserEngine::Obscura);
    assert_eq!(HEADLESS_PRIORITY[1], BrowserEngine::Chrome);
    assert_eq!(HEADLESS_PRIORITY[2], BrowserEngine::Firefox);

    assert_eq!(GUI_PRIORITY.len(), 3);
    assert_eq!(GUI_PRIORITY[0], BrowserEngine::Chrome);
    assert_eq!(GUI_PRIORITY[1], BrowserEngine::Obscura);
    assert_eq!(GUI_PRIORITY[2], BrowserEngine::Firefox);
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

    assert!(names.contains(&"fetch_or_browse".to_string()));
    assert!(names.contains(&"search_web".to_string()));
    assert!(names.contains(&"browser_navigate".to_string()));
    assert!(names.contains(&"browser_snapshot".to_string()));
    assert!(names.contains(&"browser_eval".to_string()));
    assert!(names.contains(&"browser_screenshot".to_string()));
}
