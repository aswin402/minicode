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

#[tokio::test]
async fn test_agent_auto_continue_on_iteration_limit() {
    let dir = tempdir().unwrap();
    let ws_path = dir.path().to_path_buf();
    tokio::fs::create_dir_all(ws_path.join("src"))
        .await
        .unwrap();
    tokio::fs::write(ws_path.join("src/a.txt"), "content a")
        .await
        .unwrap();
    tokio::fs::write(ws_path.join("src/b.txt"), "content b")
        .await
        .unwrap();

    // 2 tool calls, but max_tool_iterations = 1 with auto_continue = true
    let responses = vec![
        MockResponse::with_tool_call(
            "call_1",
            "read_file",
            serde_json::json!({"path": "src/a.txt"}),
        ),
        MockResponse::with_tool_call(
            "call_2",
            "read_file",
            serde_json::json!({"path": "src/b.txt"}),
        ),
        MockResponse::text_only("Finished reading both files."),
    ];

    let provider = Box::new(MockProvider::new(responses));
    let mut config = Config::default();
    config.git.auto_commit = false;
    config.agent.auto_approve = true;
    config.agent.max_tool_iterations = 1;
    config.agent.auto_continue = true;
    config.agent.max_auto_continues = 3;

    let mut agent = AgentLoop::new(&ws_path, config, provider);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(evt) = event_rx.recv().await {
            events.push(evt);
        }
        events
    });

    let turn = agent
        .execute_turn("Read both files", event_tx, None)
        .await
        .unwrap();

    let events = collector.await.unwrap();

    // Should have auto-continued and completed both tool calls
    assert_eq!(turn.tool_calls.len(), 2);
    assert_eq!(turn.tool_results.len(), 2);
    assert!(turn
        .assistant_response
        .contains("Finished reading both files"));

    // TurnEnd status must be "complete"
    let turn_end = events
        .iter()
        .find(|e| matches!(e, minicode::agent::types::AgentEvent::TurnEnd { .. }))
        .expect("TurnEnd event must be emitted");
    if let minicode::agent::types::AgentEvent::TurnEnd { status, .. } = turn_end {
        assert_eq!(status, minicode::constants::TURN_STATUS_COMPLETE);
    }
}

#[tokio::test]
async fn test_agent_pauses_when_auto_continue_exhausted() {
    let dir = tempdir().unwrap();
    let ws_path = dir.path().to_path_buf();
    tokio::fs::create_dir_all(ws_path.join("src"))
        .await
        .unwrap();
    tokio::fs::write(ws_path.join("src/a.txt"), "content a")
        .await
        .unwrap();

    // With max_tool_iterations = 1 and auto_continue = false, agent should pause after 1 iteration
    let responses = vec![
        MockResponse::with_tool_call(
            "call_1",
            "read_file",
            serde_json::json!({"path": "src/a.txt"}),
        ),
        MockResponse::with_tool_call(
            "call_2",
            "read_file",
            serde_json::json!({"path": "src/a.txt"}),
        ),
    ];

    let provider = Box::new(MockProvider::new(responses));
    let mut config = Config::default();
    config.git.auto_commit = false;
    config.agent.auto_approve = true;
    config.agent.max_tool_iterations = 1;
    config.agent.auto_continue = false;
    config.agent.max_auto_continues = 0;

    let mut agent = AgentLoop::new(&ws_path, config, provider);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(evt) = event_rx.recv().await {
            events.push(evt);
        }
        events
    });

    let turn = agent
        .execute_turn("Read files", event_tx, None)
        .await
        .unwrap();

    let events = collector.await.unwrap();

    // Only 1 tool iteration ran before pause
    assert_eq!(turn.tool_calls.len(), 1);
    assert!(turn
        .assistant_response
        .contains("Reached tool iteration limit"));

    // TurnEnd status must be "iteration_limit"
    let turn_end = events
        .iter()
        .find(|e| matches!(e, minicode::agent::types::AgentEvent::TurnEnd { .. }))
        .expect("TurnEnd event must be emitted");
    if let minicode::agent::types::AgentEvent::TurnEnd { status, .. } = turn_end {
        assert_eq!(status, minicode::constants::TURN_STATUS_ITERATION_LIMIT);
    }
}

#[tokio::test]
async fn test_agent_unbounded_execution_by_default() {
    let dir = tempdir().unwrap();
    let ws_path = dir.path().to_path_buf();
    tokio::fs::create_dir_all(ws_path.join("src"))
        .await
        .unwrap();
    tokio::fs::write(ws_path.join("src/1.txt"), "1")
        .await
        .unwrap();
    tokio::fs::write(ws_path.join("src/2.txt"), "2")
        .await
        .unwrap();
    tokio::fs::write(ws_path.join("src/3.txt"), "3")
        .await
        .unwrap();

    // 3 tool calls executed across iterations with default config (max_tool_iterations = 0 unbounded)
    let responses = vec![
        MockResponse::with_tool_call(
            "call_1",
            "read_file",
            serde_json::json!({"path": "src/1.txt"}),
        ),
        MockResponse::with_tool_call(
            "call_2",
            "read_file",
            serde_json::json!({"path": "src/2.txt"}),
        ),
        MockResponse::with_tool_call(
            "call_3",
            "read_file",
            serde_json::json!({"path": "src/3.txt"}),
        ),
        MockResponse::text_only("Finished reading all 3 files without interruption."),
    ];

    let provider = Box::new(MockProvider::new(responses));
    let mut config = Config::default();
    config.git.auto_commit = false;
    config.agent.auto_approve = true;
    assert_eq!(config.agent.max_tool_iterations, 0); // verify default is unbounded

    let mut agent = AgentLoop::new(&ws_path, config, provider);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(evt) = event_rx.recv().await {
            events.push(evt);
        }
        events
    });

    let turn = agent
        .execute_turn("Read all 3 files", event_tx, None)
        .await
        .unwrap();

    let events = collector.await.unwrap();

    // All 3 tool calls must run without pausing or hitting limit
    assert_eq!(turn.tool_calls.len(), 3);
    assert_eq!(turn.tool_results.len(), 3);
    assert!(turn
        .assistant_response
        .contains("Finished reading all 3 files without interruption"));

    // TurnEnd status must be "complete"
    let turn_end = events
        .iter()
        .find(|e| matches!(e, minicode::agent::types::AgentEvent::TurnEnd { .. }))
        .expect("TurnEnd event must be emitted");
    if let minicode::agent::types::AgentEvent::TurnEnd { status, .. } = turn_end {
        assert_eq!(status, minicode::constants::TURN_STATUS_COMPLETE);
    }
}
