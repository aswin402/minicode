use minicode::agent::reproducer_guard::{
    ReproducerGuard, ReproducerPhase, ReproducerRecord, ReproducerReport,
};
use minicode::agent::verification_barrier::{GateStatus, VerificationBarrier};
use minicode::tools::ToolRegistry;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_target_name_normalization() {
    assert_eq!(
        ReproducerGuard::normalize_target_name("parser_bounds"),
        "repro_parser_bounds"
    );
    assert_eq!(
        ReproducerGuard::normalize_target_name("repro_parser_bounds"),
        "repro_parser_bounds"
    );
    assert_eq!(
        ReproducerGuard::normalize_target_name("my-bug-test.rs"),
        "repro_my_bug_test"
    );
    assert_eq!(
        ReproducerGuard::normalize_target_name("   repro_empty.rs  "),
        "repro_empty"
    );
}

#[test]
fn test_reproducer_persistence_and_formatting() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let record1 = ReproducerRecord {
        name: "repro_test_failure".to_string(),
        file_path: "tests/repro_test_failure.rs".to_string(),
        description: "Null dereference on empty input".to_string(),
        created_at: 1000,
        status: ReproducerPhase::RedConfirmed {
            exit_code: 101,
            failure_snippet: "panicked at 'assertion failed'".to_string(),
            timestamp: 1000,
        },
    };

    let record2 = ReproducerRecord {
        name: "repro_test_verified".to_string(),
        file_path: "tests/repro_test_verified.rs".to_string(),
        description: "Buffer overflow fix".to_string(),
        created_at: 2000,
        status: ReproducerPhase::GreenVerified { timestamp: 2000 },
    };

    let record3 = ReproducerRecord {
        name: "repro_test_vacuous".to_string(),
        file_path: "tests/repro_test_vacuous.rs".to_string(),
        description: "Vacuous test assertion".to_string(),
        created_at: 1500,
        status: ReproducerPhase::VacuousWarning { timestamp: 1500 },
    };

    // Save records
    ReproducerGuard::save_record(root, &record1).unwrap();
    ReproducerGuard::save_record(root, &record2).unwrap();
    ReproducerGuard::save_record(root, &record3).unwrap();

    // Load individual record
    let loaded = ReproducerGuard::load_record(root, "test_failure").unwrap();
    assert_eq!(loaded.name, "repro_test_failure");
    assert_eq!(loaded.description, "Null dereference on empty input");

    // List all records
    let records = ReproducerGuard::list_active_reproducers(root);
    assert_eq!(records.len(), 3);
    // Verified sorted by created_at desc: record2 (2000), record3 (1500), record1 (1000)
    assert_eq!(records[0].name, "repro_test_verified");
    assert_eq!(records[1].name, "repro_test_vacuous");
    assert_eq!(records[2].name, "repro_test_failure");

    // Format markdown table
    let table = ReproducerGuard::format_reproducer_list(&records);
    assert!(table.contains("Active TDD Bug Reproducers (3 registered)"));
    assert!(table.contains("🔴 **RED CONFIRMED**"));
    assert!(table.contains("🟢 **GREEN VERIFIED**"));
    assert!(table.contains("🟡 **VACUOUS WARNING**"));
}

#[test]
fn test_reproducer_report_messages() {
    let red = ReproducerReport::RedConfirmed {
        name: "repro_calc".to_string(),
        file_path: "tests/repro_calc.rs".to_string(),
        exit_code: 101,
        failure_snippet: "assertion `left == right` failed\n  left: 4\n right: 5".to_string(),
    };
    let red_msg = red.format_message();
    assert!(red_msg.contains("[RED PHASE CONFIRMED]"));
    assert!(red_msg.contains("exit code: 101"));
    assert!(red_msg.contains("left == right"));

    let vacuous = ReproducerReport::VacuousWarning {
        name: "repro_calc".to_string(),
        file_path: "tests/repro_calc.rs".to_string(),
    };
    let vac_msg = vacuous.format_message();
    assert!(vac_msg.contains("[VACUOUS REPRODUCER WARNING]"));
    assert!(vac_msg.contains("PASSED (exit code 0) on the current unpatched codebase"));

    let green = ReproducerReport::GreenVerified {
        name: "repro_calc".to_string(),
        file_path: "tests/repro_calc.rs".to_string(),
    };
    let green_msg = green.format_message();
    assert!(green_msg.contains("[GREEN PHASE VERIFIED]"));
    assert!(green_msg.contains("Red-to-Green transition is mathematically confirmed"));

    let compile_err = ReproducerReport::CompilationError {
        name: "repro_broken".to_string(),
        file_path: "tests/repro_broken.rs".to_string(),
        diagnostic: "error[E0425]: cannot find value `xyz` in this scope".to_string(),
    };
    let compile_msg = compile_err.format_message();
    assert!(compile_msg.contains("[REPRODUCER COMPILATION ERROR]"));
    assert!(compile_msg.contains("cannot find value `xyz`"));
}

#[tokio::test]
async fn test_reproducer_synthesis_written_only() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let test_code = r#"
#[test]
fn test_dummy() {
    assert_eq!(1, 1);
}
"#;

    let report = ReproducerGuard::synthesize_rust_reproducer(
        root,
        "dummy_test",
        test_code,
        "Simple test description",
        false, // do not execute red phase
    )
    .unwrap();

    match report {
        ReproducerReport::WrittenOnly { name, file_path } => {
            assert_eq!(name, "repro_dummy_test");
            assert_eq!(file_path, "tests/repro_dummy_test.rs");
            assert!(root.join("tests/repro_dummy_test.rs").exists());
        }
        _ => panic!("Expected WrittenOnly report"),
    }
}

#[tokio::test]
async fn test_reproducer_tool_dispatch() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // 1. List reproducers when empty
    let list_res =
        ToolRegistry::dispatch(root, "call_1", "list_reproducers", &json!({}), None, 1).await;
    assert!(list_res.success);
    assert!(list_res.output.contains("No active reproducer tests"));

    // 2. Synthesize reproducer with run_red_phase: false
    let synth_res = ToolRegistry::dispatch(
        root,
        "call_2",
        "synthesize_reproducer",
        &json!({
            "name": "calc_overflow",
            "test_code": "#[test] fn test_overflow() { assert_eq!(1 + 1, 2); }",
            "description": "Checks overflow behavior",
            "run_red_phase": false
        }),
        None,
        1,
    )
    .await;
    assert!(synth_res.success);
    assert!(synth_res
        .output
        .contains("Successfully wrote reproducer test to 'tests/repro_calc_overflow.rs'"));
    assert!(root.join("tests/repro_calc_overflow.rs").exists());

    // 3. List reproducers after synthesis
    let list_res2 =
        ToolRegistry::dispatch(root, "call_3", "list_reproducers", &json!({}), None, 1).await;
    assert!(list_res2.success);
    assert!(list_res2.output.contains("repro_calc_overflow"));
}

#[tokio::test]
async fn test_gate2_active_reproducer_blocks_completion_until_green() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // Create a pending RedConfirmed reproducer record
    let record = ReproducerRecord {
        name: "repro_mock_pending".to_string(),
        file_path: "tests/repro_mock_pending.rs".to_string(),
        description: "Mock pending bug reproducer".to_string(),
        created_at: 1000,
        status: ReproducerPhase::RedConfirmed {
            exit_code: 101,
            failure_snippet: "panicked at 'assertion failed'".to_string(),
            timestamp: 1000,
        },
    };
    ReproducerGuard::save_record(root, &record).unwrap();

    // Even if no tests were modified in this turn (only src/logic.rs modified),
    // Gate 2 recognizes that there is an active RedConfirmed reproducer and targets it!
    let status =
        VerificationBarrier::check_gate2_reproducer_test(root, &["src/logic.rs".to_string()]);

    // Since tests/repro_mock_pending.rs doesn't exist on disk, Gate 2 handles the target
    // and returns either Passed or Failed without crashing or blindly skipping.
    // Specifically, it does NOT skip with "No reproducer script".
    match status {
        GateStatus::Skipped { reason } => {
            panic!(
                "Gate 2 should NOT skip when an active RedConfirmed reproducer exists: {}",
                reason
            );
        }
        GateStatus::Passed | GateStatus::Failed { .. } => {
            // Expected: Gate 2 attempted verification of the pending reproducer!
        }
    }
}
