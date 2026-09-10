use minicode::agent::types::AgentEvent;
use minicode::logging::{
    find_session_by_id_or_prefix, is_pid_alive, list_active_sessions, register_active_session,
    runtime_dir, HonoLogFormatter, LogTailer,
};
use std::fs::{File, OpenOptions};
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_runtime_active_session_registration_and_cleanup() {
    let temp = tempdir().unwrap();
    let session_id = format!("test-reg-{}", uuid::Uuid::new_v4());
    let session_file = temp.path().join(format!("{}.jsonl", session_id));
    std::fs::write(&session_file, "{\"session_meta\":{}}\n").unwrap();

    let guard = register_active_session(
        &session_id,
        temp.path(),
        "anthropic",
        "claude-3.7-sonnet",
        &session_file,
    )
    .unwrap();

    let active_list = list_active_sessions();
    assert!(
        active_list.iter().any(|a| a.session_id == session_id),
        "Active session should be found in list_active_sessions"
    );

    let runtime_file = runtime_dir().join(format!("{}.json", session_id));
    assert!(runtime_file.exists(), "Runtime lockfile must exist on disk");

    // Drop guard and verify cleanup
    drop(guard);
    assert!(
        !runtime_file.exists(),
        "Runtime lockfile must be removed on drop"
    );
    let active_after = list_active_sessions();
    assert!(!active_after.iter().any(|a| a.session_id == session_id));
}

#[test]
fn test_runtime_dead_pid_purging() {
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir).unwrap();

    let dead_session_id = format!("test-dead-{}", uuid::Uuid::new_v4());
    let dead_file = dir.join(format!("{}.json", dead_session_id));

    let dead_record = serde_json::json!({
        "session_id": dead_session_id,
        "pid": 9_999_999u32, // Non-existent PID
        "workspace": "/tmp/test",
        "provider": "openai",
        "model": "gpt-4o",
        "started_at": "2026-09-10T12:00:00Z",
        "session_file": "/tmp/test.jsonl"
    });

    std::fs::write(&dead_file, serde_json::to_string(&dead_record).unwrap()).unwrap();
    assert!(dead_file.exists());

    // list_active_sessions should check PID liveness, notice it's dead, and purge the file
    let active = list_active_sessions();
    assert!(!active.iter().any(|a| a.session_id == dead_session_id));
    assert!(
        !dead_file.exists(),
        "Stale dead-PID lockfile should be automatically purged"
    );
}

#[test]
fn test_pid_liveness_check() {
    let my_pid = std::process::id();
    assert!(is_pid_alive(my_pid), "Current process PID must be alive");
    assert!(!is_pid_alive(9_999_999), "PID 9999999 should not be alive");
}

#[test]
fn test_find_session_by_id_or_prefix() {
    let temp = tempdir().unwrap();
    let sessions_dir = temp.path().join(".minicode").join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let session_id = "20260910T120000Z-deadbeef";
    let session_file = sessions_dir.join(format!("{}.jsonl", session_id));
    std::fs::write(&session_file, "{\"session_meta\":{}}\n").unwrap();

    // 1. Exact match
    let found_exact = find_session_by_id_or_prefix(session_id, Some(temp.path()));
    assert!(found_exact.is_some());
    assert_eq!(found_exact.unwrap().0, session_file);

    // 2. Prefix match
    let found_prefix = find_session_by_id_or_prefix("20260910", Some(temp.path()));
    assert!(found_prefix.is_some());
    assert_eq!(found_prefix.unwrap().0, session_file);

    // 3. Suffix match
    let found_suffix = find_session_by_id_or_prefix("deadbeef", Some(temp.path()));
    assert!(found_suffix.is_some());
    assert_eq!(found_suffix.unwrap().0, session_file);

    // 4. Non-matching query
    let not_found = find_session_by_id_or_prefix("nonexistent_session_id", Some(temp.path()));
    assert!(not_found.is_none());
}

#[test]
fn test_hono_formatter_plain_and_color() {
    let plain_formatter = HonoLogFormatter::new(true);
    let color_formatter = HonoLogFormatter::new(false);

    let prompt_event = AgentEvent::UserPrompt {
        turn_id: 1,
        timestamp: "2026-09-10T12:00:00Z".to_string(),
        prompt: "Fix compiler warning in src/main.rs".to_string(),
    };

    let plain_prompt = plain_formatter.format_event(&prompt_event).unwrap();
    assert!(plain_prompt.contains("--> USER   Turn #1"));
    assert!(plain_prompt.contains("Fix compiler warning"));
    assert!(
        !plain_prompt.contains("\x1b["),
        "Plain mode must not contain ANSI escapes"
    );

    let color_prompt = color_formatter.format_event(&prompt_event).unwrap();
    assert!(color_prompt.contains("-->"));
    assert!(color_prompt.contains("USER"));
    assert!(color_prompt.contains("Turn #1"));
    assert!(
        color_prompt.contains("\x1b["),
        "Color mode must contain ANSI escapes"
    );

    let ok_tool = AgentEvent::ToolResult {
        tool_id: "t1".to_string(),
        turn_id: 1,
        tool: "read_file".to_string(),
        success: true,
        output: "line1\nline2\nline3\nline4".to_string(),
        duration_ms: 18,
    };

    let plain_tool_ok = plain_formatter.format_event(&ok_tool).unwrap();
    assert!(plain_tool_ok.contains("<-- TOOL   read_file 200 18ms"));
    assert!(plain_tool_ok.contains("4 lines"));

    let err_tool = AgentEvent::ToolResult {
        tool_id: "t2".to_string(),
        turn_id: 1,
        tool: "exec_cmd".to_string(),
        success: false,
        output: "exit status 1: cargo test failed".to_string(),
        duration_ms: 1250,
    };

    let plain_tool_err = plain_formatter.format_event(&err_tool).unwrap();
    assert!(plain_tool_err.contains("<-- TOOL   exec_cmd 500 1.25s"));
}

#[test]
fn test_hono_formatter_json() {
    let formatter = HonoLogFormatter::new(true);
    let tool_call = AgentEvent::ToolCall {
        turn_id: 1,
        tool_id: "call_1".to_string(),
        tool: "write_file".to_string(),
        args: serde_json::json!({ "path": "test.txt", "content": "hello" }),
    };

    let json_line = formatter.format_json(&tool_call).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_line).unwrap();

    assert_eq!(parsed["event_type"], "tool_call");
    assert_eq!(parsed["turn_id"], 1);
    assert_eq!(parsed["status_code"], 200);
    assert!(parsed["summary"].as_str().unwrap().contains("write_file"));
}

#[tokio::test]
async fn test_log_tailer_read_initial_tail() {
    let temp = tempdir().unwrap();
    let session_file = temp.path().join("session.jsonl");
    let mut file = File::create(&session_file).unwrap();

    // Write 5 user prompt events
    for i in 1..=5 {
        let event = AgentEvent::UserPrompt {
            turn_id: i,
            timestamp: "2026-09-10T12:00:00Z".to_string(),
            prompt: format!("Prompt number {}", i),
        };
        writeln!(file, "{}", serde_json::to_string(&event).unwrap()).unwrap();
    }
    file.flush().unwrap();

    let tailer = LogTailer::new(true, false, false, None);
    let mut read_file = File::open(&session_file).unwrap();
    let lines = tailer.read_initial_tail(&mut read_file, 3).unwrap();

    assert_eq!(lines.len(), 3, "Should return exactly last 3 lines");
    assert!(lines[0].contains("Turn #3"));
    assert!(lines[1].contains("Turn #4"));
    assert!(lines[2].contains("Turn #5"));
}

#[tokio::test]
async fn test_live_follow_file_growth() {
    let temp = tempdir().unwrap();
    let session_file = temp.path().join("session_live.jsonl");
    let mut file = File::create(&session_file).unwrap();

    let event1 = AgentEvent::UserPrompt {
        turn_id: 1,
        timestamp: "2026-09-10T12:00:00Z".to_string(),
        prompt: "Initial prompt".to_string(),
    };
    writeln!(file, "{}", serde_json::to_string(&event1).unwrap()).unwrap();
    file.flush().unwrap();

    let file_path = session_file.clone();
    let writer_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut appender = OpenOptions::new().append(true).open(&file_path).unwrap();
        let event2 = AgentEvent::TurnEnd {
            turn_id: 1,
            status: "success".to_string(),
            total_tokens_used: 1500,
            files_modified: vec!["src/main.rs".to_string()],
        };
        writeln!(appender, "{}", serde_json::to_string(&event2).unwrap()).unwrap();
        appender.flush().unwrap();
    });

    writer_task.await.unwrap();

    let tailer = LogTailer::new(true, false, false, None);
    let mut read_file = File::open(&session_file).unwrap();
    let lines = tailer.read_initial_tail(&mut read_file, 10).unwrap();

    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("Turn #1"));
    assert!(lines[1].contains("DONE   Turn #1"));
}
