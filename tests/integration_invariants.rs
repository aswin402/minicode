use minicode::context::graph::CodeGraph;
use minicode::context::invariants::InvariantChecker;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_layer_inversion_detection() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    let service_dir = src_dir.join("agent");
    let ui_dir = src_dir.join("ui");
    fs::create_dir_all(&service_dir).expect("create service");
    fs::create_dir_all(&ui_dir).expect("create ui");

    // UI component
    fs::write(
        ui_dir.join("order_modal.rs"),
        r#"
pub fn render_order_modal() {}
"#,
    )
    .expect("write order_modal.rs");

    // Service component erroneously invoking UI
    fs::write(
        service_dir.join("order_service.rs"),
        r#"
pub fn execute_order() {
    render_order_modal();
}
"#,
    )
    .expect("write order_service.rs");

    let mut graph = CodeGraph::new();
    let _ = graph.build_graph(dir.path());

    let report = InvariantChecker::check_workspace(dir.path(), Some(&graph), None)
        .expect("check workspace invariants");

    let layer_violation = report.violations.iter().find(|v| v.rule_id == "INV-001");
    assert!(layer_violation.is_some());
    assert!(layer_violation
        .unwrap()
        .message
        .contains("Service layer symbol `execute_order`"));
}

#[test]
fn test_mutual_call_cycle_detection() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("module_a.rs"),
        r#"
pub fn alpha() {
    beta();
}
"#,
    )
    .expect("write module_a.rs");

    fs::write(
        src_dir.join("module_b.rs"),
        r#"
pub fn beta() {
    alpha();
}
"#,
    )
    .expect("write module_b.rs");

    let mut graph = CodeGraph::new();
    let _ = graph.build_graph(dir.path());

    let report = InvariantChecker::check_workspace(dir.path(), Some(&graph), None)
        .expect("check workspace invariants");

    let cycle_violation = report.violations.iter().find(|v| v.rule_id == "INV-003");
    assert!(cycle_violation.is_some());
    assert!(cycle_violation
        .unwrap()
        .message
        .contains("mutual recursion"));
}

#[tokio::test]
async fn test_architecture_invariants_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("clean.rs"),
        r#"
pub fn pure_helper() -> i32 {
    42
}
"#,
    )
    .expect("write clean.rs");

    let args = json!({
        "target_file": "src/clean.rs"
    });

    let res = minicode::tools::registry::context_tools::dispatch(
        "architecture_invariants",
        &args,
        dir.path(),
    )
    .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Multi-File Dependency Invariant Report"));
}
