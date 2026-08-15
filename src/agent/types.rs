use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    #[allow(dead_code)]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub tool_id: String,
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Turn {
    pub turn_id: usize,
    pub user_prompt: String,
    pub assistant_response: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<ToolResult>,
    pub tokens_used: usize,
    pub files_modified: Vec<String>,
}

/// Structured events emitted by minicode in headless mode (`--json-stream`)
/// or sent over internal Tokio MPSC channels to the Ratatui TUI renderer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event")]
pub enum AgentEvent {
    #[serde(rename = "turn_start")]
    TurnStart {
        turn_id: usize,
        timestamp: String,
        model: String,
        context_tokens: usize,
    },

    #[serde(rename = "stream_delta")]
    StreamDelta { turn_id: usize, delta: String },

    #[serde(rename = "tool_call")]
    ToolCall {
        turn_id: usize,
        tool_id: String,
        tool: String,
        args: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        turn_id: usize,
        tool_id: String,
        tool: String,
        success: bool,
        output: String,
        duration_ms: u64,
    },

    #[serde(rename = "approval_request")]
    ApprovalRequest {
        turn_id: usize,
        tool_id: String,
        tool: String,
        args: serde_json::Value,
        reason: String,
    },

    #[serde(rename = "file_modified")]
    FileModified {
        turn_id: usize,
        path: String,
        action: String,
        backup: String,
    },

    #[serde(rename = "turn_end")]
    TurnEnd {
        turn_id: usize,
        status: String,
        total_tokens_used: usize,
        files_modified: Vec<String>,
    },

    #[serde(rename = "error")]
    Error {
        turn_id: Option<usize>,
        code: String,
        message: String,
        retrying: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        retry_after_ms: Option<u64>,
    },

    #[serde(rename = "heartbeat")]
    Heartbeat {
        timestamp: String,
        status: String,
        turn_id: Option<usize>,
    },
}

/// Commands accepted via stdin when running in bidirectional machine mode (`--json-stream`)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "method", content = "params")]
pub enum StdinCommand {
    #[serde(rename = "user_input")]
    UserInput { text: String },

    #[serde(rename = "tool_response")]
    ToolResponse {
        tool_id: String,
        action: String, // "approve" or "reject"
        #[serde(default)]
        reason: Option<String>,
    },

    #[serde(rename = "abort")]
    Abort {},

    #[serde(rename = "configure")]
    Configure {
        #[serde(default)]
        auto_approve: Option<bool>,
        #[serde(default)]
        model: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = AgentEvent::TurnStart {
            turn_id: 1,
            timestamp: "2026-08-14T10:30:00Z".to_string(),
            model: "gemini-2.5-pro".to_string(),
            context_tokens: 1420,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"turn_start\""));
        assert!(json.contains("\"model\":\"gemini-2.5-pro\""));
    }

    #[test]
    fn test_stdin_command_deserialization() {
        let json = r#"{"method":"user_input","params":{"text":"Fix the bug"}}"#;
        let cmd: StdinCommand = serde_json::from_str(json).unwrap();
        match cmd {
            StdinCommand::UserInput { text } => assert_eq!(text, "Fix the bug"),
            _ => panic!("Expected UserInput"),
        }

        let approval_json =
            r#"{"method":"tool_response","params":{"tool_id":"call_123","action":"approve"}}"#;
        let approval: StdinCommand = serde_json::from_str(approval_json).unwrap();
        match approval {
            StdinCommand::ToolResponse {
                tool_id, action, ..
            } => {
                assert_eq!(tool_id, "call_123");
                assert_eq!(action, "approve");
            }
            _ => panic!("Expected ToolResponse"),
        }
    }
}
