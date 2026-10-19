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
    pub swarm_context: Option<(String, String, PathBuf)>,
}

#[allow(dead_code)]
impl SubagentWorker {
    pub fn new(id: String, prompt: String, config: SubagentConfig, workspace_root: &Path) -> Self {
        let mut info_val = SubagentInfo::new(id, config.role, prompt);
        info_val.todos = config.todos.clone();
        info_val.file_boundaries = config.file_boundaries.clone();
        info_val.inactivity_timeout_secs = config.inactivity_timeout_secs.max(15);
        info_val.token_budget = config.token_budget.max(4096);
        info_val.max_turns = config.max_turns.max(1);
        let info = Arc::new(RwLock::new(info_val));

        Self {
            info,
            config,
            workspace_root: workspace_root.to_path_buf(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            swarm_context: None,
        }
    }

    /// Attaches Swarm wave coordination context `(swarm_id, task_id, swarm_dir)` for peer messaging.
    pub fn with_swarm_context(
        mut self,
        swarm_id: String,
        task_id: String,
        swarm_dir: PathBuf,
    ) -> Self {
        self.swarm_context = Some((swarm_id, task_id, swarm_dir));
        self
    }

    /// Generates the specialized system prompt for the `minipub` worker's role, including
    /// any Orchestrator-assigned todo checklist and file boundaries.
    pub fn build_system_prompt(&self) -> String {
        let base = if let Some(custom) = &self.config.system_prompt_override {
            custom.clone()
        } else {
            match &self.config.role {
                SubagentRole::Scout => {
                    "You are a fast, lightweight MiniPub (`minipub`) Scout Subagent spawned by the `minicode` Orchestrator. Your mission is to explore the codebase or online documentation thoroughly and answer the Orchestrator's research request. You have READ-ONLY exploration tools plus `kit_skill_show(name)`, `kit_info(name)`, `block_search`, and `scratchpad_write`. Be concise, precise, cite exact file paths and line numbers, and return a high-signal boomerang summary to the Orchestrator.".to_string()
                }
                SubagentRole::Reviewer => {
                    "You are a Senior MiniPub (`minipub`) Reviewer Subagent spawned by the `minicode` Orchestrator. Your mission is to evaluate code changes, architecture, type contracts, and standards adherence. You can verify technology standards against `kit_skill_show(name)` and inspect live pages with `browser_inspect`/`browser_console`. Look for bugs, performance anti-patterns, missing error handling, and convention violations. Return actionable findings to the Orchestrator.".to_string()
                }
                SubagentRole::Tester => {
                    "You are a QA & Test Engineer MiniPub (`minipub`) Subagent spawned by the `minicode` Orchestrator. Your mission is to run test suites, analyze test failures, reproduce edge cases, and verify fixes. Report exact test commands run and pass/fail outcomes back to the Orchestrator.".to_string()
                }
                SubagentRole::Coder => {
                    "You are a specialized MiniPub (`minipub`) Coder Subagent spawned by the `minicode` Orchestrator to execute a scoped implementation task. Fulfill the Orchestrator's instructions accurately and concisely. You have access to MiniKit (`kit_skill_show`, `kit_info`, `kit_add`, `kit_stack_diff`) and MiniBlocks (`block_search`, `block_get`, `block_palettes`). Verify your changes before concluding and return a concise boomerang summary of modified files and completed todos to the Orchestrator.".to_string()
                }
            }
        };

        let mut full_prompt = base;

        if !self.config.todos.is_empty() {
            full_prompt.push_str("\n\n<assigned_minipub_todos>\nThe `minicode` Orchestrator assigned you the following Todo Checklist. Execute each item in order, communicate key milestones via `send_worker_message` or `scratchpad_write` if needed, and explicitly check off `[x]` each completed item in your final response:\n");
            for item in &self.config.todos {
                full_prompt.push_str(&format!("- [ ] {}. {}\n", item.id, item.task));
            }
            full_prompt.push_str("</assigned_minipub_todos>");
        }

        if !self.config.file_boundaries.is_empty() {
            full_prompt.push_str("\n\n<assigned_file_boundaries>\nYou are operating under strict cooperative file boundaries. You may read any file in the workspace for context, but you MUST ONLY create, write, or patch files within these paths:\n");
            for b in &self.config.file_boundaries {
                full_prompt.push_str(&format!("- `{}`\n", b));
            }
            full_prompt.push_str("</assigned_file_boundaries>");
        }

        full_prompt
    }

    /// Executes the `minipub` subagent loop and returns the final boomerang result
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

            tracing::info!(minipub_id = %id, role = ?role, "Starting minipub subagent worker");

            let system_prompt = self.build_system_prompt();
            let mut messages = vec![
                Message::system(system_prompt.clone()),
                Message::user(prompt.clone()),
            ];

            // Filter tools to role's capability whitelist using cached global schema registry
            let filtered_tools: Vec<_> = get_cached_tool_schemas()
                .iter()
                .filter(|schema| self.config.tool_whitelist.contains(&schema.name))
                .cloned()
                .collect();

            let model_name = self
                .config
                .model
                .clone()
                .unwrap_or_else(|| provider.default_model().to_string());

            let configured_max_tokens =
                crate::config::Config::load(Some(&self.workspace_root), None)
                    .map(|c| c.provider.max_tokens.clamp(4096, 16384))
                    .unwrap_or(8192);

            let options = CompletionOptions {
                model: model_name,
                temperature: 0.2,
                max_tokens: configured_max_tokens,
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
            let mut last_seen_swarm_msg_id: Option<String> = None;
            let mut seen_global_msg_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut transcript = crate::agent::subagent::transcript::SubagentTranscript::new(
                &id,
                role,
                prompt.clone(),
            );

            loop {
                let (live_max_turns, live_token_budget, live_timeout_secs, drained_steers) = {
                    let mut info = self.info.write().await;
                    let mt = if info.max_turns > 0 {
                        info.max_turns
                    } else {
                        self.config.max_turns
                    };
                    let tb = if info.token_budget > 0 {
                        info.token_budget
                    } else {
                        self.config.token_budget
                    };
                    let to = if info.inactivity_timeout_secs > 0 {
                        info.inactivity_timeout_secs
                    } else {
                        self.config.inactivity_timeout_secs
                    };
                    let steers: Vec<String> = std::mem::take(&mut info.pending_steer_messages);
                    (mt, tb, to, steers)
                };

                if turns_executed >= live_max_turns {
                    break;
                }

                for steer in drained_steers {
                    messages.push(Message::user(format!(
                        "[ORCHESTRATOR LIVE UPDATE / STEER]: {}",
                        steer
                    )));
                }

                // Deliver unread peer messages from durable SwarmMessageBus (if running in a swarm wave)
                if let Some((_, ref task_id, ref swarm_dir)) = self.swarm_context {
                    if let Ok(bus) = crate::agent::swarm::bus::SwarmMessageBus::new(swarm_dir) {
                        if let Ok(unread) =
                            bus.read_unread(task_id, last_seen_swarm_msg_id.as_deref())
                        {
                            if !unread.is_empty() {
                                if let Some(last) = unread.last() {
                                    last_seen_swarm_msg_id = Some(last.id.clone());
                                }
                                let formatted = unread
                                    .iter()
                                    .map(|m| m.format_for_prompt())
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                messages.push(Message::user(format!(
                                    "[SWARM PEER MESSAGES]:\n{}",
                                    formatted
                                )));
                            }
                        }
                    }
                }

                // Deliver unread peer messages from the global in-memory worker message bus
                let global_inbox = crate::agent::subagent::get_global_message_bus().read_inbox(&id);
                let mut new_global_msgs = Vec::new();
                for msg in global_inbox {
                    if seen_global_msg_ids.insert(msg.id.clone()) {
                        new_global_msgs.push(format!(
                            "<peer_message id=\"{}\" from=\"{}\" topic=\"{}\">\n{}\n</peer_message>",
                            msg.id, msg.from_worker_id, msg.topic, msg.payload
                        ));
                    }
                }
                if !new_global_msgs.is_empty() {
                    messages.push(Message::user(format!(
                        "[PEER WORKER MESSAGES]:\n{}",
                        new_global_msgs.join("\n")
                    )));
                }

                let inactivity_timeout = std::time::Duration::from_secs(live_timeout_secs.max(15));

                if self.cancel_flag.load(Ordering::SeqCst) {
                    let now = now_unix_secs();

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
                    info.last_activity_secs = now;
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
                    info.last_activity_secs = now_unix_secs();
                    // Advance progressive todo indicator based on turn progression
                    if !info.todos.is_empty() && turns_executed > 1 {
                        let completed_idx =
                            ((turns_executed - 1) * info.todos.len()) / live_max_turns.max(1);
                        for (idx, todo) in info.todos.iter_mut().enumerate() {
                            if idx < completed_idx {
                                todo.completed = true;
                            }
                        }
                    }
                    info.status_message = Some(format!(
                        "Generating response (turn {}/{})",
                        turns_executed, live_max_turns
                    ));
                }

                // Estimate prompt tokens using process-wide cached BPE tokenizer
                let prompt_tokens = estimate_prompt_tokens(&messages);
                tokens_used += prompt_tokens;
                {
                    let mut info = self.info.write().await;
                    info.tokens_used = tokens_used;
                }

                if tokens_used >= live_token_budget {
                    tracing::warn!(minipub_id = %id, tokens_used, "MiniPub token budget exceeded");
                    final_summary.push_str("\n\n_[Notice: Token budget reached maximum limit]_");
                    break;
                }

                // Stream completions from provider guarded by activity-heartbeat timeout
                let stream_res = tokio::time::timeout(
                    inactivity_timeout,
                    provider.stream_completion(&messages, &filtered_tools, &options),
                )
                .await;

                let mut stream = match stream_res {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        tracing::error!(minipub_id = %id, error = ?e, "MiniPub stream error");
                        success = false;
                        final_summary = format!("Model completion error: {}", e);
                        break;
                    }
                    Err(_) => {
                        tracing::warn!(
                            minipub_id = %id,
                            inactivity_timeout_secs = inactivity_timeout.as_secs(),
                            "MiniPub timed out waiting for initial model response"
                        );
                        success = false;
                        final_summary = format!(
                            "MiniPub worker timed out after {}s of model inactivity.",
                            inactivity_timeout.as_secs()
                        );
                        break;
                    }
                };

                let mut iteration_text = String::new();
                let mut pending_tool_calls: Vec<ToolCall> = Vec::new();
                let mut stalled = false;

                loop {
                    if self.cancel_flag.load(Ordering::SeqCst) {
                        break;
                    }

                    let next_chunk = tokio::time::timeout(inactivity_timeout, stream.next()).await;
                    let chunk_opt = match next_chunk {
                        Ok(opt) => opt,
                        Err(_) => {
                            tracing::warn!(
                                minipub_id = %id,
                                inactivity_timeout_secs = inactivity_timeout.as_secs(),
                                "MiniPub stream stalled between tokens"
                            );
                            stalled = true;
                            break;
                        }
                    };

                    let Some(chunk_res) = chunk_opt else {
                        break;
                    };

                    match chunk_res {
                        Ok(StreamChunk::Delta(delta)) => {
                            iteration_text.push_str(&delta);
                            if let Ok(mut info) = self.info.try_write() {
                                info.last_activity_secs = now_unix_secs();
                            }
                        }
                        Ok(StreamChunk::ToolCallChunk(tc)) => {
                            pending_tool_calls.push(tc);
                            if let Ok(mut info) = self.info.try_write() {
                                info.last_activity_secs = now_unix_secs();
                            }
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
                            tracing::warn!(minipub_id = %id, error = ?e, "Error chunk in minipub stream");
                            break;
                        }
                    }
                }

                if stalled {
                    success = false;
                    if iteration_text.is_empty() {
                        final_summary = format!(
                            "MiniPub stream stalled for >{}s without activity.",
                            inactivity_timeout.as_secs()
                        );
                    } else {
                        final_summary = format!(
                            "{}\n\n_[Aborted after {}s stream stall]_",
                            iteration_text,
                            inactivity_timeout.as_secs()
                        );
                    }
                    break;
                }

                final_summary = iteration_text.clone();
                {
                    let mut info = self.info.write().await;
                    sync_todos_from_activity(&mut info.todos, &iteration_text, &files_modified);
                }

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
                    let mut args = tool_call.arguments;
                    let call_id = tool_call.id.clone();

                    // Inject thread-safe worker & swarm identity into peer messaging tools
                    if tool_name == "send_worker_message" {
                        if let Some(obj) = args.as_object_mut() {
                            if !obj.contains_key("from_worker_id") {
                                let sender = self
                                    .swarm_context
                                    .as_ref()
                                    .map(|(_, tid, _)| tid.clone())
                                    .unwrap_or_else(|| id.clone());
                                obj.insert(
                                    "from_worker_id".to_string(),
                                    serde_json::Value::String(sender),
                                );
                            }
                            if let Some((ref s_id, _, ref s_dir)) = self.swarm_context {
                                obj.insert(
                                    "_swarm_id".to_string(),
                                    serde_json::Value::String(s_id.clone()),
                                );
                                obj.insert(
                                    "_swarm_dir".to_string(),
                                    serde_json::Value::String(s_dir.to_string_lossy().to_string()),
                                );
                            }
                        }
                    } else if tool_name == "read_worker_messages" {
                        if let Some(obj) = args.as_object_mut() {
                            if !obj.contains_key("worker_id") {
                                obj.insert(
                                    "worker_id".to_string(),
                                    serde_json::Value::String(id.clone()),
                                );
                            }
                        }
                    }

                    // Enforce Capability Whitelist
                    if !self.config.tool_whitelist.contains(&tool_name) {
                        tracing::warn!(
                            minipub_id = %id,
                            tool = %tool_name,
                            "Blocked tool execution outside whitelist"
                        );
                        let err_msg = format!(
                            "Error: Tool '{}' is strictly prohibited for minipub role '{}'. Allowed tools: {:?}",
                            tool_name,
                            role.badge(),
                            self.config.tool_whitelist
                        );
                        messages.push(Message::tool_result(call_id, tool_name, err_msg));
                        continue;
                    }

                    // Enforce Cooperative File Boundaries on mutating file tools
                    if let Some(path_val) = args.get("path").or_else(|| args.get("file_path")) {
                        if let Some(p) = path_val.as_str() {
                            if tool_name == "write_file" || tool_name == "patch_file" {
                                let live_boundaries = {
                                    let info = self.info.read().await;
                                    info.file_boundaries.clone()
                                };
                                if let Err(boundary_err) =
                                    crate::sandbox::worktree::FileLeaseManager::verify_write_allowed(
                                        &live_boundaries,
                                        &self.workspace_root,
                                        p,
                                    )
                                {
                                    tracing::warn!(
                                        minipub_id = %id,
                                        path = %p,
                                        reason = %boundary_err,
                                        "Blocked file write outside assigned file_boundaries"
                                    );
                                    messages.push(Message::tool_result(
                                        call_id,
                                        tool_name,
                                        format!("Error: {}", boundary_err),
                                    ));
                                    continue;
                                }
                                if !files_modified.contains(&p.to_string()) {
                                    files_modified.push(p.to_string());
                                }
                            } else if !files_inspected.contains(&p.to_string()) {
                                files_inspected.push(p.to_string());
                            }
                        }
                    }

                    // Update live tool telemetry & heartbeat
                    {
                        let mut info = self.info.write().await;
                        info.current_tool = Some(tool_name.clone());
                        info.last_activity_secs = now_unix_secs();
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

                    // Reset tool telemetry after execution & update activity heartbeat
                    {
                        let mut info = self.info.write().await;
                        info.current_tool = None;
                        info.last_activity_secs = now_unix_secs();
                        info.status_message = Some(format!("Completed `{}`", tool_name));
                        sync_todos_from_activity(
                            &mut info.todos,
                            &tool_res.output,
                            &files_modified,
                        );
                    }

                    messages.push(Message::tool_result(call_id, tool_name, tool_res.output));
                }
            }

            let now = now_unix_secs();

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

            let was_cancelled = self.cancel_flag.load(Ordering::SeqCst);
            {
                let mut info = self.info.write().await;
                let already_terminated = matches!(info.state, SubagentState::Terminated);
                if was_cancelled || already_terminated {
                    success = false;
                    info.state = SubagentState::Terminated;
                    info.status_message = Some("Canceled by user".to_string());
                } else {
                    info.state = if success {
                        SubagentState::Completed
                    } else {
                        SubagentState::Failed(markdown_report.clone())
                    };
                    if success {
                        for todo in &mut info.todos {
                            todo.completed = true;
                        }
                    }
                    info.status_message = Some(if success {
                        "Completed".to_string()
                    } else {
                        "Failed".to_string()
                    });
                }
                info.finished_at_secs = Some(now);
                info.last_activity_secs = now;
                info.tokens_used = tokens_used;
                info.turns_executed = turns_executed;
                info.current_tool = None;
                info.final_summary = Some(markdown_report.clone());
            }

            // Auto-deposit synthesized report into shared scratchpad
            let scratchpad = crate::agent::subagent::get_global_scratchpad();
            let report_title = format!("MiniPub {} ({}) Report", id, role.badge());
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
            scratchpad.write_entry(
                &format!("minipub/{}", id),
                &report_title,
                &markdown_report,
                &id,
            );
            let _ = scratchpad.save_to_disk(&self.workspace_root);

            tracing::info!(
                minipub_id = %id,
                turns = turns_executed,
                tokens = tokens_used,
                steps = transcript.steps.len(),
                success,
                "MiniPub subagent worker finished"
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

/// First-class alias for `SubagentWorker`
#[allow(dead_code)]
pub type MiniPubWorker = SubagentWorker;

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn get_cached_tool_schemas() -> &'static [crate::agent::provider::ToolSchema] {
    static CACHED_SCHEMAS: std::sync::OnceLock<Vec<crate::agent::provider::ToolSchema>> =
        std::sync::OnceLock::new();
    CACHED_SCHEMAS.get_or_init(crate::tools::ToolRegistry::get_tool_schemas)
}

fn estimate_prompt_tokens(messages: &[Message]) -> usize {
    static GLOBAL_BPE: std::sync::OnceLock<Option<tiktoken_rs::CoreBPE>> =
        std::sync::OnceLock::new();
    let prompt_str: String = messages
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let bpe_opt = GLOBAL_BPE.get_or_init(|| tiktoken_rs::cl100k_base().ok());
    if let Some(bpe) = bpe_opt {
        bpe.encode_with_special_tokens(&prompt_str).len()
    } else {
        prompt_str.len() / 4
    }
}

/// Checks off assigned `MiniPub` todo items when the worker explicitly marks `[x]` / `✔`
/// in its output or modifies a file referenced in the todo description.
fn sync_todos_from_activity(
    todos: &mut [super::types::MiniPubTodoItem],
    text: &str,
    files_modified: &[String],
) {
    if todos.is_empty() {
        return;
    }
    let lower_text = text.to_lowercase();
    for todo in todos.iter_mut() {
        if todo.completed {
            continue;
        }
        let id_checked = lower_text.contains(&format!("[x] {}.", todo.id))
            || lower_text.contains(&format!("[x] #{}", todo.id))
            || lower_text.contains(&format!("✔ {}.", todo.id));
        if id_checked {
            todo.completed = true;
            continue;
        }

        let task_lower = todo.task.to_lowercase();
        let snippet: String = task_lower.chars().take(28).collect();
        if !snippet.is_empty() {
            for line in lower_text.lines() {
                if (line.contains("[x]") || line.contains('✔')) && line.contains(&snippet) {
                    todo.completed = true;
                    break;
                }
            }
        }
        if todo.completed {
            continue;
        }

        // Also check if any modified file path is explicitly mentioned in the todo task
        for modified in files_modified {
            let mod_clean = modified.trim_start_matches("./");
            if !mod_clean.is_empty() && todo.task.contains(mod_clean) {
                todo.completed = true;
                break;
            }
        }
    }
}
