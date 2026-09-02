use minicode::agent::types::AgentEvent;
use minicode::context::checkpoint::SessionCheckpointer;
use minicode::context::working_memory::WorkingMemory;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_create_and_list_checkpoints() {
    let dir = tempdir().expect("tempdir");
    let wm = WorkingMemory::new(dir.path());
    wm.init_plan("Test Plan", &["Step 1".to_string(), "Step 2".to_string()])
        .expect("init plan");

    let events = vec![
        AgentEvent::UserPrompt {
            turn_id: 1,
            timestamp: "2026-09-02T10:00:00Z".to_string(),
            prompt: "Refactor codebase".to_string(),
        },
        AgentEvent::StreamDelta {
            turn_id: 1,
            delta: "Analyzing structure".to_string(),
        },
    ];

    let info = SessionCheckpointer::create_checkpoint(
        dir.path(),
        "session_123",
        "before-refactor",
        Some("Saved prior to major restructuring"),
        &events,
    )
    .expect("create checkpoint");

    assert_eq!(info.label, "before-refactor");
    assert_eq!(info.event_count, 2);
    assert!(info.has_working_plan);

    let list = SessionCheckpointer::list_checkpoints(dir.path(), Some("session_123"))
        .expect("list checkpoints");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, info.id);
}

#[test]
fn test_rewind_and_fork_checkpoint() {
    let dir = tempdir().expect("tempdir");
    let wm = WorkingMemory::new(dir.path());
    wm.init_plan("Original Plan", &["Step A".to_string()])
        .expect("init original");

    let info = SessionCheckpointer::create_checkpoint(
        dir.path(),
        "session_abc",
        "stable-state",
        None,
        &[],
    )
    .expect("checkpoint");

    // Overwrite plan with new mutated content
    wm.init_plan("Mutated Plan", &["Step B".to_string()])
        .expect("init mutated");
    let plan_text = fs::read_to_string(wm.task_plan_path()).expect("read mutated");
    assert!(plan_text.contains("Mutated Plan"));

    // Rewind back to checkpoint
    let (report, restored_events) =
        SessionCheckpointer::rewind_checkpoint(dir.path(), "session_abc", &info.id)
            .expect("rewind");

    assert_eq!(report.checkpoint_id, info.id);
    assert!(report.restored_plan);
    assert_eq!(restored_events.len(), 0);

    let restored_plan = fs::read_to_string(wm.task_plan_path()).expect("read restored");
    assert!(restored_plan.contains("Original Plan"));

    // Fork checkpoint
    let forked =
        SessionCheckpointer::fork_checkpoint(dir.path(), &info.id, Some("forked-experiment"))
            .expect("fork");

    assert_eq!(forked.label, "forked-experiment");
    assert_eq!(forked.parent_id, Some(info.id));
}

#[tokio::test]
async fn test_checkpoint_and_rewind_tools_dispatch() {
    let dir = tempdir().expect("tempdir");

    // 1. Create Checkpoint Tool
    let create_args = json!({
        "label": "ckpt-tool-test",
        "description": "Integration test snapshot"
    });

    let res1 = minicode::tools::registry::context_tools::dispatch(
        "checkpoint_session",
        &create_args,
        dir.path(),
    )
    .await;

    assert!(res1.is_some());
    let out1 = res1.unwrap().expect("create checkpoint tool");
    assert!(out1.contains("Created session checkpoint"));

    // 2. List Checkpoint Tool
    let list_args = json!({
        "action": "list"
    });

    let res2 = minicode::tools::registry::context_tools::dispatch(
        "rewind_session",
        &list_args,
        dir.path(),
    )
    .await;

    assert!(res2.is_some());
    let out2 = res2.unwrap().expect("list checkpoints tool");
    assert!(out2.contains("Workspace Session Checkpoints"));
    assert!(out2.contains("ckpt-tool-test"));
}
