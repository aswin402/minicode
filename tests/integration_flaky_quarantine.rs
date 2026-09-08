use minicode::context::flaky::{
    FlakinessVerdict, FlakySignature, FlakyTestDetector, QuarantineManager, SingleTestRun,
};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_flaky_variance_statistical_analysis() {
    // 1. All passes -> DeterministicPass
    let pass_runs = vec![
        SingleTestRun {
            run_index: 1,
            passed: true,
            duration_ms: 120,
            error_snippet: None,
        },
        SingleTestRun {
            run_index: 2,
            passed: true,
            duration_ms: 110,
            error_snippet: None,
        },
        SingleTestRun {
            run_index: 3,
            passed: true,
            duration_ms: 115,
            error_snippet: None,
        },
    ];
    let pass_report = FlakyTestDetector::analyze_runs("tests::test_stable", pass_runs);
    assert_eq!(pass_report.verdict, FlakinessVerdict::DeterministicPass);
    assert_eq!(pass_report.flakiness_ratio, 0.0);
    assert_eq!(pass_report.passes, 3);
    assert_eq!(pass_report.failures, 0);

    // 2. All failures -> DeterministicFail
    let fail_runs = vec![
        SingleTestRun {
            run_index: 1,
            passed: false,
            duration_ms: 50,
            error_snippet: Some("assertion `left == right` failed".to_string()),
        },
        SingleTestRun {
            run_index: 2,
            passed: false,
            duration_ms: 55,
            error_snippet: Some("assertion `left == right` failed".to_string()),
        },
    ];
    let fail_report = FlakyTestDetector::analyze_runs("tests::test_broken", fail_runs);
    assert_eq!(fail_report.verdict, FlakinessVerdict::DeterministicFail);
    assert_eq!(fail_report.flakiness_ratio, 0.0);
    assert_eq!(fail_report.failures, 2);

    // 3. Intermittent mixed runs -> FlakyIntermittent
    let mixed_runs = vec![
        SingleTestRun {
            run_index: 1,
            passed: true,
            duration_ms: 100,
            error_snippet: None,
        },
        SingleTestRun {
            run_index: 2,
            passed: false,
            duration_ms: 250,
            error_snippet: Some("test timed out after 10 seconds".to_string()),
        },
        SingleTestRun {
            run_index: 3,
            passed: true,
            duration_ms: 105,
            error_snippet: None,
        },
        SingleTestRun {
            run_index: 4,
            passed: false,
            duration_ms: 280,
            error_snippet: Some("test timed out after 10 seconds".to_string()),
        },
    ];
    let flaky_report = FlakyTestDetector::analyze_runs("tests::test_flaky_socket", mixed_runs);
    assert_eq!(flaky_report.verdict, FlakinessVerdict::FlakyIntermittent);
    assert_eq!(flaky_report.flakiness_ratio, 0.5);
    assert_eq!(flaky_report.signature, FlakySignature::TimingJitter);
    assert!(!flaky_report.stabilization_advice.is_empty());

    let md = flaky_report.format_markdown();
    assert!(md.contains("Flaky Intermittent"));
    assert!(md.contains("Timing Jitter"));
    assert!(md.contains("Recommended Stabilization Actions"));
}

#[test]
fn test_flaky_signature_error_classification() {
    assert_eq!(
        FlakySignature::from_output("panicked at 'assertion `left == right` failed'"),
        FlakySignature::AssertionVariance
    );
    assert_eq!(
        FlakySignature::from_output("operation timed out elapsed deadline"),
        FlakySignature::TimingJitter
    );
    assert_eq!(
        FlakySignature::from_output("address already in use (os error 98)"),
        FlakySignature::ResourceContention
    );
    assert_eq!(
        FlakySignature::from_output("random unexpected panic"),
        FlakySignature::Unknown
    );
}

#[test]
fn test_quarantine_store_crud_and_persistence() {
    let dir = tempdir().expect("tempdir");
    let workspace = dir.path();

    // Initially empty
    assert!(!QuarantineManager::is_quarantined(workspace, "my_test"));
    let initial_store = QuarantineManager::load(workspace);
    assert!(initial_store.tests.is_empty());

    let empty_report = QuarantineManager::format_report(&initial_store);
    assert!(empty_report.contains("No tests are currently quarantined"));

    // Quarantine a test
    let entry = QuarantineManager::quarantine(
        workspace,
        "integration::test_network_sync",
        0.4,
        FlakySignature::TimingJitter,
        "Intermittent timeout on busy CI runners",
        5,
    )
    .expect("quarantine success");

    assert_eq!(entry.test_name, "integration::test_network_sync");
    assert_eq!(entry.runs_evaluated, 5);
    assert!(QuarantineManager::is_quarantined(
        workspace,
        "integration::test_network_sync"
    ));

    // Formatted report displays quarantined test
    let loaded_store = QuarantineManager::load(workspace);
    assert_eq!(loaded_store.tests.len(), 1);
    let report = QuarantineManager::format_report(&loaded_store);
    assert!(report.contains("integration::test_network_sync"));
    assert!(report.contains("Timing Jitter"));
    assert!(report.contains("40.0%"));

    // Unquarantine test
    let removed = QuarantineManager::unquarantine(workspace, "integration::test_network_sync")
        .expect("unquarantine success");
    assert!(removed);
    assert!(!QuarantineManager::is_quarantined(
        workspace,
        "integration::test_network_sync"
    ));

    // Clear test store
    QuarantineManager::quarantine(
        workspace,
        "test_a",
        0.5,
        FlakySignature::ResourceContention,
        "port clash",
        4,
    )
    .expect("quarantine a");
    QuarantineManager::quarantine(
        workspace,
        "test_b",
        0.25,
        FlakySignature::AssertionVariance,
        "hashmap order",
        4,
    )
    .expect("quarantine b");

    assert_eq!(QuarantineManager::load(workspace).tests.len(), 2);
    let cleared = QuarantineManager::clear(workspace).expect("clear success");
    assert_eq!(cleared, 2);
    assert_eq!(QuarantineManager::load(workspace).tests.len(), 0);
}

#[tokio::test]
async fn test_tool_registry_quarantine_flaky_tests_dispatch() {
    let dir = tempdir().expect("tempdir");
    let workspace = dir.path();

    // 1. Initial list
    let list_args = json!({
        "action": "list"
    });
    let list_res = minicode::tools::registry::context_tools::dispatch(
        "quarantine_flaky_tests",
        &list_args,
        workspace,
    )
    .await
    .expect("tool registered")
    .expect("dispatch success");
    assert!(list_res.contains("No tests are currently quarantined"));

    // 2. Manual quarantine action
    let add_args = json!({
        "action": "quarantine",
        "test_name": "tests::test_distributed_lock",
        "reason": "Occasional lock acquisition collision under high thread load"
    });
    let add_res = minicode::tools::registry::context_tools::dispatch(
        "quarantine_flaky_tests",
        &add_args,
        workspace,
    )
    .await
    .expect("tool registered")
    .expect("dispatch success");
    assert!(add_res.contains("Successfully quarantined test `tests::test_distributed_lock`"));

    // 3. List should now include the test
    let list_res2 = minicode::tools::registry::context_tools::dispatch(
        "quarantine_flaky_tests",
        &list_args,
        workspace,
    )
    .await
    .expect("tool registered")
    .expect("dispatch success");
    assert!(list_res2.contains("tests::test_distributed_lock"));
    assert!(list_res2.contains("Quarantined Test Registry"));

    // 4. Unquarantine action
    let unq_args = json!({
        "action": "unquarantine",
        "test_name": "tests::test_distributed_lock"
    });
    let unq_res = minicode::tools::registry::context_tools::dispatch(
        "quarantine_flaky_tests",
        &unq_args,
        workspace,
    )
    .await
    .expect("tool registered")
    .expect("dispatch success");
    assert!(unq_res.contains("Successfully un-quarantined test `tests::test_distributed_lock`"));

    // 5. Argument validation
    let bad_args = json!({
        "action": "quarantine",
        "test_name": ""
    });
    let err_res = minicode::tools::registry::context_tools::dispatch(
        "quarantine_flaky_tests",
        &bad_args,
        workspace,
    )
    .await
    .expect("tool registered");
    assert!(err_res.is_err());
}
