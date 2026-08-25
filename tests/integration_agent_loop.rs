mod common;

use common::{MockProvider, MockResponse};
use minicode::agent::AgentLoop;
use minicode::config::Config;
use tempfile::tempdir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_autonomous_agent_read_and_patch_turn() {
    let dir = tempdir().unwrap();
    let ws_path = dir.path().to_path_buf();

    // Create an initial file in workspace
    let file_path = ws_path.join("src/lib.rs");
    tokio::fs::create_dir_all(ws_path.join("src"))
        .await
        .unwrap();
    tokio::fs::write(
        &file_path,
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    )
    .await
    .unwrap();

    // Scripted responses:
    // 1. LLM decides to read file
    // 2. LLM decides to patch file
    // 3. LLM completes turn with message
    let responses = vec![
        MockResponse::with_tool_call(
            "call_1",
            "read_file",
            serde_json::json!({"path": "src/lib.rs"}),
        ),
        MockResponse::with_tool_call(
            "call_2",
            "patch_file",
            serde_json::json!({
                "path": "src/lib.rs",
                "search_block": "a + b",
                "replace_block": "a + b + 1"
            }),
        ),
        MockResponse::text_only("I have successfully modified the add function in src/lib.rs."),
    ];

    let provider = Box::new(MockProvider::new(responses));
    let mut config = Config::default();
    config.git.auto_commit = false; // no git repo needed for this basic test
    config.agent.auto_approve = true; // scripted tools must run without the approval gate

    let mut agent = AgentLoop::new(&ws_path, config, provider);

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    // Spawn a collector for events
    let event_collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(evt) = event_rx.recv().await {
            events.push(evt);
        }
        events
    });

    let turn = agent
        .execute_turn("Please change add to add 1", event_tx, None)
        .await
        .unwrap();

    let events = event_collector.await.unwrap();

    // Verify turn results
    assert_eq!(turn.tool_calls.len(), 2);
    assert_eq!(turn.tool_results.len(), 2);
    assert!(turn.files_modified.contains(&"src/lib.rs".to_string()));
    assert!(turn.assistant_response.contains("successfully modified"));

    // Verify actual file on disk
    let modified_content = tokio::fs::read_to_string(&file_path).await.unwrap();
    assert!(modified_content.contains("a + b + 1"));

    // Verify emitted events
    assert!(events
        .iter()
        .any(|e| matches!(e, minicode::agent::types::AgentEvent::TurnStart { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, minicode::agent::types::AgentEvent::TurnEnd { .. })));
}

#[tokio::test]
async fn test_agent_cancellation_during_turn() {
    let dir = tempdir().unwrap();
    let ws_path = dir.path().to_path_buf();

    let responses = vec![MockResponse::with_tool_call(
        "call_1",
        "read_file",
        serde_json::json!({"path": "src/missing.rs"}),
    )];

    let provider = Box::new(MockProvider::new(responses));
    let mut config = Config::default();
    config.git.auto_commit = false;

    let mut agent = AgentLoop::new(&ws_path, config, provider);

    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let cancel_token = CancellationToken::new();

    // Cancel immediately before/during turn
    cancel_token.cancel();

    let turn = agent
        .execute_turn("Read something", event_tx, Some(cancel_token))
        .await
        .unwrap();

    // Turn should complete gracefully with empty or partial tools
    assert_eq!(turn.turn_id, 1);
}
