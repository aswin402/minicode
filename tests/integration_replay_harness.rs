use minicode::agent::replay::{RecordedToolCall, ReplayHarness, SessionTape};
use minicode::config::Config;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_full_session_replay_with_tool_invocations() {
    let dir = tempdir().unwrap();
    let workspace = dir.path();

    // Setup a sample file in workspace
    let test_file = workspace.join("notes.txt");
    fs::write(&test_file, "Initial content line 1\n").unwrap();

    let mut tape = SessionTape::new("test_agent_simulation", "mock-agent-ultra");

    // Turn 1: Agent reads file
    tape.add_turn(
        1,
        "Read notes.txt".to_string(),
        "I read the notes file.".to_string(),
        vec![RecordedToolCall {
            id: "call_read_1".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({ "path": "notes.txt" }),
            output: "Initial content line 1\n".to_string(),
            success: true,
        }],
    );

    // Turn 2: Agent writes file
    tape.add_turn(
        2,
        "Append line 2 to notes.txt".to_string(),
        "I wrote line 2.".to_string(),
        vec![RecordedToolCall {
            id: "call_write_2".to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": "notes.txt",
                "content": "Initial content line 1\nLine 2 added\n"
            }),
            output: "Successfully wrote 36 bytes to notes.txt".to_string(),
            success: true,
        }],
    );

    // Save tape to disk and reload
    let tape_path = workspace.join("simulation.tape.jsonl");
    tape.save_to_file(&tape_path).unwrap();
    let reloaded_tape = SessionTape::load_from_file(&tape_path).unwrap();

    assert_eq!(reloaded_tape.turns.len(), 2);
    assert_eq!(reloaded_tape.name, "test_agent_simulation");

    // Run deterministic replay
    let config = Config::default();
    let report = ReplayHarness::run_replay(workspace, &reloaded_tape, config)
        .await
        .unwrap();

    assert!(
        report.passed,
        "Replay failed with discrepancies: {:?}",
        report.discrepancies
    );
    assert_eq!(report.passed_turns, 2);
    assert_eq!(report.matched_tool_calls, 2);
}
