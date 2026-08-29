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

    // Deleting again should return Ok(false)
    let deleted_again = store.delete_session(&session_id).unwrap();
    assert!(!deleted_again);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_utf8_safe_truncation_and_markdown_export() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_utf8_test_{}", uuid::Uuid::new_v4()));
    let store = SessionStore::with_dir(temp_dir.clone());
    let session_id = store.create_session(&temp_dir).unwrap();

    // Multi-byte Unicode (emojis, CJK characters, math symbols)
    let unicode_text = "🚀 Rust agent 🦀 探索智能代理 🧮 ∫(x)dx 🌟 ".repeat(60);
    let event1 = AgentEvent::TurnStart {
        turn_id: 1,
        timestamp: "2026-08-28T12:00:00Z".to_string(),
        model: "claude-3-7-sonnet".to_string(),
        context_tokens: 1500,
    };
    let event2 = AgentEvent::StreamDelta {
        turn_id: 1,
        delta: unicode_text.clone(),
    };
    let event3 = AgentEvent::ToolResult {
        turn_id: 1,
        tool_id: "call_utf8".to_string(),
        tool: "exec_cmd".to_string(),
        success: true,
        output: unicode_text.clone(),
        duration_ms: 250,
    };
    let event4 = AgentEvent::GitCommit {
        turn_id: 1,
        hash: "a1b2c3d4e5f67890".to_string(),
        message: "feat: ✨ unicode commit message".to_string(),
        files: vec!["src/main.rs".to_string()],
    };

    store.append_event(&session_id, &event1).unwrap();
    store.append_event(&session_id, &event2).unwrap();
    store.append_event(&session_id, &event3).unwrap();
    store.append_event(&session_id, &event4).unwrap();

    let summary = store.get_session_summary(&session_id).unwrap();
    assert!(!summary.first_prompt.is_empty());
    assert!(!summary.last_response.is_empty());

    let export_path = temp_dir.join("unicode_export.md");
    let exported = store.export_markdown(&session_id, &export_path).unwrap();
    assert!(exported.exists());

    let content = std::fs::read_to_string(&exported).unwrap();
    assert!(content.contains("Tool Execution Time"));
    assert!(content.contains("[truncated]"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_path_traversal_rejection() {
    let temp_dir = std::env::temp_dir().join(format!("minicode_sec_test_{}", uuid::Uuid::new_v4()));
    let store = SessionStore::with_dir(temp_dir.clone());

    let malicious_ids = vec![
        "../escape",
        "../../etc/passwd",
        "nested/sub/id",
        "back\\slash",
        "",
    ];

    for bad_id in malicious_ids {
        assert!(store.load_session(bad_id).is_err());
        assert!(store.delete_session(bad_id).is_err());
        assert!(store.get_session_summary(bad_id).is_err());
        assert!(store.fork_session(bad_id, &temp_dir).is_err());
        assert!(store
            .export_markdown(bad_id, &temp_dir.join("out.md"))
            .is_err());
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_corrupted_jsonl_resilience() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_corrupt_test_{}", uuid::Uuid::new_v4()));
    let store = SessionStore::with_dir(temp_dir.clone());
    let session_id = store.create_session(&temp_dir).unwrap();

    let valid_event = AgentEvent::TurnStart {
        turn_id: 1,
        timestamp: "2026-08-28T10:00:00Z".to_string(),
        model: "gemini-2.5-pro".to_string(),
        context_tokens: 500,
    };
    store.append_event(&session_id, &valid_event).unwrap();

    // Manually inject invalid JSON lines into the session file
    let file_path = temp_dir.join(format!("{}.jsonl", session_id));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&file_path)
        .unwrap();
    writeln!(file, "{{not_valid_json: 123}}").unwrap();
    writeln!(file, "random garbage line").unwrap();

    let valid_event2 = AgentEvent::StreamDelta {
        turn_id: 1,
        delta: "Valid delta after corruption".to_string(),
    };
    let line = serde_json::to_string(&valid_event2).unwrap();
    writeln!(file, "{}", line).unwrap();

    // load_session should gracefully skip corrupted lines and load valid events
    let events = store.load_session(&session_id).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], valid_event);
    assert_eq!(events[1], valid_event2);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_empty_session_summary() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_empty_test_{}", uuid::Uuid::new_v4()));
    let store = SessionStore::with_dir(temp_dir.clone());
    let session_id = store.create_session(&temp_dir).unwrap();

    let summary = store.get_session_summary(&session_id).unwrap();
    assert_eq!(summary.id, session_id);
    assert_eq!(summary.total_events, 0);
    assert_eq!(summary.total_turns, 0);
    assert_eq!(summary.total_tokens, 0);
    assert_eq!(summary.total_duration_ms, 0);
    assert!(summary.tools_used.is_empty());
    assert!(summary.files_touched.is_empty());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_user_prompt_in_summary_and_export() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_prompt_test_{}", uuid::Uuid::new_v4()));
    let store = SessionStore::with_dir(temp_dir.clone());
    let session_id = store.create_session(&temp_dir).unwrap();

    let user_event = AgentEvent::UserPrompt {
        turn_id: 1,
        timestamp: "2026-08-28T14:00:00Z".to_string(),
        prompt: "Refactor authentication module to support OAuth2".to_string(),
    };
    let turn_start = AgentEvent::TurnStart {
        turn_id: 1,
        timestamp: "2026-08-28T14:00:01Z".to_string(),
        model: "claude-3-7-sonnet".to_string(),
        context_tokens: 1000,
    };
    let stream_delta = AgentEvent::StreamDelta {
        turn_id: 1,
        delta: "I will start by checking auth.rs".to_string(),
    };
    let turn_end = AgentEvent::TurnEnd {
        turn_id: 1,
        status: "completed".to_string(),
        total_tokens_used: 350,
        files_modified: vec!["src/auth.rs".to_string()],
    };

    store.append_event(&session_id, &user_event).unwrap();
    store.append_event(&session_id, &turn_start).unwrap();
    store.append_event(&session_id, &stream_delta).unwrap();
    store.append_event(&session_id, &turn_end).unwrap();

    let summary = store.get_session_summary(&session_id).unwrap();
    assert_eq!(
        summary.first_prompt,
        "Refactor authentication module to support OAuth2"
    );
    assert_eq!(summary.total_events, 4);

    let export_path = temp_dir.join("export.md");
    store.export_markdown(&session_id, &export_path).unwrap();
    assert!(export_path.exists());

    let md_content = std::fs::read_to_string(&export_path).unwrap();
    assert!(md_content.contains("### 👤 User"));
    assert!(md_content.contains("Refactor authentication module to support OAuth2"));
    assert!(md_content.contains("### 🎯 Turn 1"));
    assert!(md_content.contains("I will start by checking auth.rs"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}
