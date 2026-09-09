use minicode::context::governance::ArchitectureGovernor;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_audit_architecture_detects_violations_and_cycles() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();

    let src = root.join("src");
    let core = src.join("core");
    let ui = src.join("ui");
    let data = src.join("data");

    fs::create_dir_all(&core).expect("create core dir");
    fs::create_dir_all(&ui).expect("create ui dir");
    fs::create_dir_all(&data).expect("create data dir");

    // Cycle: core -> ui -> data -> core
    // Layer violation: core imports ui (R1_CORE_NO_UI), data imports core (allowed), ui imports data (allowed)
    fs::write(
        core.join("engine.rs"),
        "use crate::ui::view;\npub struct Engine;\n",
    )
    .expect("write engine.rs");

    fs::write(
        ui.join("view.rs"),
        "use crate::data::store;\npub fn render_view() {}\n",
    )
    .expect("write view.rs");

    fs::write(
        data.join("store.rs"),
        "use crate::core::engine;\npub struct DataStore;\n",
    )
    .expect("write store.rs");

    let report = ArchitectureGovernor::scan_workspace(root).expect("scan workspace");

    assert!(
        report.health_score < 100,
        "Health score should be degraded due to violations and cycles, got {}",
        report.health_score
    );
    assert_eq!(report.total_files, 3);

    // Verify layer violations
    assert!(
        !report.layer_violations.is_empty(),
        "Expected boundary violations"
    );
    let r1_violation = report
        .layer_violations
        .iter()
        .find(|v| v.rule_id == "R1_CORE_NO_UI");
    assert!(
        r1_violation.is_some(),
        "Expected R1_CORE_NO_UI boundary violation"
    );

    // Verify circular cycles (Tarjan SCC)
    assert!(
        !report.circular_cycles.is_empty(),
        "Expected at least one circular cycle"
    );

    // Verify coupling metrics & Martin's instability
    assert!(
        !report.coupling_metrics.is_empty(),
        "Expected coupling metrics"
    );
    for metric in &report.coupling_metrics {
        assert!(
            metric.instability >= 0.0 && metric.instability <= 1.0,
            "Instability must be in range [0.0, 1.0], got {}",
            metric.instability
        );
    }
}

#[tokio::test]
async fn test_audit_architecture_tool_dispatch_modes() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();

    let src = root.join("src");
    let core = src.join("core");
    let ui = src.join("ui");

    fs::create_dir_all(&core).expect("create core dir");
    fs::create_dir_all(&ui).expect("create ui dir");

    // Clean layered setup: ui -> core
    fs::write(core.join("lib.rs"), "pub fn compute() -> i32 { 42 }\n").expect("write lib.rs");
    fs::write(
        ui.join("cli.rs"),
        "use crate::core::lib;\npub fn run() { println!(\"{}\", lib::compute()); }\n",
    )
    .expect("write cli.rs");

    // Test mode: "check"
    let res_check = ToolRegistry::dispatch(
        root,
        "call_audit_1",
        "audit_architecture",
        &json!({ "mode": "check" }),
        None,
        1,
    )
    .await;
    assert!(res_check.success);
    assert!(res_check.output.contains("Architectural Health Report"));

    // Test mode: "matrix"
    let res_matrix = ToolRegistry::dispatch(
        root,
        "call_audit_2",
        "audit_architecture",
        &json!({ "mode": "matrix" }),
        None,
        1,
    )
    .await;
    assert!(res_matrix.success);
    assert!(res_matrix.output.contains("Coupling & Instability Matrix"));

    // Test mode: "cycles"
    let res_cycles = ToolRegistry::dispatch(
        root,
        "call_audit_3",
        "audit_architecture",
        &json!({ "mode": "cycles" }),
        None,
        1,
    )
    .await;
    assert!(res_cycles.success);
    assert!(
        res_cycles.output.contains("Zero Circular Cycles")
            || res_cycles.output.contains("acyclic DAG")
    );

    // Test format: "json"
    let res_json = ToolRegistry::dispatch(
        root,
        "call_audit_4",
        "audit_architecture",
        &json!({ "mode": "full", "format": "json" }),
        None,
        1,
    )
    .await;
    assert!(res_json.success);
    let parsed: serde_json::Value =
        serde_json::from_str(&res_json.output).expect("parse report json");
    assert!(parsed.get("health_score").is_some());
    assert_eq!(parsed["total_files"].as_u64(), Some(2));
}

#[tokio::test]
async fn test_audit_architecture_tool_enforce_flag() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();

    let src = root.join("src");
    let core = src.join("core");
    let ui = src.join("ui");

    fs::create_dir_all(&core).expect("create core dir");
    fs::create_dir_all(&ui).expect("create ui dir");

    // Introduce violation: core imports ui
    fs::write(
        core.join("engine.rs"),
        "use crate::ui::view;\npub struct Engine;\n",
    )
    .expect("write engine.rs");
    fs::write(ui.join("view.rs"), "pub fn draw() {}\n").expect("write view.rs");

    // Without enforce: succeeds and returns report
    let res_no_enforce = ToolRegistry::dispatch(
        root,
        "call_audit_5",
        "audit_architecture",
        &json!({ "enforce": false }),
        None,
        1,
    )
    .await;
    assert!(res_no_enforce.success);

    // With enforce: fails because score is degraded and violations exist
    let res_enforce = ToolRegistry::dispatch(
        root,
        "call_audit_6",
        "audit_architecture",
        &json!({ "enforce": true }),
        None,
        1,
    )
    .await;
    assert!(!res_enforce.success);
    assert!(res_enforce
        .output
        .contains("Architectural Enforcement Failed"));
}

#[test]
fn test_audit_architecture_js_ts_and_python_governance() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();

    let src = root.join("src");
    let utils = src.join("utils");
    let presentation = src.join("presentation");
    let data = src.join("data");
    let services = src.join("services");

    fs::create_dir_all(&utils).expect("create utils dir");
    fs::create_dir_all(&presentation).expect("create presentation dir");
    fs::create_dir_all(&data).expect("create data dir");
    fs::create_dir_all(&services).expect("create services dir");

    // TS: utility imports presentation (violates R3_UTILITY_PURITY)
    fs::write(
        utils.join("helper.ts"),
        "import { Banner } from '../presentation/banner';\nexport const format = () => {};\n",
    )
    .expect("write helper.ts");

    fs::write(
        presentation.join("banner.ts"),
        "export const Banner = () => {};\n",
    )
    .expect("write banner.ts");

    // Python: data imports services (violates R2_DATA_NO_SERVICE)
    fs::write(
        data.join("models.py"),
        "from src.services.auth import verify_token\nclass User:\n    pass\n",
    )
    .expect("write models.py");

    fs::write(
        services.join("auth.py"),
        "def verify_token():\n    return True\n",
    )
    .expect("write auth.py");

    let report = ArchitectureGovernor::scan_workspace(root).expect("scan workspace");

    assert_eq!(report.total_files, 4);
    assert!(
        report.layer_violations.len() >= 2,
        "Expected at least 2 violations across TS and Python, got: {:?}",
        report.layer_violations
    );

    let ts_violation = report
        .layer_violations
        .iter()
        .find(|v| v.rule_id == "R3_UTILITY_PURITY");
    assert!(
        ts_violation.is_some(),
        "Expected R3_UTILITY_PURITY violation in helper.ts"
    );

    let py_violation = report
        .layer_violations
        .iter()
        .find(|v| v.rule_id == "R2_DATA_NO_SERVICE");
    assert!(
        py_violation.is_some(),
        "Expected R2_DATA_NO_SERVICE violation in models.py"
    );
}
