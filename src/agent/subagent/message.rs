use crate::agent::subagent::types::AgentId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// High-level intent of an agent-to-agent (A2A) message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageIntent {
    TaskInit,
    StatusUpdate,
    ClarificationRequest,
    ClarificationResponse,
    Feedback,
    Handoff,
    TaskComplete,
}

#[allow(dead_code)]
impl MessageIntent {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageIntent::TaskInit => "task_init",
            MessageIntent::StatusUpdate => "status_update",
            MessageIntent::ClarificationRequest => "clarification_request",
            MessageIntent::ClarificationResponse => "clarification_response",
            MessageIntent::Feedback => "feedback",
            MessageIntent::Handoff => "handoff",
            MessageIntent::TaskComplete => "task_complete",
        }
    }
}

/// Typed message exchanged between agents via reactive mailboxes
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub sender: AgentId,
    pub recipient: AgentId,
    pub intent: MessageIntent,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[allow(dead_code)]
impl AgentMessage {
    /// Creates a new AgentMessage with a unique ID and current timestamp
    pub fn new(
        sender: AgentId,
        recipient: AgentId,
        intent: MessageIntent,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: format!("msg-{}", uuid::Uuid::new_v4()),
            sender,
            recipient,
            intent,
            content: content.into(),
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    /// Attaches structured metadata to the message
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Overrides message id (useful for testing or message correlation)
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Formats the message into an XML-style `<agent_message>` block suitable for LLM prompt ingestion
    pub fn format_for_prompt(&self) -> String {
        let intent_str =
            serde_json::to_string(&self.intent).unwrap_or_else(|_| "message".to_string());
        format!(
            "<agent_message from=\"{}\" intent=\"{}\" timestamp=\"{}\">\n{}\n</agent_message>",
            self.sender.0,
            intent_str.trim_matches('"'),
            self.timestamp.to_rfc3339(),
            self.content.trim()
        )
    }
}
