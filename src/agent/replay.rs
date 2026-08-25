use crate::agent::mock_provider::MockProvider;
use crate::agent::types::{AgentEvent, ToolCall};
use crate::agent::AgentLoop;
use crate::config::Config;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use tokio::sync::mpsc;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub output: String,
    pub success: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TapeTurn {
    pub turn_id: usize,
    pub user_prompt: String,
    pub assistant_response: String,
    pub tool_calls: Vec<RecordedToolCall>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTape {
    pub name: String,
    pub model: String,
    pub created_at: String,
    pub turns: Vec<TapeTurn>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayReport {
    pub tape_name: String,
    pub total_turns: usize,
    pub passed_turns: usize,
    pub matched_tool_calls: usize,
    pub discrepancies: Vec<String>,
    pub passed: bool,
}

#[allow(dead_code)]
impl SessionTape {
    pub fn new(name: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            model: model.into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            turns: Vec::new(),
        }
    }

    /// Appends a recorded conversational turn to the tape
    pub fn add_turn(
        &mut self,
        turn_id: usize,
        user_prompt: String,
        assistant_response: String,
        tool_calls: Vec<RecordedToolCall>,
    ) {
        self.turns.push(TapeTurn {
            turn_id,
            user_prompt,
            assistant_response,
            tool_calls,
        });
    }

    /// Serializes session tape into a formatted JSONL file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        let mut file = File::create(path).map_err(|e| ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        })?;

        let header_json = serde_json::json!({
            "__type": "header",
            "name": self.name,
            "model": self.model,
            "created_at": self.created_at,
            "turn_count": self.turns.len(),
        });
        writeln!(file, "{}", header_json).map_err(|e| ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        })?;

        for turn in &self.turns {
            let turn_json =
                serde_json::to_string(turn).map_err(|e| ToolError::CommandExec(e.to_string()))?;
            writeln!(file, "{}", turn_json).map_err(|e| ToolError::FileOp {
                path: path.display().to_string(),
                source: e,
            })?;
        }

        Ok(())
    }

    /// Loads session tape from a JSONL file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        })?;

        let reader = BufReader::new(file);
        let mut name = "unnamed".to_string();
        let mut model = "mock".to_string();
        let mut created_at = String::new();
        let mut turns = Vec::new();

        for line_res in reader.lines() {
            let line = line_res.map_err(|e| ToolError::FileOp {
                path: path.display().to_string(),
                source: e,
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if val.get("__type").and_then(|t| t.as_str()) == Some("header") {
                    if let Some(n) = val.get("name").and_then(|n| n.as_str()) {
                        name = n.to_string();
                    }
                    if let Some(m) = val.get("model").and_then(|m| m.as_str()) {
                        model = m.to_string();
                    }
                    if let Some(c) = val.get("created_at").and_then(|c| c.as_str()) {
                        created_at = c.to_string();
                    }
                    continue;
                }
            }

            if let Ok(turn) = serde_json::from_str::<TapeTurn>(trimmed) {
                turns.push(turn);
            }
        }

        Ok(Self {
            name,
            model,
            created_at,
            turns,
        })
    }

    /// Builds a MockProvider pre-loaded with this tape's scripted responses
    pub fn build_mock_provider(&self) -> MockProvider {
        let provider = MockProvider::new(&self.name, &self.model);
        for turn in &self.turns {
            let mut calls = Vec::new();
            for c in &turn.tool_calls {
                calls.push(ToolCall {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    arguments: c.arguments.clone(),
                });
            }

            // Push the turn with tool calls first (if any) and response
            if !calls.is_empty() {
                provider.push_response(&[], calls);
            }
            if !turn.assistant_response.is_empty() {
                provider.push_response(&[&turn.assistant_response], vec![]);
            }
        }
        provider
    }
}

#[allow(dead_code)]
pub struct ReplayHarness;

#[allow(dead_code)]
impl ReplayHarness {
    /// Replays a SessionTape through an AgentLoop instance deterministically,
    /// capturing events and asserting structural parity.
    pub async fn run_replay(
        workspace_root: &Path,
        tape: &SessionTape,
        config: Config,
    ) -> Result<ReplayReport> {
        let mock_provider = tape.build_mock_provider();
        let mut config = config;
        // Deterministic replay re-executes recorded turns; the approval gate
        // has no responder here and would block forever on dangerous tools.
        config.agent.auto_approve = true;
        let mut agent = AgentLoop::new(workspace_root, config, Box::new(mock_provider));

        let mut passed_turns = 0;
        let mut matched_tool_calls = 0;
        let mut discrepancies = Vec::new();

        for turn in &tape.turns {
            let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
            let prompt = turn.user_prompt.clone();

            // Run execution turn against mock
            let turn_result = agent.execute_turn(&prompt, tx, None).await;
            if let Err(e) = turn_result {
                discrepancies.push(format!("Turn #{}: Execution error: {}", turn.turn_id, e));
                continue;
            }

            let mut observed_tools = Vec::new();
            let mut assistant_text = String::new();

            while let Ok(event) = rx.try_recv() {
                match event {
                    AgentEvent::StreamDelta { delta, .. } => {
                        assistant_text.push_str(&delta);
                    }
                    AgentEvent::ToolCall { tool, args, .. } => {
                        observed_tools.push((tool, args));
                    }
                    _ => {}
                }
            }

            // Verify tool call counts
            if observed_tools.len() == turn.tool_calls.len() {
                matched_tool_calls += observed_tools.len();
            } else {
                discrepancies.push(format!(
                    "Turn #{}: Tool call count mismatch (expected {}, got {})",
                    turn.turn_id,
                    turn.tool_calls.len(),
                    observed_tools.len()
                ));
            }

            passed_turns += 1;
        }

        let passed = discrepancies.is_empty() && passed_turns == tape.turns.len();

        Ok(ReplayReport {
            tape_name: tape.name.clone(),
            total_turns: tape.turns.len(),
            passed_turns,
            matched_tool_calls,
            discrepancies,
            passed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_tape_serialization_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let tape_path = dir.path().join("test_session.tape.jsonl");

        let mut tape = SessionTape::new("math_session", "mock-gpt");
        tape.add_turn(
            1,
            "Calculate 2 + 2".to_string(),
            "4".to_string(),
            vec![RecordedToolCall {
                id: "call_1".to_string(),
                name: "exec_cmd".to_string(),
                arguments: serde_json::json!({ "command": "echo 4" }),
                output: "4\n".to_string(),
                success: true,
            }],
        );

        tape.save_to_file(&tape_path).unwrap();
        let loaded = SessionTape::load_from_file(&tape_path).unwrap();

        assert_eq!(loaded.name, "math_session");
        assert_eq!(loaded.turns.len(), 1);
        assert_eq!(loaded.turns[0].user_prompt, "Calculate 2 + 2");
        assert_eq!(loaded.turns[0].tool_calls.len(), 1);
        assert_eq!(loaded.turns[0].tool_calls[0].name, "exec_cmd");
    }

    #[tokio::test]
    async fn test_deterministic_replay_harness() {
        let dir = tempfile::tempdir().unwrap();
        let mut tape = SessionTape::new("hello_test", "mock-agent");
        tape.add_turn(
            1,
            "Say hello".to_string(),
            "Hello there!".to_string(),
            vec![],
        );

        let config = Config::default();
        let report = ReplayHarness::run_replay(dir.path(), &tape, config)
            .await
            .unwrap();

        assert!(report.passed);
        assert_eq!(report.passed_turns, 1);
        assert_eq!(report.discrepancies.len(), 0);
    }
}
