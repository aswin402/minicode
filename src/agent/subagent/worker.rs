use super::types::{SubagentConfig, SubagentInfo, SubagentResult, SubagentRole, SubagentState};
use crate::agent::provider::{CompletionOptions, Provider, StreamChunk};
use crate::agent::types::{Message, ToolCall};
use crate::error::Result;
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Runs an isolated, capability-sandboxed subagent loop
pub struct SubagentWorker {
    pub info: Arc<RwLock<SubagentInfo>>,
    pub config: SubagentConfig,
    pub workspace_root: PathBuf,
    pub cancel_flag: Arc<AtomicBool>,
}

#[allow(dead_code)]
impl SubagentWorker {
    pub fn new(id: String, prompt: String, config: SubagentConfig, workspace_root: &Path) -> Self {
        let info = Arc::new(RwLock::new(SubagentInfo::new(id, config.role, prompt)));

        Self {
            info,
            config,
            workspace_root: workspace_root.to_path_buf(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Generates the specialized system prompt for the worker's role
    pub fn build_system_prompt(&self) -> String {
        if let Some(custom) = &self.config.system_prompt_override {
            return custom.clone();
        }

        match &self.config.role {
            SubagentRole::Scout => {
                "You are an expert Scout Subagent. Your mission is to explore the codebase or online documentation thoroughly and answer the user's research request. You have READ-ONLY tools. Be concise, precise, cite exact file paths, line numbers, and return structured summaries.".to_string()
            }
            SubagentRole::Reviewer => {
                "You are a Senior Reviewer Subagent. Your mission is to evaluate code changes, architecture, type contracts, and standards adherence. Look for bugs, performance anti-patterns, missing error handling, and convention violations. Provide actionable feedback.".to_string()
            }
            SubagentRole::Tester => {
                "You are a QA & Test Engineer Subagent. Your mission is to run test suites, analyze test failures, reproduce edge cases, and ensure high test coverage.".to_string()
            }
            SubagentRole::Coder => {
                "You are a specialized Coder Subagent executing an assigned implementation task. Fulfill the user's instructions accurately and concisely.".to_string()
            }
        }
    }

    /// Executes the subagent loop and returns the final result
    pub fn run<'a>(
        &'a self,
        provider: Arc<dyn Provider>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<SubagentResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let (prompt, role, id) = {
                let info = self.info.read().await;
                (info.prompt.clone(), info.role, info.id.clone())
            };

            tracing::info!(subagent_id = %id, role = ?role, "Starting subagent worker");

            let system_prompt = self.build_system_prompt();
            let mut messages = vec![
                Message::system(system_prompt.clone()),
                Message::user(prompt.clone()),
            ];

            // Filter tools to role's capability whitelist
            let all_schemas = crate::tools::ToolRegistry::get_tool_schemas();
            let filtered_tools: Vec<_> = all_schemas
                .into_iter()
                .filter(|schema| self.config.tool_whitelist.contains(&schema.name))
                .collect();

            let model_name = self
                .config
                .model
                .clone()
                .unwrap_or_else(|| provider.default_model().to_string());

            let options = CompletionOptions {
                model: model_name,
                temperature: 0.2,
                max_tokens: 4096,
                system_instruction: Some(system_prompt),
                thinking_budget: None,
                reasoning_effort: None,
            };

            let mut files_inspected = Vec::new();
            let mut files_modified = Vec::new();
            let mut tokens_used = 0;
            let mut turns_executed = 0;
            let mut final_summary = String::new();
            let mut success = true;
            let mut step_counter = 0;
            let mut transcript = crate::agent::subagent::transcript::SubagentTranscript::new(
                &id,
                role,
                prompt.clone(),
            );

            while turns_executed < self.config.max_turns {
                if self.cancel_flag.load(Ordering::SeqCst) {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    transcript.total_tokens = tokens_used;
                    transcript.turns_executed = turns_executed;
                    transcript.finished_at_secs = Some(now);
                    crate::agent::subagent::transcript::get_global_transcript_store()
                        .store(transcript.clone(), Some(&self.workspace_root));

                    let report = crate::agent::subagent::reducer::SubagentSynthesisReducer::reduce(
                        crate::agent::subagent::reducer::SubagentSynthesisContext {
                            subagent_id: &id,
                            role: &role,
                            final_summary: "Subagent execution canceled by user",
                            success: false,
                            status: "canceled",
                            files_inspected: &files_inspected,
                            files_modified: &files_modified,
                            transcript: &transcript,
                            workspace_root: &self.workspace_root,
                            worktree_branch: None,
                        },
                    )
                    .await;
                    let report_md = report.format_markdown();

                    let mut info = self.info.write().await;
                    info.state = SubagentState::Terminated;
                    info.current_tool = None;
                    info.status_message = Some("Canceled by user".to_string());
                    info.final_summary = Some(report_md.clone());
                    return Ok(SubagentResult {
                        id: id.clone(),
                        task_id: id,
                        role,
                        success: false,
                        final_summary: report_md,
                        tokens_used,
                        turns_executed,
                        files_inspected,
                        files_modified,
                        worktree_branch: None,
                    });
                }

                turns_executed += 1;
                {
                    let mut info = self.info.write().await;
                    info.turns_executed = turns_executed;
                    info.current_tool = None;
                    info.status_message = Some(format!(
                        "Generating response (turn {}/{})",
                        turns_executed, self.config.max_turns
                    ));
                }

                // Estimate prompt tokens without holding tokenizer across await
                let prompt_tokens = estimate_prompt_tokens(&messages);
                tokens_used += prompt_tokens;
                {
                    let mut info = self.info.write().await;
                    info.tokens_used = tokens_used;
                }

                if tokens_used >= self.config.token_budget {
                    tracing::warn!(subagent_id = %id, tokens_used, "Subagent token budget exceeded");
                    final_summary.push_str("\n\n_[Notice: Token budget reached maximum limit]_");
                    break;
                }

                // Stream completions from provider
                let mut stream = match provider
                    .stream_completion(&messages, &filtered_tools, &options)
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(subagent_id = %id, error = ?e, "Subagent stream error");
                        success = false;
                        final_summary = format!("Model completion error: {}", e);
                        break;
                    }
                };

                let mut iteration_text = String::new();
                let mut pending_tool_calls: Vec<ToolCall> = Vec::new();

                while let Some(chunk_res) = stream.next().await {
                    if self.cancel_flag.load(Ordering::SeqCst) {
                        break;
                    }

                    match chunk_res {
                        Ok(StreamChunk::Delta(delta)) => {
                            iteration_text.push_str(&delta);
                        }
                        Ok(StreamChunk::ToolCallChunk(tc)) => {
                            pending_tool_calls.push(tc);
                        }
                        Ok(StreamChunk::Usage {
                            completion_tokens,
                            prompt_tokens,
                            ..
                        }) => {
                            tokens_used = prompt_tokens + completion_tokens;
                        }
                        Ok(StreamChunk::Done) => break,
                        Err(e) => {
                            tracing::warn!(subagent_id = %id, error = ?e, "Error chunk in subagent stream");
                            break;
                        }
                    }
                }

                final_summary = iteration_text.clone();

                if pending_tool_calls.is_empty() {
                    // Agent concluded with final response
                    break;
                }

                // Record assistant tool calls
                messages.push(Message::assistant_with_tools(
                    iteration_text,
                    pending_tool_calls.clone(),
                ));

                // Execute each tool call
                for tool_call in pending_tool_calls {
                    let tool_name = tool_call.name;
                    let args = tool_call.arguments;
                    let call_id = tool_call.id.clone();

                    // Enforce Capability Whitelist
                    if !self.config.tool_whitelist.contains(&tool_name) {
                        tracing::warn!(
                            subagent_id = %id,
                            tool = %tool_name,
                            "Blocked tool execution outside whitelist"
                        );
                        let err_msg = format!(
                        "Error: Tool '{}' is strictly prohibited for subagent role '{}'. Allowed tools: {:?}",
                        tool_name,
                        role.badge(),
                        self.config.tool_whitelist
                    );
                        messages.push(Message::tool_result(call_id, tool_name, err_msg));
                        continue;
                    }

                    // Track file operations
                    if let Some(path_val) = args.get("path").or_else(|| args.get("file_path")) {
                        if let Some(p) = path_val.as_str() {
                            if tool_name == "write_file" || tool_name == "patch_file" {
                                if !files_modified.contains(&p.to_string()) {
                                    files_modified.push(p.to_string());
                                }
                            } else if !files_inspected.contains(&p.to_string()) {
                                files_inspected.push(p.to_string());
                            }
                        }
                    }

                    // Update live tool telemetry
                    {
                        let mut info = self.info.write().await;
                        info.current_tool = Some(tool_name.clone());
                        info.status_message = Some(format!("Executing `{}`", tool_name));
                    }

                    let step_start = std::time::Instant::now();
                    step_counter += 1;

                    // Execute tool via ToolRegistry
                    let tool_res = crate::tools::ToolRegistry::dispatch(
                        &self.workspace_root,
                        &call_id,
                        &tool_name,
                        &args,
                        None,
                        turns_executed,
                    )
                    .await;

                    let duration_ms = step_start.elapsed().as_millis() as u64;

                    // Record step into ephemeral transcript
                    transcript.add_step(
                        crate::agent::subagent::transcript::SubagentStepRecord::new(
                            step_counter,
                            turns_executed,
                            tool_name.clone(),
                            args.clone(),
                            tool_res.output.clone(),
                            tool_res.success,
                            duration_ms,
                        ),
                    );

                    // Reset tool telemetry after execution
                    {
                        let mut info = self.info.write().await;
                        info.current_tool = None;
                        info.status_message = Some(format!("Completed `{}`", tool_name));
                    }

                    messages.push(Message::tool_result(call_id, tool_name, tool_res.output));
                }
            }

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            transcript.total_tokens = tokens_used;
            transcript.turns_executed = turns_executed;
            transcript.finished_at_secs = Some(now);
            crate::agent::subagent::transcript::get_global_transcript_store()
                .store(transcript.clone(), Some(&self.workspace_root));

            let isolate_worktree = {
                let info = self.info.read().await;
                info.isolate_worktree
            };
            let worktree_branch = if isolate_worktree {
                Some(format!("subagent/{}", id))
            } else {
                None
            };

            let report = crate::agent::subagent::reducer::SubagentSynthesisReducer::reduce(
                crate::agent::subagent::reducer::SubagentSynthesisContext {
                    subagent_id: &id,
                    role: &role,
                    final_summary: &final_summary,
                    success,
                    status: if success { "completed" } else { "failed" },
                    files_inspected: &files_inspected,
                    files_modified: &files_modified,
                    transcript: &transcript,
                    workspace_root: &self.workspace_root,
                    worktree_branch: worktree_branch.clone(),
                },
            )
            .await;
            let markdown_report = report.format_markdown();

            {
                let mut info = self.info.write().await;
                info.state = if success {
                    SubagentState::Completed
                } else {
                    SubagentState::Failed(markdown_report.clone())
                };
                info.finished_at_secs = Some(now);
                info.tokens_used = tokens_used;
                info.turns_executed = turns_executed;
                info.current_tool = None;
                info.status_message = Some(if success {
                    "Completed".to_string()
                } else {
                    "Failed".to_string()
                });
                info.final_summary = Some(markdown_report.clone());
            }

            // Auto-deposit synthesized report into shared scratchpad
            let scratchpad = crate::agent::subagent::get_global_scratchpad();
            let report_title = format!("Subagent {} ({}) Report", id, role.badge());
            scratchpad.write_entry(
                &format!("subagent_{}", id),
                &report_title,
                &markdown_report,
                &id,
            );
            scratchpad.write_entry(
                &format!("subagent/{}", id),
                &report_title,
                &markdown_report,
                &id,
            );
            let _ = scratchpad.save_to_disk(&self.workspace_root);

            tracing::info!(
                subagent_id = %id,
                turns = turns_executed,
                tokens = tokens_used,
                steps = transcript.steps.len(),
                success,
                "Subagent worker finished"
            );

            Ok(SubagentResult {
                id: id.clone(),
                task_id: id,
                role,
                success,
                final_summary: markdown_report,
                tokens_used,
                turns_executed,
                files_inspected,
                files_modified,
                worktree_branch,
            })
        })
    }
}

fn estimate_prompt_tokens(messages: &[Message]) -> usize {
    let prompt_str: String = messages
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if let Ok(bpe) = tiktoken_rs::cl100k_base() {
        bpe.encode_with_special_tokens(&prompt_str).len()
    } else {
        prompt_str.len() / 4
    }
}
