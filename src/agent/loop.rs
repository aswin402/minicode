#![allow(dead_code)]

use crate::agent::prompt::PromptBuilder;
use crate::agent::provider::{CompletionOptions, Provider, StreamChunk};
use crate::agent::types::{AgentEvent, Message, ToolCall, Turn};
use crate::config::Config;
use crate::error::Result;
use crate::session::backup::BackupManager;
use crate::session::store::SessionStore;
use crate::tools::ToolRegistry;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

pub struct AgentLoop {
    workspace_root: PathBuf,
    config: Config,
    provider: Box<dyn Provider>,
    session_store: SessionStore,
    session_id: String,
    backup_manager: BackupManager,
    messages: Vec<Message>,
    current_turn_id: usize,
}

impl AgentLoop {
    pub fn new(workspace_root: &Path, config: Config, provider: Box<dyn Provider>) -> Self {
        let session_store = SessionStore::new();
        let session_id = session_store
            .create_session(workspace_root)
            .unwrap_or_else(|_| "ephemeral-session".to_string());

        Self {
            workspace_root: workspace_root.to_path_buf(),
            config,
            provider,
            session_store,
            session_id,
            backup_manager: BackupManager::new(workspace_root),
            messages: Vec::new(),
            current_turn_id: 0,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Executes a single interactive or autonomous turn with the ReAct tool-use loop.
    /// Emits structured `AgentEvent`s over the provided MPSC channel for UI or NDJSON rendering.
    pub async fn execute_turn(
        &mut self,
        user_prompt: &str,
        event_sender: mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<Turn> {
        self.current_turn_id += 1;
        let turn_id = self.current_turn_id;

        // 1. Append user prompt
        self.messages.push(Message::user(user_prompt));

        let system_prompt = PromptBuilder::build_system_prompt(&self.workspace_root, None);
        let tools = ToolRegistry::get_tool_schemas();

        let start_event = AgentEvent::TurnStart {
            turn_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            model: self.config.provider.model.clone(),
            context_tokens: 0,
        };
        self.session_store
            .append_event(&self.session_id, &start_event)
            .ok();
        event_sender.send(start_event)?;

        let mut turn_response = String::new();
        let mut turn_tool_calls = Vec::new();
        let mut turn_tool_results = Vec::new();
        let mut turn_tokens_used = 0;
        let mut turn_files_modified = Vec::new();

        let max_iterations = 10; // Prevent infinite tool loops
        let mut iteration = 0;

        let options = CompletionOptions {
            model: self.config.provider.model.clone(),
            temperature: self.config.provider.temperature,
            max_tokens: self.config.provider.max_tokens,
            system_instruction: Some(system_prompt),
        };

        let mut retry_count = 0;
        let max_retries = 3;

        while iteration < max_iterations {
            iteration += 1;

            let mut iteration_text = String::new();
            let mut pending_tool_calls: Vec<ToolCall> = Vec::new();

            let mut success = false;
            while !success && retry_count <= max_retries {
                let mut stream = match self
                    .provider
                    .stream_completion(&self.messages, &tools, &options)
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        if retry_count < max_retries {
                            retry_count += 1;
                            let delay = std::time::Duration::from_secs(retry_count * 2);
                            tracing::warn!(
                                "Rate limit or connection issue. Retrying in {:?} (attempt {}/{})",
                                delay,
                                retry_count,
                                max_retries
                            );
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                        return Err(e);
                    }
                };

                let mut stream_error = None;
                while let Some(chunk_res) = stream.next().await {
                    match chunk_res {
                        Ok(StreamChunk::Delta(delta)) => {
                            iteration_text.push_str(&delta);
                            turn_response.push_str(&delta);
                            let delta_event = AgentEvent::StreamDelta { turn_id, delta };
                            event_sender.send(delta_event)?;
                        }
                        Ok(StreamChunk::ToolCallChunk(tool_call)) => {
                            let call_event = AgentEvent::ToolCall {
                                turn_id,
                                tool_id: tool_call.id.clone(),
                                tool: tool_call.name.clone(),
                                args: tool_call.arguments.clone(),
                            };
                            self.session_store
                                .append_event(&self.session_id, &call_event)
                                .ok();
                            event_sender.send(call_event)?;
                            pending_tool_calls.push(tool_call);
                        }
                        Ok(StreamChunk::Usage {
                            prompt_tokens,
                            completion_tokens,
                        }) => {
                            turn_tokens_used = prompt_tokens + completion_tokens;
                        }
                        Ok(StreamChunk::Done) => {
                            break;
                        }
                        Err(e) => {
                            stream_error = Some(e);
                            break;
                        }
                    }
                }

                if let Some(err) = stream_error {
                    if retry_count < max_retries {
                        retry_count += 1;
                        let delay_secs = retry_count * 3;
                        let retry_msg = format!(
                            "Rate limit reached. Retrying in {}s (attempt {}/{})...",
                            delay_secs, retry_count, max_retries
                        );
                        let event = AgentEvent::Error {
                            turn_id: Some(turn_id),
                            code: "rate_limited".to_string(),
                            message: retry_msg,
                            retrying: true,
                            retry_after_ms: Some(delay_secs * 1000),
                        };
                        event_sender.send(event).ok();
                        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                        continue;
                    }
                    return Err(err);
                }

                success = true;
            }

            // If assistant produced tool calls, execute them and continue ReAct loop
            if !pending_tool_calls.is_empty() {
                self.messages.push(Message::assistant_with_tools(
                    iteration_text,
                    pending_tool_calls.clone(),
                ));

                for tool_call in pending_tool_calls {
                    turn_tool_calls.push(tool_call.clone());

                    // Check if file modification tool to record file
                    if tool_call.name == "write_file" || tool_call.name == "patch_file" {
                        if let Some(path) = tool_call.arguments.get("path").and_then(|p| p.as_str())
                        {
                            turn_files_modified.push(path.to_string());
                        }
                    }

                    // Execute tool
                    let tool_result = ToolRegistry::dispatch(
                        &self.workspace_root,
                        &tool_call.id,
                        &tool_call.name,
                        &tool_call.arguments,
                        Some(&self.backup_manager),
                        turn_id,
                    )
                    .await;

                    let res_event = AgentEvent::ToolResult {
                        turn_id,
                        tool_id: tool_result.tool_id.clone(),
                        tool: tool_result.tool_name.clone(),
                        success: tool_result.success,
                        output: tool_result.output.clone(),
                        duration_ms: tool_result.duration_ms,
                    };
                    self.session_store
                        .append_event(&self.session_id, &res_event)
                        .ok();
                    event_sender.send(res_event)?;

                    // Append tool result message for LLM context
                    self.messages.push(Message::tool_result(
                        tool_call.name,
                        tool_result.output.clone(),
                    ));

                    turn_tool_results.push(tool_result);
                }
            } else {
                // No more tool calls; assistant finished turn
                self.messages.push(Message::assistant(iteration_text));
                break;
            }
        }

        let end_event = AgentEvent::TurnEnd {
            turn_id,
            status: "complete".to_string(),
            total_tokens_used: turn_tokens_used,
            files_modified: turn_files_modified.clone(),
        };
        self.session_store
            .append_event(&self.session_id, &end_event)
            .ok();
        event_sender.send(end_event)?;

        let turn = Turn {
            turn_id,
            user_prompt: user_prompt.to_string(),
            assistant_response: turn_response,
            tool_calls: turn_tool_calls,
            tool_results: turn_tool_results,
            tokens_used: turn_tokens_used,
            files_modified: turn_files_modified,
        };

        Ok(turn)
    }
}
