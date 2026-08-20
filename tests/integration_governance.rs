use minicode::context::governance::ArchitectureGovernor;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_governance_detects_clean_layered_architecture() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let src = root.join("src");
    let tools = src.join("tools");
    let ui = src.join("ui");

    fs::create_dir_all(&tools).unwrap();
    fs::create_dir_all(&ui).unwrap();

    fs::write(tools.join("mod.rs"), "pub fn execute() {}\n").unwrap();
    fs::write(
        ui.join("view.rs"),
        "use crate::tools::mod;\npub fn draw() {}\n",
    )
    .unwrap();

    let report = ArchitectureGovernor::scan_workspace(root).unwrap();

    assert!(report.health_score >= 80);
    assert_eq!(report.total_files, 2);
    assert!(report.circular_cycles.is_empty());
    assert!(report.layer_violations.is_empty());

    let md = report.format_markdown();
    assert!(md.contains("Architectural Health Report"));
    assert!(md.contains("100% DAG compliant"));
    assert!(md.contains("All architectural module boundaries intact"));
}

#[tokio::test]
async fn test_check_architecture_tool_dispatch() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let src = root.join("src");
    let core = src.join("core");
    fs::create_dir_all(&core).unwrap();
    fs::write(core.join("engine.rs"), "pub struct Engine;\n").unwrap();

    let res = ToolRegistry::dispatch(
        root,
        "call_arch_1",
        "check_architecture",
        &json!({}),
        None,
        1,
    )
    .await;

    assert!(res.success);
    assert!(res.output.contains("Architectural Health Report"));
    assert!(res.output.contains("100% DAG compliant"));
}
