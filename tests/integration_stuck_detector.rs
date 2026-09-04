use minicode::agent::stuck_detector::StuckDetector;
use serde_json::json;

#[test]
fn test_canonical_json_nested_sorting_and_hashing() {
    let obj1 = json!({
        "path": "src/lib.rs",
        "nested": {
            "z": 100,
            "a": 200,
            "m": [1, 2, 3]
        },
        "flags": ["debug", "release"]
    });

    let obj2 = json!({
        "flags": ["debug", "release"],
        "nested": {
            "a": 200,
            "m": [1, 2, 3],
            "z": 100
        },
        "path": "src/lib.rs"
    });

    assert_eq!(
        StuckDetector::canonicalize_json(&obj1),
        StuckDetector::canonicalize_json(&obj2)
    );
    assert_eq!(
        StuckDetector::compute_args_hash(&obj1),
        StuckDetector::compute_args_hash(&obj2)
    );
}

#[test]
fn test_consecutive_failure_triggers_circuit_breaker() {
    let mut detector = StuckDetector::new();
    let args = json!({"path": "src/auth.rs", "search_block": "let x = 1;"});

    // 1st failure: no circuit breaker
    let res1 = detector.record_and_check("patch_file", &args, false);
    assert!(res1.is_none());

    // 2nd consecutive failure: triggers immediately!
    let res2 = detector.record_and_check("patch_file", &args, false);
    assert!(res2.is_some());
    let intervention = res2.unwrap();
    assert!(intervention.contains("[CIRCUIT BREAKER TRIGGERED: CONSECUTIVE REPETITION]"));
    assert!(intervention.contains("patch_file"));
    assert!(intervention.contains("failing consecutively"));
    assert!(intervention.contains("STOP repeating this action immediately"));
}

#[test]
fn test_consecutive_success_triggers_at_threshold() {
    let mut detector = StuckDetector::new();
    let args = json!({"path": "src/router.rs", "start_line": 1, "end_line": 50});

    assert!(detector
        .record_and_check("read_file", &args, true)
        .is_none());
    assert!(detector
        .record_and_check("read_file", &args, true)
        .is_none());

    // 3rd consecutive execution with identical args triggers circuit breaker
    let res3 = detector.record_and_check("read_file", &args, true);
    assert!(res3.is_some());
    let msg = res3.unwrap();
    assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: CONSECUTIVE REPETITION]"));
    assert!(msg.contains("read_file"));
    assert!(msg.contains("3 times"));
}

#[test]
fn test_ping_pong_oscillation_break() {
    let mut detector = StuckDetector::new();
    let args_read = json!({"path": "src/main.rs"});
    let args_grep = json!({"query": "Config"});

    // Cycle 1: read_file -> grep_search
    assert!(detector
        .record_and_check("read_file", &args_read, true)
        .is_none());
    assert!(detector
        .record_and_check("grep_search", &args_grep, true)
        .is_none());

    // Cycle 2: read_file -> grep_search
    assert!(detector
        .record_and_check("read_file", &args_read, true)
        .is_none());
    let res = detector.record_and_check("grep_search", &args_grep, true);

    assert!(res.is_some());
    let msg = res.unwrap();
    assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: PING-PONG OSCILLATION]"));
    assert!(msg.contains("read_file"));
    assert!(msg.contains("grep_search"));
    assert!(msg.contains("2 complete cycles"));
}

#[test]
fn test_triangular_oscillation_break() {
    let mut detector = StuckDetector::new();
    let args1 = json!({"cmd": "cargo check"});
    let args2 = json!({"path": "src/main.rs"});
    let args3 = json!({"symbol": "main"});

    // Cycle 1: A -> B -> C
    assert!(detector
        .record_and_check("exec_cmd", &args1, true)
        .is_none());
    assert!(detector
        .record_and_check("read_file", &args2, true)
        .is_none());
    assert!(detector
        .record_and_check("locate_symbol", &args3, true)
        .is_none());

    // Cycle 2: A -> B -> C
    assert!(detector
        .record_and_check("exec_cmd", &args1, true)
        .is_none());
    assert!(detector
        .record_and_check("read_file", &args2, true)
        .is_none());
    let res = detector.record_and_check("locate_symbol", &args3, true);

    assert!(res.is_some());
    let msg = res.unwrap();
    assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: CYCLIC OSCILLATION]"));
    assert!(msg.contains("exec_cmd -> read_file -> locate_symbol"));
}

#[test]
fn test_stuck_detector_reset_on_turn_boundary() {
    let mut detector = StuckDetector::new();
    let args = json!({"path": "src/main.rs"});

    // 2 consecutive calls
    detector.record_and_check("read_file", &args, true);
    detector.record_and_check("read_file", &args, true);

    // Reset occurs at user turn boundary
    detector.reset();

    // 1st call of next turn should NOT trigger circuit breaker
    let res = detector.record_and_check("read_file", &args, true);
    assert!(res.is_none());
}
