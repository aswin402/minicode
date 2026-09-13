use minicode::agent::stuck_detector::{BreakerAction, StuckDetector};
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_exec_cmd_nonzero_exit_returns_success_false() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    let res = ToolRegistry::dispatch(
        ws,
        "call_nonzero",
        "exec_cmd",
        &json!({
            "command": "sh -c 'echo \"Fatal job error\"; exit 2'"
        }),
        None,
        1,
    )
    .await;

    assert!(
        !res.success,
        "ToolResult.success must be false on non-zero exit: {}",
        res.output
    );
    assert!(
        res.output.contains("non-zero status (2)"),
        "Output must reflect exit code: {}",
        res.output
    );
    assert!(
        res.output.contains("Fatal job error"),
        "Output must include command stdout/stderr: {}",
        res.output
    );
}

#[tokio::test]
async fn test_exec_cmd_compiler_error_injects_fault_localization_hint() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    let res = ToolRegistry::dispatch(
        ws,
        "call_trace",
        "exec_cmd",
        &json!({
            "command": "sh -c 'echo \"error[E0425]: cannot find value foo in this scope\n  --> src/agent/loop.rs:105:9\"; exit 101'"
        }),
        None,
        1,
    )
    .await;

    assert!(!res.success, "Must fail with non-zero exit");
    assert!(
        res.output.contains("Probable Fault Sites:"),
        "Output must include FaultLocalizer header: {}",
        res.output
    );
    assert!(
        res.output.contains("src/agent/loop.rs:105"),
        "Output must include exact file and line: {}",
        res.output
    );
    assert!(
        res.output.contains("read_file"),
        "Output must include suggested action: {}",
        res.output
    );
}

#[tokio::test]
async fn test_repair_patch_rollback_returns_success_false() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    let target_file = ws.join("service.rs");
    let initial_code = "pub fn serve() -> bool {\n    true\n}\n";
    fs::write(&target_file, initial_code).expect("write service.rs");

    // Execute repair_patch with a verification command that always fails
    let res = ToolRegistry::dispatch(
        ws,
        "call_repair_fail",
        "repair_patch",
        &json!({
            "path": "service.rs",
            "search_block": "    true",
            "replace_block": "    false",
            "verification_cmd": "sh -c 'exit 1'"
        }),
        None,
        1,
    )
    .await;

    assert!(
        !res.success,
        "repair_patch must return success: false when verification fails and changes roll back"
    );
    assert!(
        res.output.contains("rolled back"),
        "Output must inform the agent of automatic rollback: {}",
        res.output
    );

    // Verify file contents were restored on disk
    let restored = fs::read_to_string(&target_file).expect("read service.rs");
    assert_eq!(
        restored, initial_code,
        "Disk file must remain identical to initial code after rollback"
    );
}

#[test]
fn test_stuck_detector_registers_consecutive_exec_cmd_failures() {
    let mut detector = StuckDetector::new();
    let args = json!({"command": "cargo test -j 1"});

    // 1st failure: Pass
    let a1 = detector.check("exec_cmd", &args, false);
    assert_eq!(a1, BreakerAction::Pass);

    // 2nd failure: Consecutive failure threshold reached -> Warning
    let a2 = detector.check("exec_cmd", &args, false);
    assert!(
        a2.is_warning(),
        "Second identical failed command must trigger anti-thrash warning"
    );

    // 3rd failure: Warning ignored -> Hard trip
    let a3 = detector.check("exec_cmd", &args, false);
    assert!(
        a3.is_trip(),
        "Ignoring warning on third consecutive failure must trip circuit breaker"
    );
}

#[test]
fn test_stuck_detector_file_target_thrashing_with_path_aliases() {
    let mut detector = StuckDetector::new();

    // 1st failure on "src/app.rs" using 'file_path'
    let a1 = detector.check(
        "patch_file",
        &json!({"file_path": "src/app.rs", "search": "a", "replace": "b"}),
        false,
    );
    assert_eq!(a1, BreakerAction::Pass);

    // 2nd failure on "src/app.rs" using 'target_file'
    let a2 = detector.check(
        "patch_file",
        &json!({"target_file": "src/app.rs", "search": "c", "replace": "d"}),
        false,
    );
    assert_eq!(a2, BreakerAction::Pass);

    // 3rd failure on "src/app.rs" using 'path' -> file_failure_threshold reached (3)
    let a3 = detector.check(
        "patch_file",
        &json!({"path": "src/app.rs", "search": "e", "replace": "f"}),
        false,
    );
    assert!(
        a3.is_warning(),
        "Third consecutive failure across alias names on same file must trigger FileTargetThrashing warning"
    );
    let warning = a3.intervention().unwrap_or_default();
    assert!(warning.contains("FILE TARGET THRASHING"));
    assert!(warning.contains("src/app.rs"));
}
