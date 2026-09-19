use minicode::agent::mock_provider::MockProvider;
use minicode::agent::subagent::{
    AgentId, AgentMailbox, AgentMessage, MessageIntent, SubagentRole, WorkspaceMode,
};
use minicode::agent::types::AgentEvent;
use minicode::agent::AgentLoop;
use minicode::config::{Config, ToolFilterMode};
use minicode::tools::ToolRegistry;
use minicode::ui::view::{TimelineEntry, TimelineView};
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_subagent_mailbox_drain_and_prompt_formatting() {
    let dir = tempdir().expect("tempdir creation failed");
    let parent_id = AgentId::parent();
    let mailbox = AgentMailbox::new(parent_id, dir.path()).expect("mailbox creation failed");

    let subagent_id = AgentId("coder-1".to_string());
    let task_message = "Refactored user_service.rs to use argon2 password hashing";

    let message = AgentMessage::new(
        subagent_id.clone(),
        AgentId::parent(),
        MessageIntent::TaskComplete,
        task_message,
    );

    // Verify initial count is zero
    assert_eq!(mailbox.unread_count().expect("unread count"), 0);

    // Post message from subagent to parent
    mailbox.post(message).expect("failed to post message");
    assert_eq!(mailbox.unread_count().expect("unread count"), 1);

    // Drain unread messages
    let drained = mailbox.drain_unread().expect("drain unread messages");
    assert_eq!(drained.len(), 1);

    let msg = &drained[0];
    assert_eq!(msg.sender, subagent_id);
    assert_eq!(msg.recipient, AgentId::parent());
    assert_eq!(msg.intent, MessageIntent::TaskComplete);
    assert_eq!(msg.content, task_message);

    // Verify format_for_prompt produces the XML block
    let prompt_block = msg.format_for_prompt();
    assert!(prompt_block.contains("<agent_message"));
    assert!(prompt_block.contains("from=\"coder-1\""));
    assert!(prompt_block.contains("intent=\"task_complete\""));
    assert!(prompt_block.contains(task_message));
    assert!(prompt_block.ends_with("</agent_message>"));

    // Mailbox is now empty after draining
    assert_eq!(mailbox.unread_count().expect("unread count"), 0);
}

#[test]
fn test_tool_filter_mode_parsing() {
    assert_eq!(
        "read_only".parse::<ToolFilterMode>(),
        Ok(ToolFilterMode::ReadOnly)
    );
    assert_eq!(
        "readonly".parse::<ToolFilterMode>(),
        Ok(ToolFilterMode::ReadOnly)
    );
    assert_eq!(
        "standard".parse::<ToolFilterMode>(),
        Ok(ToolFilterMode::Standard)
    );
    assert_eq!(
        "dynamic".parse::<ToolFilterMode>(),
        Ok(ToolFilterMode::Dynamic)
    );
    assert_eq!(
        "core_only".parse::<ToolFilterMode>(),
        Ok(ToolFilterMode::CoreOnly)
    );
    assert_eq!(
        "core".parse::<ToolFilterMode>(),
        Ok(ToolFilterMode::CoreOnly)
    );
    assert_eq!("full".parse::<ToolFilterMode>(), Ok(ToolFilterMode::Full));
    assert_eq!("all".parse::<ToolFilterMode>(), Ok(ToolFilterMode::Full));

    assert_eq!(ToolFilterMode::ReadOnly.to_string(), "read_only");
    assert_eq!(ToolFilterMode::Standard.to_string(), "standard");

    assert!("unknown_mode".parse::<ToolFilterMode>().is_err());
}

#[test]
fn test_subagent_roles_and_workspace_modes() {
    assert_eq!(
        SubagentRole::Scout.default_workspace_mode(),
        WorkspaceMode::Shared
    );
    assert_eq!(
        SubagentRole::Coder.default_workspace_mode(),
        WorkspaceMode::Worktree
    );
    assert_eq!(
        SubagentRole::Tester.default_workspace_mode(),
        WorkspaceMode::Worktree
    );
    assert_eq!(
        SubagentRole::Reviewer.default_workspace_mode(),
        WorkspaceMode::Shared
    );

    // Verify badges and role string tags
    assert_eq!(SubagentRole::Scout.badge(), "Scout");
    assert_eq!(SubagentRole::Coder.badge(), "Coder");
    assert_eq!(SubagentRole::Tester.badge(), "Tester");
    assert_eq!(SubagentRole::Reviewer.badge(), "Reviewer");

    assert_eq!(SubagentRole::Scout.tool_filter_mode(), "read_only");
    assert_eq!(SubagentRole::Reviewer.tool_filter_mode(), "read_only");
    assert_eq!(SubagentRole::Coder.tool_filter_mode(), "standard");
    assert_eq!(SubagentRole::Tester.tool_filter_mode(), "standard");
}

#[test]
fn test_tool_registry_filtering_modes() {
    let schemas = ToolRegistry::get_tool_schemas();
    assert!(!schemas.is_empty());

    // 1. ReadOnly mode filtering
    let read_only_schemas = ToolRegistry::filter_tools(ToolFilterMode::ReadOnly, schemas.clone());
    for s in &read_only_schemas {
        assert!(
            minicode::tools::is_read_only(&s.name),
            "Tool `{}` in ReadOnly mode must be read-only",
            s.name
        );
    }
    assert!(read_only_schemas.iter().any(|s| s.name == "read_file"));
    assert!(!read_only_schemas.iter().any(|s| s.name == "write_file"));
    assert!(!read_only_schemas.iter().any(|s| s.name == "patch_file"));

    // 2. Standard mode filtering (dangerous meta tools filtered out)
    let standard_schemas = ToolRegistry::filter_tools(ToolFilterMode::Standard, schemas.clone());
    assert!(!standard_schemas.iter().any(|s| s.name == "spawn_subagent"));
    assert!(!standard_schemas.iter().any(|s| s.name == "activate_tools"));
    assert!(standard_schemas.iter().any(|s| s.name == "read_file"));
    assert!(standard_schemas.iter().any(|s| s.name == "write_file"));
    assert!(standard_schemas.iter().any(|s| s.name == "exec_cmd"));
}

#[tokio::test]
async fn test_agent_loop_mailbox_ingestion() {
    let dir = tempdir().expect("tempdir creation failed");
    let ws_path = dir.path();

    let mock_provider = MockProvider::new("mock-model", "mock-model");
    mock_provider.push_response(&["I have received the subagent report."], vec![]);

    let config = Config::default();
    let mut agent_loop = AgentLoop::new(ws_path, config, Box::new(mock_provider));

    assert!(
        agent_loop.mailbox().is_some(),
        "AgentLoop should initialize parent mailbox"
    );
    let parent_mb = agent_loop.mailbox().expect("mailbox must exist").clone();

    // Post an A2A message from a subagent to the parent's mailbox
    let subagent_id = AgentId("coder-1".to_string());
    let subagent_msg = AgentMessage::new(
        subagent_id,
        AgentId::parent(),
        MessageIntent::TaskComplete,
        "Implementation finished with 100% tests passing.",
    );
    parent_mb
        .post(subagent_msg)
        .expect("posting message to parent mailbox");

    // Execute turn
    let (tx, _rx) = mpsc::unbounded_channel();
    let turn_result = agent_loop
        .execute_turn("Review current project progress", tx, None)
        .await;

    assert!(turn_result.is_ok(), "turn must succeed");

    // Verify that the subagent message was ingested into the conversation history
    let messages = agent_loop.messages();
    let ingested_msg = messages
        .iter()
        .find(|m| m.content.contains("<agent_message from=\"coder-1\""));

    assert!(
        ingested_msg.is_some(),
        "A2A subagent message should be ingested into AgentLoop messages"
    );
    let content = &ingested_msg.unwrap().content;
    assert!(content.contains("intent=\"task_complete\""));
    assert!(content.contains("Implementation finished with 100% tests passing."));

    // Verify parent mailbox was drained
    assert_eq!(parent_mb.unread_count().expect("unread count"), 0);
}

#[test]
fn test_agent_event_subagent_variants_serde() {
    // 1. SubagentProgress
    let progress_event = AgentEvent::SubagentProgress {
        turn_id: 1,
        subagent_id: "scout-1".to_string(),
        role: "Scout".to_string(),
        action: "Reading src/config.rs".to_string(),
        status: "running".to_string(),
    };

    let serialized = serde_json::to_string(&progress_event).expect("serialize progress");
    assert!(serialized.contains("\"event\":\"subagent_progress\""));
    assert!(serialized.contains("\"subagent_id\":\"scout-1\""));
    assert!(serialized.contains("\"action\":\"Reading src/config.rs\""));

    let deserialized: AgentEvent = serde_json::from_str(&serialized).expect("deserialize progress");
    assert_eq!(progress_event, deserialized);

    // 2. SubagentCompleted
    let completed_event = AgentEvent::SubagentCompleted {
        turn_id: 1,
        subagent_id: "scout-1".to_string(),
        role: "Scout".to_string(),
        success: true,
        summary: "Found all configuration endpoints".to_string(),
    };

    let serialized = serde_json::to_string(&completed_event).expect("serialize completed");
    assert!(serialized.contains("\"event\":\"subagent_completed\""));
    assert!(serialized.contains("\"success\":true"));

    let deserialized: AgentEvent =
        serde_json::from_str(&serialized).expect("deserialize completed");
    assert_eq!(completed_event, deserialized);
}

#[test]
fn test_timeline_subagent_events_rendering() {
    let mut timeline = TimelineView::new();

    // Ingest progress event
    timeline.handle_subagent_progress(
        "coder-42",
        "Coder",
        "patch_file(\"src/auth.rs\")",
        "running",
    );

    assert_eq!(timeline.entries.len(), 1);
    match &timeline.entries[0] {
        TimelineEntry::SubagentTree(block) => {
            assert_eq!(block.id, "coder-42");
            assert_eq!(block.role_name, "Coder");
            assert!(block.is_running);
            assert_eq!(block.items.len(), 1);
            assert_eq!(block.items[0].detail, "patch_file(\"src/auth.rs\")");
        }
        other => panic!("Expected SubagentTree, got {:?}", other),
    }

    // Ingest second progress action on same subagent
    timeline.handle_subagent_progress("coder-42", "Coder", "cargo check -j 1", "ok");
    match &timeline.entries[0] {
        TimelineEntry::SubagentTree(block) => {
            assert_eq!(block.items.len(), 2);
            assert_eq!(block.items[1].detail, "cargo check -j 1");
        }
        other => panic!("Expected SubagentTree, got {:?}", other),
    }

    // Ingest completion event
    timeline.handle_subagent_completed(
        "coder-42",
        "Coder",
        true,
        "Auth module refactored and verified cleanly",
    );

    match &timeline.entries[0] {
        TimelineEntry::SubagentTree(block) => {
            assert!(!block.is_running);
            assert!(block.is_success);
            assert_eq!(
                block.outcome.as_deref(),
                Some("Auth module refactored and verified cleanly")
            );
        }
        other => panic!("Expected SubagentTree, got {:?}", other),
    }

    // Verify transcript export formats subagent block without panicking
    let transcript = timeline.get_all_transcript_text();
    assert!(transcript.contains("### Subagent: coder-42 (Coder) [Success]"));
    assert!(transcript.contains("patch_file(\"src/auth.rs\")"));
    assert!(transcript.contains("cargo check -j 1"));
    assert!(transcript.contains("Auth module refactored and verified cleanly"));
}
