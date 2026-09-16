mod common;

use common::{MockProvider, MockResponse};
use minicode::agent::types::{AgentEvent, Role, ToolCall};
use minicode::agent::AgentLoop;
use minicode::config::Config;
use minicode::session::store::SessionStore;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_agent_hydrate_from_events_structural_reconstruction() {
    let dir = tempdir().expect("Failed to create tempdir");
    let ws_path = dir.path().to_path_buf();

    let provider = Box::new(MockProvider::new(vec![]));
    let mut config = Config::default();
    config.git.auto_commit = false;
    config.agent.auto_approve = true;

    let mut agent = AgentLoop::new(&ws_path, config, provider);

    // Initial state check
    assert_eq!(agent.current_turn_id(), 0);
    assert_eq!(agent.cumulative_tokens_used(), 0);

    // Construct events representing 2 completed turns
    let events = vec![
        AgentEvent::TurnStart {
            turn_id: 1,
            model: "mock-model".to_string(),
            timestamp: "2026-09-16T12:00:00Z".to_string(),
            context_tokens: 100,
        },
        AgentEvent::UserPrompt {
            turn_id: 1,
            timestamp: "2026-09-16T12:00:00Z".to_string(),
            prompt: "Please inspect src/calculator.rs".to_string(),
        },
        AgentEvent::StreamDelta {
            turn_id: 1,
            delta: "<thought>Let me inspect the file first</thought>I will check the file."
                .to_string(),
        },
        AgentEvent::ToolCall {
            turn_id: 1,
            tool_id: "call_read_1".to_string(),
            tool: "read_file".to_string(),
            args: serde_json::json!({"path": "src/calculator.rs"}),
        },
        AgentEvent::ToolResult {
            turn_id: 1,
            tool_id: "call_read_1".to_string(),
            tool: "read_file".to_string(),
            success: true,
            output: "pub fn multiply(a: i32, b: i32) -> i32 { a * b }".to_string(),
            duration_ms: 5,
        },
        AgentEvent::StreamDelta {
            turn_id: 1,
            delta: "I inspected the function, it multiplies two numbers.".to_string(),
        },
        AgentEvent::TurnEnd {
            turn_id: 1,
            status: "completed".to_string(),
            total_tokens_used: 240,
            files_modified: vec![],
            cached_prompt_tokens: 0,
        },
        AgentEvent::TurnStart {
            turn_id: 2,
            model: "mock-model".to_string(),
            timestamp: "2026-09-16T12:01:00Z".to_string(),
            context_tokens: 250,
        },
        AgentEvent::UserPrompt {
            turn_id: 2,
            timestamp: "2026-09-16T12:01:00Z".to_string(),
            prompt: "Add a divide function".to_string(),
        },
        AgentEvent::ToolCall {
            turn_id: 2,
            tool_id: "call_write_1".to_string(),
            tool: "write_file".to_string(),
            args: serde_json::json!({"path": "src/calculator.rs", "content": "pub fn divide(a: i32, b: i32) -> i32 { a / b }"}),
        },
        AgentEvent::ToolResult {
            turn_id: 2,
            tool_id: "call_write_1".to_string(),
            tool: "write_file".to_string(),
            success: true,
            output: "File written successfully".to_string(),
            duration_ms: 12,
        },
        AgentEvent::FileModified {
            turn_id: 2,
            path: "src/calculator.rs".to_string(),
            action: "write".to_string(),
            backup: String::new(),
        },
        AgentEvent::StreamDelta {
            turn_id: 2,
            delta: "I have added the divide function to src/calculator.rs.".to_string(),
        },
        AgentEvent::TurnEnd {
            turn_id: 2,
            status: "completed".to_string(),
            total_tokens_used: 310,
            files_modified: vec!["src/calculator.rs".to_string()],
            cached_prompt_tokens: 0,
        },
    ];

    // Hydrate agent from events
    agent.hydrate_from_events("test-session-42", &events);

    // Verify session metadata restoration
    assert_eq!(agent.session_id(), "test-session-42");
    assert_eq!(agent.current_turn_id(), 2);
    assert_eq!(agent.cumulative_tokens_used(), 550);

    // Verify memory anchor working context is preserved from initial turn
    let anchor = agent.memory_anchor();
    assert_eq!(
        anchor.working_context.as_deref(),
        Some("Please inspect src/calculator.rs")
    );

    // Verify working set recency
    assert!(agent
        .active_working_set()
        .contains(&"src/calculator.rs".to_string()));

    // Verify reconstructed message sequence
    // Turn 1:
    //   Msg 0: User("Please inspect src/calculator.rs")
    //   Msg 1: Assistant with tools [call_read_1] (stripped thought block)
    //   Msg 2: ToolResult(call_read_1)
    //   Msg 3: Assistant("I inspected the function...")
    // Turn 2:
    //   Msg 4: User("Add a divide function")
    //   Msg 5: Assistant with tools [call_write_1]
    //   Msg 6: ToolResult(call_write_1)
    //   Msg 7: Assistant("I have added the divide function...")
    let msgs = agent.messages();
    assert_eq!(msgs.len(), 8);

    assert_eq!(msgs[0].role, Role::User);
    assert_eq!(msgs[0].content, "Please inspect src/calculator.rs");

    assert_eq!(msgs[1].role, Role::Assistant);
    assert_eq!(msgs[1].content, "I will check the file.");
    assert_eq!(
        msgs[1]
            .tool_calls
            .as_ref()
            .map(|tc: &Vec<ToolCall>| tc.len()),
        Some(1)
    );
    assert_eq!(msgs[1].tool_calls.as_ref().unwrap()[0].id, "call_read_1");

    assert_eq!(msgs[2].role, Role::Tool);
    assert_eq!(msgs[2].tool_call_id.as_deref(), Some("call_read_1"));

    assert_eq!(msgs[3].role, Role::Assistant);
    assert_eq!(
        msgs[3].content,
        "I inspected the function, it multiplies two numbers."
    );

    assert_eq!(msgs[4].role, Role::User);
    assert_eq!(msgs[4].content, "Add a divide function");

    assert_eq!(msgs[5].role, Role::Assistant);
    assert_eq!(
        msgs[5]
            .tool_calls
            .as_ref()
            .map(|tc: &Vec<ToolCall>| tc.len()),
        Some(1)
    );
    assert_eq!(msgs[5].tool_calls.as_ref().unwrap()[0].id, "call_write_1");

    assert_eq!(msgs[6].role, Role::Tool);
    assert_eq!(msgs[6].tool_call_id.as_deref(), Some("call_write_1"));

    assert_eq!(msgs[7].role, Role::Assistant);
    assert_eq!(
        msgs[7].content,
        "I have added the divide function to src/calculator.rs."
    );
}

#[tokio::test]
async fn test_cross_process_continuation_end_to_end() {
    let dir = tempdir().expect("Failed to create tempdir");
    let ws_path = dir.path().to_path_buf();

    // Step 1: Initialize first AgentLoop (Process 1)
    let responses_p1 = vec![MockResponse::text_only(
        "Hello! I am ready to help you with coding.",
    )];
    let provider_p1 = Box::new(MockProvider::new(responses_p1));
    let mut config = Config::default();
    config.git.auto_commit = false;
    config.agent.auto_approve = true;

    let mut agent_p1 = AgentLoop::new(&ws_path, config.clone(), provider_p1);
    let session_id = agent_p1.session_id().to_string();

    let (tx_p1, mut rx_p1) = mpsc::unbounded_channel();
    let collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(ev) = rx_p1.recv().await {
            events.push(ev);
        }
        events
    });

    agent_p1
        .execute_turn("Hi minicode", tx_p1, None)
        .await
        .expect("Turn 1 failed");

    let events = collector.await.expect("Collector task panicked");
    assert!(!events.is_empty(), "Turn 1 must produce events");

    // Persist events into SessionStore (as done in normal operation)
    let store = SessionStore::with_workspace(&ws_path);
    for event in &events {
        store
            .append_event(&session_id, event)
            .expect("Failed to append event");
    }

    // Verify SessionStore can see the session
    let last_sid = store.get_last_session_id();
    assert_eq!(last_sid.as_deref(), Some(session_id.as_str()));

    // Step 2: Spawn Process 2 (simulating `minicode run --continue` or `/session <id>`)
    let responses_p2 = vec![MockResponse::text_only(
        "I remember you greeted me earlier! Let's proceed.",
    )];
    let provider_p2 = Box::new(MockProvider::new(responses_p2));

    let mut agent_p2 = AgentLoop::new(&ws_path, config.clone(), provider_p2);
    // Initially agent_p2 has its own random session_id and turn 0
    assert_ne!(agent_p2.session_id(), session_id);
    assert_eq!(agent_p2.current_turn_id(), 0);

    // Load and hydrate
    let loaded_events = store
        .load_session(&session_id)
        .expect("Failed to load session from disk");
    agent_p2.hydrate_from_events(&session_id, &loaded_events);

    // Verify state match
    assert_eq!(agent_p2.session_id(), session_id);
    assert_eq!(agent_p2.current_turn_id(), 1);
    assert!(agent_p2.messages().len() >= 2);

    // Step 3: Execute Turn 2 on hydrated AgentLoop
    let (tx_p2, mut rx_p2) = mpsc::unbounded_channel();
    let collector_p2 = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(ev) = rx_p2.recv().await {
            events.push(ev);
        }
        events
    });

    agent_p2
        .execute_turn("What did I ask you first?", tx_p2, None)
        .await
        .expect("Turn 2 failed");

    let p2_events = collector_p2.await.expect("Collector 2 panicked");

    // Verify Turn 2 started with turn_id 2
    let turn_2_start = p2_events
        .iter()
        .any(|e| matches!(e, AgentEvent::TurnStart { turn_id: 2, .. }));
    assert!(turn_2_start, "Turn 2 must start with turn_id 2");
    assert_eq!(agent_p2.current_turn_id(), 2);
}
