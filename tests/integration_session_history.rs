use minicode::agent::types::AgentEvent;
use minicode::session::store::SessionStore;

#[tokio::test]
async fn test_session_summary_analytics() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_history_test_{}", uuid::Uuid::new_v4()));
    let store = SessionStore::with_dir(temp_dir.clone());
    let session_id = store.create_session(&temp_dir).unwrap();

    let event1 = AgentEvent::TurnStart {
        turn_id: 1,
        timestamp: "2026-08-28T10:00:00Z".to_string(),
        model: "gemini-2.5-pro".to_string(),
        context_tokens: 1200,
    };
    let event2 = AgentEvent::StreamDelta {
        turn_id: 1,
        delta: "I am analyzing the workspace.".to_string(),
    };
    let event3 = AgentEvent::ToolCall {
        turn_id: 1,
        tool_id: "call_1".to_string(),
        tool: "exec_cmd".to_string(),
        args: serde_json::json!({"cmd": "ls -la", "path": "src/main.rs"}),
    };
    let event3_res = AgentEvent::ToolResult {
        turn_id: 1,
        tool_id: "call_1".to_string(),
        tool: "exec_cmd".to_string(),
        success: true,
        output: "total 0".to_string(),
        duration_ms: 1500,
    };
    let event4 = AgentEvent::FileModified {
        turn_id: 1,
        path: "src/lib.rs".to_string(),
        action: "patch".to_string(),
        backup: "src/lib.rs.bak".to_string(),
    };
    let event5 = AgentEvent::TurnEnd {
        turn_id: 1,
        status: "completed".to_string(),
        total_tokens_used: 450,
        files_modified: vec!["src/lib.rs".to_string()],
    };

    store.append_event(&session_id, &event1).unwrap();
    store.append_event(&session_id, &event2).unwrap();
    store.append_event(&session_id, &event3).unwrap();
    store.append_event(&session_id, &event3_res).unwrap();
    store.append_event(&session_id, &event4).unwrap();
    store.append_event(&session_id, &event5).unwrap();

    let summary = store.get_session_summary(&session_id).unwrap();
    assert_eq!(summary.id, session_id);
    assert_eq!(summary.model, "gemini-2.5-pro");
    assert_eq!(summary.total_turns, 1);
    assert_eq!(summary.total_events, 6);
    assert_eq!(summary.total_tokens, 450);
    assert_eq!(summary.total_duration_ms, 1500);
    assert!(summary
        .tools_used
        .iter()
        .any(|(t, c)| t == "exec_cmd" && *c == 1));
    assert!(summary.files_touched.contains(&"src/main.rs".to_string()));
    assert!(summary.files_touched.contains(&"src/lib.rs".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_session_forking_and_isolation() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_fork_test_{}", uuid::Uuid::new_v4()));
    let store = SessionStore::with_dir(temp_dir.clone());
    let session_id = store.create_session(&temp_dir).unwrap();

    let event = AgentEvent::TurnStart {
        turn_id: 1,
        timestamp: "2026-08-28T10:00:00Z".to_string(),
        model: "claude-3-7-sonnet".to_string(),
        context_tokens: 800,
    };
    store.append_event(&session_id, &event).unwrap();

    // Fork the session
    let forked_id = store.fork_session(&session_id, &temp_dir).unwrap();
    assert_ne!(session_id, forked_id);

    // Verify forked session has original history
    let forked_events = store.load_session(&forked_id).unwrap();
    assert_eq!(forked_events.len(), 1);
    assert_eq!(forked_events[0], event);

    // Append new event to forked session only
    let event2 = AgentEvent::StreamDelta {
        turn_id: 1,
        delta: "Forked branch response".to_string(),
    };
    store.append_event(&forked_id, &event2).unwrap();

    // Original session is untouched
    let orig_events = store.load_session(&session_id).unwrap();
    assert_eq!(orig_events.len(), 1);

    // Forked session has 2 events
    let updated_forked = store.load_session(&forked_id).unwrap();
    assert_eq!(updated_forked.len(), 2);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_session_export_markdown_transcript() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_export_test_{}", uuid::Uuid::new_v4()));
    let store = SessionStore::with_dir(temp_dir.clone());
    let session_id = store.create_session(&temp_dir).unwrap();

    let event1 = AgentEvent::TurnStart {
        turn_id: 1,
        timestamp: "2026-08-28T10:00:00Z".to_string(),
        model: "gemini-2.5-pro".to_string(),
        context_tokens: 500,
    };
    let event2 = AgentEvent::ToolCall {
        turn_id: 1,
        tool_id: "t_1".to_string(),
        tool: "patch_file".to_string(),
        args: serde_json::json!({"path": "src/main.rs"}),
    };
    let event3 = AgentEvent::ToolResult {
        turn_id: 1,
        tool_id: "t_1".to_string(),
        tool: "patch_file".to_string(),
        success: true,
        output: "Applied 1 diff block cleanly.".to_string(),
        duration_ms: 120,
    };

    store.append_event(&session_id, &event1).unwrap();
    store.append_event(&session_id, &event2).unwrap();
    store.append_event(&session_id, &event3).unwrap();

    let export_path = temp_dir.join("transcript.md");
    let exported = store.export_markdown(&session_id, &export_path).unwrap();
    assert!(exported.exists());

    let content = std::fs::read_to_string(&exported).unwrap();
    assert!(content.contains("# minicode Session Transcript"));
    assert!(content.contains("patch_file"));
    assert!(content.contains("Applied 1 diff block cleanly."));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_session_deletion() {
    let temp_dir = std::env::temp_dir().join(format!("minicode_del_test_{}", uuid::Uuid::new_v4()));
    let store = SessionStore::with_dir(temp_dir.clone());
    let session_id = store.create_session(&temp_dir).unwrap();

    let sessions_before = store.list_sessions().unwrap();
    assert_eq!(sessions_before.len(), 1);

    let deleted = store.delete_session(&session_id).unwrap();
    assert!(deleted);

    let sessions_after = store.list_sessions().unwrap();
    assert_eq!(sessions_after.len(), 0);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
