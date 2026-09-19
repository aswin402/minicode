use crate::agent::subagent::mailbox::AgentMailbox;
use crate::agent::subagent::message::{AgentMessage, MessageIntent};
use crate::agent::subagent::types::{AgentId, SubagentRole, WorkspaceMode};

#[test]
fn test_agent_id_and_roles() {
    let parent = AgentId::parent();
    assert_eq!(parent.0, "parent");
    assert!(parent.is_parent());

    let scout_id = AgentId::new_subagent("scout");
    assert!(scout_id.0.starts_with("scout-"));

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
}

#[test]
fn test_agent_message_format_for_prompt() {
    let msg = AgentMessage {
        id: "msg-1".to_string(),
        sender: AgentId("scout-1".to_string()),
        recipient: AgentId::parent(),
        intent: MessageIntent::StatusUpdate,
        content: "Found 3 auth files".to_string(),
        timestamp: chrono::Utc::now(),
        metadata: None,
    };
    let formatted = msg.format_for_prompt();
    assert!(formatted.contains("<agent_message from=\"scout-1\" intent=\"status_update\""));
    assert!(formatted.contains("Found 3 auth files"));
    assert!(formatted.contains("</agent_message>"));
}

#[tokio::test]
async fn test_agent_mailbox_fifo_and_disk_persistence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let agent_dir = temp_dir
        .path()
        .join(".minicode")
        .join("agents")
        .join("test-agent");
    let mailbox = AgentMailbox::new(AgentId("test-agent".to_string()), &agent_dir).unwrap();

    assert_eq!(mailbox.unread_count().unwrap(), 0);

    let msg1 = AgentMessage {
        id: "m1".to_string(),
        sender: AgentId::parent(),
        recipient: AgentId("test-agent".to_string()),
        intent: MessageIntent::TaskInit,
        content: "Start search".to_string(),
        timestamp: chrono::Utc::now(),
        metadata: None,
    };
    let msg2 = AgentMessage {
        id: "m2".to_string(),
        sender: AgentId::parent(),
        recipient: AgentId("test-agent".to_string()),
        intent: MessageIntent::Feedback,
        content: "Also check tests/".to_string(),
        timestamp: chrono::Utc::now(),
        metadata: None,
    };

    mailbox.post(msg1).unwrap();
    mailbox.post(msg2).unwrap();
    assert_eq!(mailbox.unread_count().unwrap(), 2);

    let drained = mailbox.drain_unread().unwrap();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].content, "Start search");
    assert_eq!(drained[1].content, "Also check tests/");
    assert_eq!(mailbox.unread_count().unwrap(), 0);
}
