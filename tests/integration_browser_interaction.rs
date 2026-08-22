/// Integration tests for Phase 31: Interactive Automation & Live Debugger
///
/// Tests versioned ARIA element refs, stale reference detection, DOM interaction
/// helpers, live console/network diagnostics collector, and schema registration.
use minicode::tools::browser::accessibility::AccessibilityManager;
use minicode::tools::browser::debug::{DebugCollector, LogLevel};
use minicode::tools::registry::web_tools;

#[test]
fn test_accessibility_versioned_ref_generation() {
    let mut mgr = AccessibilityManager::new();
    assert_eq!(mgr.revision(), 1);

    let html = r#"
        <header>
            <a href="/home">Home</a>
            <a href="/dashboard">Dashboard</a>
        </header>
        <main>
            <input type="text" name="search" placeholder="Search repos..." />
            <button type="submit">Search</button>
            <select name="sort">
                <option value="stars">Stars</option>
                <option value="forks">Forks</option>
            </select>
            <textarea name="comment" placeholder="Leave a comment"></textarea>
        </main>
    "#;

    let elements = mgr.update_from_html(html);
    assert_eq!(elements.len(), 6);

    assert_eq!(elements[0].ref_id, "@v1:e1");
    assert_eq!(elements[0].role, "Link");

    assert_eq!(elements[1].ref_id, "@v1:e2");
    assert_eq!(elements[1].name, "Dashboard");

    assert_eq!(elements[2].ref_id, "@v1:e3");
    assert_eq!(elements[2].role, "Input");

    assert_eq!(elements[3].ref_id, "@v1:e4");
    assert_eq!(elements[3].role, "Button");

    assert_eq!(elements[4].ref_id, "@v1:e5");
    assert_eq!(elements[4].role, "Select");

    assert_eq!(elements[5].ref_id, "@v1:e6");
    assert_eq!(elements[5].role, "TextBox");
}

#[test]
fn test_accessibility_ref_resolution_and_short_names() {
    let mut mgr = AccessibilityManager::new();
    let html = r#"
        <form>
            <input type="email" name="user_email" placeholder="you@domain.com" />
            <button type="submit">Continue</button>
        </form>
    "#;
    mgr.update_from_html(html);

    // Exact resolution
    let el = mgr
        .resolve_ref("@v1:e1")
        .expect("Must resolve exact @v1:e1");
    assert_eq!(el.attributes.get("name").unwrap(), "user_email");

    // Shorthand resolution (@e2 -> @v1:e2)
    let btn = mgr.resolve_ref("@e2").expect("Must resolve short @e2");
    assert_eq!(btn.name, "Continue");

    // Non-existent ref error
    let err = mgr.resolve_ref("@v1:e99");
    assert!(err.is_err());
}

#[test]
fn test_accessibility_stale_ref_detection() {
    let mut mgr = AccessibilityManager::new();
    let html_v1 = r#"<button>Step 1</button>"#;
    mgr.update_from_html(html_v1);
    assert!(mgr.resolve_ref("@v1:e1").is_ok());

    // Advance revision to v2
    mgr.next_revision();
    let html_v2 = r#"<button>Step 2</button>"#;
    mgr.update_from_html(html_v2);

    // Stale v1 ref must be rejected
    let stale_res = mgr.resolve_ref("@v1:e1");
    assert!(stale_res.is_err());
    let err_msg = stale_res.unwrap_err().to_string();
    assert!(err_msg.contains("Stale element reference '@v1:e1'"));
    assert!(err_msg.contains("Current page revision is v2"));

    // Current v2 ref must succeed
    assert!(mgr.resolve_ref("@v2:e1").is_ok());
}

#[test]
fn test_debug_collector_recording_and_formatting() {
    let collector = DebugCollector::new();

    collector.record_console(
        LogLevel::Error,
        "Uncaught ReferenceError: process is not defined at bundle.js:104",
    );
    collector.record_console(
        LogLevel::Warn,
        "Cookie was rejected because it had the SameSite=None attribute without Secure",
    );
    collector.record_network_error(
        "GET",
        "http://localhost:3000/api/auth/session",
        500,
        Some("Internal Server Error"),
    );

    let report = collector.format_report();
    assert!(report.contains("Browser Runtime Diagnostics"));
    assert!(report.contains("[HTTP 500]"));
    assert!(report.contains("GET http://localhost:3000/api/auth/session"));
    assert!(report.contains("[ERROR]"));
    assert!(report.contains("Uncaught ReferenceError"));
    assert!(report.contains("[WARN]"));

    collector.clear();
    let cleared_report = collector.format_report();
    assert!(cleared_report.contains("No runtime console errors"));
}

#[test]
fn test_phase31_web_tools_schemas_registered() {
    let schemas = web_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"browser_navigate".to_string()));
    assert!(names.contains(&"browser_snapshot".to_string()));
    assert!(names.contains(&"browser_click".to_string()));
    assert!(names.contains(&"browser_fill".to_string()));
    assert!(names.contains(&"browser_scroll".to_string()));
    assert!(names.contains(&"browser_debug_logs".to_string()));
    assert!(names.contains(&"browser_eval".to_string()));
    assert!(names.contains(&"browser_screenshot".to_string()));
}
