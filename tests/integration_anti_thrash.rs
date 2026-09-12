//! Integration tests for Anti-Thrashing Circuit Breaker & Runaway Loop Guard (Phase 112).

mod common;

use common::{MockProvider, MockResponse};
use minicode::agent::stuck_detector::{BreakerAction, StuckDetector};
use minicode::agent::types::AgentEvent;
use minicode::agent::AgentLoop;
use minicode::config::Config;
use serde_json::json;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[test]
fn test_file_target_thrashing_and_hard_trip() {
    let mut detector = StuckDetector::new();
    let file = "src/payment/processor.rs";

    // Consecutive failures on the same file with different arguments
    let step1 = detector.check("patch_file", &json!({"path": file, "diff": "try 1"}), false);
    assert_eq!(step1, BreakerAction::Pass);

    let step2 = detector.check(
        "apply_diff",
        &json!({"target_file": file, "diff": "try 2"}),
        false,
    );
    assert_eq!(step2, BreakerAction::Pass);

    // 3rd failure on same file: Triggers warning
    let step3 = detector.check(
        "edit_block",
        &json!({"file_path": file, "diff": "try 3"}),
        false,
    );
    assert!(step3.is_warning());
    let warning_msg = step3.intervention().unwrap_or_default();
    assert!(warning_msg.contains("[CIRCUIT BREAKER TRIGGERED: FILE TARGET THRASHING]"));
    assert!(warning_msg.contains(file));
    assert!(!detector.is_tripped());

    // 4th failure: warnings ignored -> Hard trip!
    let step4 = detector.check("patch_file", &json!({"path": file, "diff": "try 4"}), false);
    assert!(step4.is_trip());
    assert!(detector.is_tripped());
    let trip_msg = step4.intervention().unwrap_or_default();
    assert!(trip_msg.contains("[CIRCUIT BREAKER HARD TRIP: RUNAWAY LOOP HALTED]"));
    assert!(trip_msg.contains(file));
}

#[test]
fn test_execution_collapse_and_hard_trip() {
    let mut detector = StuckDetector::new();

    // 4 failures across different tools
    assert_eq!(
        detector.check("exec_cmd", &json!({"command": "cargo build"}), false),
        BreakerAction::Pass
    );
    assert_eq!(
        detector.check("locate_symbol", &json!({"name": "UserAuth"}), false),
        BreakerAction::Pass
    );
    assert_eq!(
        detector.check("read_file", &json!({"path": "missing.json"}), false),
        BreakerAction::Pass
    );

    // 4th failure -> Execution collapse warning
    let action = detector.check(
        "fetch_or_browse",
        &json!({"url": "http://offline.local"}),
        false,
    );
    assert!(action.is_warning());
    let msg = action.intervention().unwrap_or_default();
    assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: EXECUTION COLLAPSE]"));
    assert!(msg.contains("4 tool operations have ALL failed consecutively"));
    assert!(!detector.is_tripped());

    // 5th failure -> Trips hard
    let trip_action = detector.check("exec_cmd", &json!({"command": "pytest"}), false);
    assert!(trip_action.is_trip());
    assert!(detector.is_tripped());
}

#[test]
fn test_consecutive_repetition_detection() {
    let mut detector = StuckDetector::new();
    let args = json!({"path": "src/main.rs", "line": 42});

    // 1st failure: pass
    assert_eq!(
        detector.check("read_file", &args, false),
        BreakerAction::Pass
    );

    // 2nd identical failure: warning
    let act2 = detector.check("read_file", &args, false);
    assert!(act2.is_warning());
    let warn = act2.intervention().unwrap_or_default();
    assert!(warn.contains("failing consecutively"));

    // 3rd identical failure: trip
    let act3 = detector.check("read_file", &args, false);
    assert!(act3.is_trip());
    assert!(detector.is_tripped());
}

#[test]
fn test_oscillation_patterns() {
    let mut detector = StuckDetector::new();

    // Ping-pong A-B-A-B
    let a = json!({"path": "a.txt"});
    let b = json!({"path": "b.txt"});

    assert_eq!(detector.check("read_file", &a, true), BreakerAction::Pass);
    assert_eq!(detector.check("read_file", &b, true), BreakerAction::Pass);
    assert_eq!(detector.check("read_file", &a, true), BreakerAction::Pass);

    let pp_action = detector.check("read_file", &b, true);
    assert!(pp_action.is_warning());
    let warn = pp_action.intervention().unwrap_or_default();
    assert!(warn.contains("PING-PONG OSCILLATION"));

    // Reset clears state
    detector.reset();
    assert!(!detector.is_tripped());

    // Triangular A-B-C-A-B-C
    let c = json!({"path": "c.txt"});
    assert_eq!(detector.check("tool_1", &a, true), BreakerAction::Pass);
    assert_eq!(detector.check("tool_2", &b, true), BreakerAction::Pass);
    assert_eq!(detector.check("tool_3", &c, true), BreakerAction::Pass);
    assert_eq!(detector.check("tool_1", &a, true), BreakerAction::Pass);
    assert_eq!(detector.check("tool_2", &b, true), BreakerAction::Pass);

    let tri_action = detector.check("tool_3", &c, true);
    assert!(tri_action.is_warning());
    let tri_warn = tri_action.intervention().unwrap_or_default();
    assert!(tri_warn.contains("CYCLIC OSCILLATION"));
}

#[test]
fn test_anti_thrash_event_serde_roundtrip() {
    let event = AgentEvent::AntiThrashTripped {
        turn_id: 3,
        pattern: "file_target_thrashing".to_string(),
        target: Some("src/db.rs".to_string()),
        failures: 3,
        intervention: "Prescriptive stop guidance".to_string(),
    };

    let serialized = serde_json::to_string(&event).expect("must serialize");
    assert!(serialized.contains("\"anti_thrash_tripped\""));
    assert!(serialized.contains("\"target\":\"src/db.rs\""));

    let deserialized: AgentEvent = serde_json::from_str(&serialized).expect("must deserialize");
    assert_eq!(event, deserialized);
}

#[tokio::test]
async fn test_agent_loop_circuit_breaker_trip_and_early_exit() {
    let dir = tempdir().expect("tempdir");
    let ws_path = dir.path().to_path_buf();

    // Create a dummy file in workspace
    let file_path = ws_path.join("broken.rs");
    tokio::fs::write(&file_path, "fn broken() {}\n")
        .await
        .expect("write");

    // Scripted mock provider responses:
    // Repeatedly attempt to run a tool that fails on the same file target
    let responses = vec![
        MockResponse::with_tool_call(
            "call_1",
            "patch_file",
            json!({"path": "broken.rs", "search_block": "wrong_target_1", "replace_block": "fixed"}),
        ),
        MockResponse::with_tool_call(
            "call_2",
            "patch_file",
            json!({"path": "broken.rs", "search_block": "wrong_target_2", "replace_block": "fixed"}),
        ),
        MockResponse::with_tool_call(
            "call_3",
            "patch_file",
            json!({"path": "broken.rs", "search_block": "wrong_target_3", "replace_block": "fixed"}),
        ),
        MockResponse::with_tool_call(
            "call_4",
            "patch_file",
            json!({"path": "broken.rs", "search_block": "wrong_target_4", "replace_block": "fixed"}),
        ),
        // A 5th call that should NEVER be reached because the loop breaks on trip
        MockResponse::text_only("Should not be called if circuit breaker tripped"),
    ];

    let provider = Box::new(MockProvider::new(responses));
    let mut config = Config::default();
    config.git.auto_commit = false;
    config.agent.auto_approve = true;

    let mut agent = AgentLoop::new(&ws_path, config, provider);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let event_collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(ev) = event_rx.recv().await {
            events.push(ev);
        }
        events
    });

    let result = agent
        .execute_turn("Fix the failing test", event_tx, None)
        .await;
    assert!(result.is_ok(), "turn must finish cleanly");

    let events = event_collector.await.expect("collector join");

    // Verify an AntiThrashTripped event was emitted
    let has_tripped_event = events
        .iter()
        .any(|e| matches!(e, AgentEvent::AntiThrashTripped { .. }));
    assert!(
        has_tripped_event,
        "AgentEvent::AntiThrashTripped should be emitted"
    );

    // Verify TurnEnd status is "circuit_tripped"
    let turn_end = events.iter().find_map(|e| match e {
        AgentEvent::TurnEnd { status, .. } => Some(status.as_str()),
        _ => None,
    });
    assert_eq!(
        turn_end,
        Some("circuit_tripped"),
        "TurnEnd status should be 'circuit_tripped'"
    );
}
