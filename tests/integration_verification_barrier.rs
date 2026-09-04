use minicode::agent::verification_barrier::{GateStatus, VerificationBarrier};
use tempfile::tempdir;

#[tokio::test]
async fn test_gate1_syntax_validation() {
    let temp = tempdir().unwrap();
    let file_clean = temp.path().join("clean.rs");
    std::fs::write(&file_clean, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

    let file_broken = temp.path().join("broken.rs");
    std::fs::write(
        &file_broken,
        "pub fn broken(a: i32, b: i32) -> i32 { a + \n",
    )
    .unwrap();

    // Clean file passes Gate 1
    let status_clean =
        VerificationBarrier::check_gate1_syntax_compiler(temp.path(), &["clean.rs".to_string()]);
    assert_eq!(status_clean, GateStatus::Passed);

    // Broken file fails Gate 1
    let status_broken =
        VerificationBarrier::check_gate1_syntax_compiler(temp.path(), &["broken.rs".to_string()]);
    match status_broken {
        GateStatus::Failed {
            gate_name, reason, ..
        } => {
            assert!(gate_name.contains("Gate 1"));
            assert!(reason.contains("AST syntax error"));
        }
        _ => panic!("Expected Gate 1 to fail on broken syntax"),
    }
}

#[tokio::test]
async fn test_gate2_skipped_when_no_tests_modified() {
    let temp = tempdir().unwrap();
    let status = VerificationBarrier::check_gate2_reproducer_test(
        temp.path(),
        &["src/service.rs".to_string(), "Cargo.toml".to_string()],
    );

    match status {
        GateStatus::Skipped { reason } => {
            assert!(reason.contains("No reproducer"));
        }
        _ => panic!("Expected Gate 2 to be skipped when no tests are modified"),
    }
}

#[tokio::test]
async fn test_gate3_conflict_markers_rejection() {
    let temp = tempdir().unwrap();
    let file = temp.path().join("merge_conflict.rs");
    std::fs::write(
        &file,
        "pub fn calculate() -> usize {\n<<<<<<< HEAD\n    10\n=======\n    20\n>>>>>>> branch\n}\n",
    )
    .unwrap();

    let status = VerificationBarrier::check_gate3_regression_conflicts(
        temp.path(),
        &["merge_conflict.rs".to_string()],
    );

    match status {
        GateStatus::Failed {
            gate_name, reason, ..
        } => {
            assert!(gate_name.contains("Gate 3"));
            assert!(reason.contains("conflict marker"));
            assert!(reason.contains("<<<<<<<"));
        }
        _ => panic!("Expected Gate 3 to fail on conflict markers"),
    }
}

#[tokio::test]
async fn test_gate4_debug_statement_detection_in_production_code() {
    let temp = tempdir().unwrap();
    let src_dir = temp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let prod_file = src_dir.join("worker.rs");
    std::fs::write(
        &prod_file,
        "pub fn run() {\n    // This is fine: // println!(\"commented\");\n    println!(\"raw debug log\");\n}\n",
    )
    .unwrap();

    let status =
        VerificationBarrier::check_gate4_diff_sanity(temp.path(), &["src/worker.rs".to_string()]);

    match status {
        GateStatus::Failed {
            gate_name, reason, ..
        } => {
            assert!(gate_name.contains("Gate 4"));
            assert!(reason.contains("println!"));
        }
        _ => panic!("Expected Gate 4 to reject println! in production src/ code"),
    }
}

#[tokio::test]
async fn test_gate4_debug_statement_permitted_in_test_files() {
    let temp = tempdir().unwrap();
    let tests_dir = temp.path().join("tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    let test_file = tests_dir.join("integration_test.rs");
    std::fs::write(
        &test_file,
        "#[test]\nfn test_foo() {\n    println!(\"debug in test is permitted\");\n}\n",
    )
    .unwrap();

    let status = VerificationBarrier::check_gate4_diff_sanity(
        temp.path(),
        &["tests/integration_test.rs".to_string()],
    );

    assert_eq!(status, GateStatus::Passed);
}

#[tokio::test]
async fn test_gate4_secret_leak_rejection() {
    let temp = tempdir().unwrap();
    let src_dir = temp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let prod_file = src_dir.join("auth.rs");
    std::fs::write(
        &prod_file,
        "pub const API_KEY: &str = \"sk-proj-123456789012345678901234567890\";\n",
    )
    .unwrap();

    let status =
        VerificationBarrier::check_gate4_diff_sanity(temp.path(), &["src/auth.rs".to_string()]);

    match status {
        GateStatus::Failed {
            gate_name, reason, ..
        } => {
            assert!(gate_name.contains("Gate 4"));
            assert!(reason.contains("secret or API key"));
        }
        _ => panic!("Expected Gate 4 to reject hardcoded secret"),
    }
}

#[tokio::test]
async fn test_verification_barrier_overall_evaluation() {
    let temp = tempdir().unwrap();
    let src_dir = temp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_clean = src_dir.join("module.rs");
    std::fs::write(&file_clean, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

    let report = VerificationBarrier::verify(temp.path(), &["src/module.rs".to_string()]).await;

    assert!(report.all_passed);
    assert_eq!(report.gate1_syntax_compiler, GateStatus::Passed);
    assert!(matches!(
        report.gate2_reproducer_test,
        GateStatus::Skipped { .. }
    ));
    assert_eq!(report.gate3_regression_conflicts, GateStatus::Passed);
    assert_eq!(report.gate4_diff_sanity, GateStatus::Passed);
}
