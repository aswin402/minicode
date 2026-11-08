//! Session state, context compaction, event hydration, turn rollback, and helper methods for `AgentLoop`.

use super::{classify_mandatory_approval, AgentLoop};
use crate::agent::provider::{CompletionOptions, Provider, StreamChunk};
use crate::agent::types::{AgentEvent, Message, ToolCall};
use crate::config::Config;
use crate::constants::CONTEXT_MIN_PRESERVED_MESSAGES;
use crate::error::Result;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

impl AgentLoop {
    /// Returns a reference to the reactive agent-to-agent mailbox, if available.
    #[allow(dead_code)]
    pub fn mailbox(&self) -> Option<&crate::agent::subagent::mailbox::AgentMailbox> {
        self.mailbox.as_ref()
    }

    /// Returns a handle to the in-flight approval registry for hosts (TUI/NDJSON).
    pub fn approval_registry(&self) -> super::super::types::ApprovalRegistry {
        self.pending_approvals.clone()
    }

    /// Returns a handle to the in-flight inquiry registry for hosts (TUI/NDJSON).
    pub fn inquiry_registry(&self) -> crate::agent::inquiry::InquiryRegistry {
        self.pending_inquiries.clone()
    }

    /// Disables blocking approval waits (headless mode): dangerous tools are
    /// rejected immediately unless `config.agent.auto_approve` is set.
    pub fn set_interactive_approvals(&mut self, enabled: bool) {
        self.interactive_approvals = enabled;
    }

    /// Returns `Some(reason)` when this tool call must pause for an interactive user approval decision.
    /// Critical remote-mutating or destructive operations (`git push`, `git pull`, `git reset --hard`,
    /// `git clean -f`, `gh pr merge`, `cargo/npm publish`, etc.) ALWAYS require explicit permission
    /// in interactive mode, even when `config.agent.auto_approve` ("Allow session") is enabled.
    pub(super) fn requires_approval(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> Option<String> {
        if !self.interactive_approvals {
            return None;
        }
        if let Some(reason) = classify_mandatory_approval(tool_name, args) {
            return Some(reason);
        }
        if !self.config.agent.auto_approve
            && crate::constants::APPROVAL_REQUIRED_TOOLS.contains(&tool_name)
        {
            return Some("requires user approval in strict mode".to_string());
        }
        None
    }

    #[allow(dead_code)]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[allow(dead_code)]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    #[allow(dead_code)]
    pub fn messages_mut(&mut self) -> &mut Vec<Message> {
        &mut self.messages
    }

    /// Returns the total cumulative tokens consumed across all turns in this session.
    #[allow(dead_code)]
    #[must_use]
    pub fn cumulative_tokens_used(&self) -> usize {
        self.cumulative_tokens_used
    }

    /// Returns the current turn index of the agent loop.
    #[allow(dead_code)]
    #[must_use]
    pub fn current_turn_id(&self) -> usize {
        self.current_turn_id
    }

    /// Returns the active session memory anchor.
    #[allow(dead_code)]
    #[must_use]
    pub fn memory_anchor(&self) -> &crate::context::auto_compact::MemoryAnchor {
        self.compactor.anchor()
    }

    /// Returns the active working set of accessed files.
    #[allow(dead_code)]
    #[must_use]
    pub fn active_working_set(&self) -> &std::collections::VecDeque<String> {
        &self.active_working_set
    }

    /// Returns the living execution ledger and goal anchor, if active.
    #[allow(dead_code)]
    pub fn intent_ledger(&self) -> Option<&crate::context::memory::intent::IntentLedger> {
        self.intent_ledger.as_ref()
    }

    /// Returns a mutable reference to the living execution ledger, if active.
    #[allow(dead_code)]
    pub fn intent_ledger_mut(
        &mut self,
    ) -> Option<&mut crate::context::memory::intent::IntentLedger> {
        self.intent_ledger.as_mut()
    }

    /// Records a file path into the active working set (Zone 3 Recency).
    pub fn record_file_access(&mut self, file_path: &str) {
        let trimmed = file_path.trim();
        if trimmed.is_empty() {
            return;
        }
        self.active_working_set.retain(|p| p != trimmed);
        self.active_working_set.push_front(trimmed.to_string());
        if self.active_working_set.len() > 10 {
            self.active_working_set.pop_back();
        }
    }

    /// Formats live user steering prompt(s) submitted while the orchestrator, `minitask`,
    /// or `MiniPub` background workers are running, enriched with live `minitask` and `MiniPub`
    /// telemetry plus the active `todo.md` / `task_plan.md` continuation state.
    pub(super) async fn build_live_steering_block(&self, prompts: &[String]) -> String {
        let wm = crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
        let joined_prompts = prompts.join("\n\n---\n\n");
        let active_tasks_ctx = crate::dev::registry::get_global_dev_registry()
            .format_active_tasks_context()
            .await;
        let active_minipub_ctx =
            if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                pool.format_orchestrator_context_block()
                    .await
                    .map(|b| format!("{}\n", b))
                    .unwrap_or_default()
            } else {
                String::new()
            };
        let todo_state = if let Some(plan_summary) = wm.read_active_plan_summary() {
            let remaining = plan_summary
                .total_tasks
                .saturating_sub(plan_summary.completed_tasks);
            let active_step = plan_summary
                .active_task
                .unwrap_or_else(|| "next pending task".to_string());
            format!(
                "<active_todo_continuation completed=\"{}\" total=\"{}\" remaining=\"{}\">\n  Active Step [>]: {}\n</active_todo_continuation>\n",
                plan_summary.completed_tasks, plan_summary.total_tasks, remaining, active_step
            )
        } else {
            String::new()
        };

        format!(
            "[SYSTEM - LIVE USER PROMPT RECEIVED DURING ASYNC EXECUTION]\n\
             The user submitted a new prompt while background task(s), `MiniPub` subagent(s), or orchestration steps were running:\n\
             <live_user_prompt>\n{}\n</live_user_prompt>\n\
             {}{}{}\
             <async_orchestrator_triage_protocol>\n\
             1. **Status Question / Simple Question / Read-Only Inspection / Non-Conflicting Request**: Answer or handle it directly right now using the live `<active_minipub_workers>` and `<active_minitask_processes>` telemetry above while background `MiniPub` subagents and `minitask` processes keep running uninterrupted; the runtime will automatically resume waiting and deliver their Boomerang completion reports when they finish.\n\
             2. **Independent Parallel Task**: Spawn a `MiniPub` subagent (`delegate_task` or `dispatch_subagent`, using `isolation=\"worktree\"` if file edits might overlap with an active build/test) so both workflows progress concurrently.\n\
             3. **Conflicting Direction / Pivot / Requires Stopping Active Build or Subagent**: Do NOT blindly kill running `minitask` builds or `MiniPub` subagents or abandon `todo.md`. First call `ask_user` explaining the impact on the active background work and `todo.md` roadmap so the user can approve stopping/pivoting or queuing it for after completion.\n\
             </async_orchestrator_triage_protocol>",
            joined_prompts, active_minipub_ctx, active_tasks_ctx, todo_state
        )
    }

    pub fn update_config(&mut self, config: Config, provider: Box<dyn Provider>) {
        let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::from(provider);
        crate::agent::subagent::sync_live_orchestrator_runtime(
            &self.workspace_root,
            std::sync::Arc::clone(&provider),
            &config.provider.model,
            None,
        );
        self.config = config;
        self.provider = provider;
        self.compactor.update_model(
            &self.config.provider.model,
            self.config.provider.context_window,
        );
        crate::context::budget::donut::set_active_context_limit(self.compactor.model_token_limit());
        crate::tools::exec::set_configured_exec_timeout(self.config.agent.timeout);
        self.speculative_executor = crate::agent::speculative::SpeculativeExecutor::new(
            self.workspace_root.clone(),
            self.config.agent.max_parallel_tools,
            self.config.agent.speculative_execution,
            self.config.agent.parallel_tools,
        );
        tracing::info!(
            provider = %self.config.provider.default,
            model = %self.config.provider.model,
            context_limit = self.compactor.model_token_limit(),
            "Updated agent configuration at runtime"
        );
    }

    /// Runs a single non-streaming LLM iteration: calls completion(), collects all
    /// tool calls and usage metadata internally, then emits aggregated events.
    pub(super) async fn run_nonstreaming_iteration(
        &self,
        messages: &[Message],
        tools: &[crate::agent::provider::ToolSchema],
        options: &CompletionOptions,
        event_sender: &mpsc::UnboundedSender<AgentEvent>,
        session_id: &str,
        turn_id: usize,
    ) -> Result<(String, Vec<ToolCall>, usize, usize)> {
        let mut stream = self
            .provider
            .stream_completion(messages, tools, options)
            .await?;

        let mut text = String::new();
        let mut pending_tool_calls = Vec::new();
        let mut last_prompt_tokens = 0_usize;
        let mut cumulative_completion_tokens = 0_usize;

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(StreamChunk::Delta(delta)) => {
                    text.push_str(&delta);
                }
                Ok(StreamChunk::ToolCallProgress { .. }) => {}
                Ok(StreamChunk::ToolCallChunk(tc)) => {
                    pending_tool_calls.push(tc);
                }
                Ok(StreamChunk::Usage {
                    prompt_tokens,
                    completion_tokens,
                    cached_prompt_tokens: _,
                }) => {
                    last_prompt_tokens = prompt_tokens;
                    cumulative_completion_tokens += completion_tokens;
                }
                Ok(StreamChunk::Done) => {
                    break;
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if (!text.is_empty() || !pending_tool_calls.is_empty())
                        && err_str.contains("stream ended")
                    {
                        break;
                    }
                    return Err(e);
                }
            }
        }

        // Emit a single StreamDelta with the full accumulated text.
        let delta_event = AgentEvent::StreamDelta {
            turn_id,
            delta: text.clone(),
        };
        let _ = self.session_store.append_event(session_id, &delta_event);
        let _ = event_sender.send(delta_event);

        // Emit ToolCall events for each detected tool call.
        for tc in &pending_tool_calls {
            let call_event = AgentEvent::ToolCall {
                turn_id,
                tool_id: tc.id.clone(),
                tool: tc.name.clone(),
                args: tc.arguments.clone(),
            };
            let _ = self.session_store.append_event(session_id, &call_event);
            let _ = event_sender.send(call_event);
        }

        Ok((
            text,
            pending_tool_calls,
            last_prompt_tokens,
            cumulative_completion_tokens,
        ))
    }

    /// Prunes conversation message history progressively via 4-tier auto-compaction,
    /// or deduplicates repetitive file reads/diagnostics across turns.
    pub fn prune_context(&mut self) -> Option<crate::context::auto_compact::CompactionMetrics> {
        // 0. Proactively strip thoughts/scratchpads from older assistant turns (keeping reasoning only on the 2 most recent)
        // This prevents O(N^2) reasoning bloat across multi-turn sessions before token thresholds are hit.
        crate::context::budget::auto_compact::AutoCompactor::strip_older_reasoning(
            &mut self.messages,
        );

        if self.messages.len() <= CONTEXT_MIN_PRESERVED_MESSAGES {
            return None;
        }

        // 1. Deduplicate identical file reads and repetitive compiler checks across turns
        crate::context::dedup::ObservationDeduplicator::deduplicate_messages(&mut self.messages);

        // 2. Smart 4-tier progressive auto-compaction
        let metrics = self
            .compactor
            .compact(&mut self.messages, self.current_turn_id);

        if let Some(ref m) = metrics {
            self.compactor
                .write_continuity_ledger(&self.workspace_root, &self.session_id);
            if m.tier >= 2 {
                // Senior Engineer Pre/PostCompact Hook: persist critical context to .minicode/NOTES.md
                let wm = crate::context::working_memory::WorkingMemory::new(&self.workspace_root);
                for d in &self.compactor.anchor().key_decisions {
                    let _ = wm.append_note(
                        crate::context::memory::working_memory::NoteSection::Invariants,
                        d,
                    );
                }
                for e in &self.compactor.anchor().unresolved_errors {
                    let _ = wm.append_note(
                        crate::context::memory::working_memory::NoteSection::NegativeKnowledge,
                        e,
                    );
                }

                // Pre-Compaction Operational Anchor: append deterministic operational state into .minicode/notepads/session_wisdom.md
                let wisdom_path = self
                    .workspace_root
                    .join(crate::constants::WORKSPACE_DIR_NAME)
                    .join(crate::constants::SESSION_WISDOM_FILE);
                if let Some(parent) = wisdom_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let anchor = self.compactor.anchor();
                let mod_files: Vec<String> =
                    anchor.files_modified.iter().take(8).cloned().collect();
                let mod_str = if mod_files.is_empty() {
                    "none".to_string()
                } else {
                    mod_files.join(", ")
                };
                let mut existing = std::fs::read_to_string(&wisdom_path).unwrap_or_else(|_| {
                    "# MiniPub Session Wisdom Notepad\n\n> Automatically accumulated across MiniPub subagent runs in this workspace.\n\n".to_string()
                });
                existing.push_str(&format!(
                    "\n#### Pre-Compaction Operational Checkpoint (Turn #{}, Tier {})\n- **Active Files Modified**: `{}`\n",
                    self.current_turn_id, m.tier, mod_str
                ));
                if let Some(last_dec) = anchor.key_decisions.last() {
                    existing.push_str(&format!("- **Latest Decision**: {}\n", last_dec));
                }
                if existing.len() > 6_000 {
                    let keep_from = existing.floor_char_boundary(existing.len() - 4_800);
                    existing = format!(
                        "# MiniPub Session Wisdom Notepad (Recent Entries)\n\n...\n{}",
                        &existing[keep_from..]
                    );
                }
                let _ = std::fs::write(&wisdom_path, existing);
            }
        }

        metrics
    }

    /// Ensures that the persistent session file exists on disk and is registered in the
    /// runtime session registry. This is called lazily on the first user prompt or turn,
    /// preventing empty 0-event session files when the user exits from the welcome screen.
    pub fn ensure_session_persisted(&mut self) -> Result<()> {
        if !self.session_persisted {
            let session_file = self
                .session_store
                .init_session_file(&self.session_id, &self.workspace_root)?;
            self._active_guard = crate::logging::register_active_session(
                &self.session_id,
                &self.workspace_root,
                &self.config.provider.default,
                &self.config.provider.model,
                &session_file,
            )
            .ok();
            self.session_persisted = true;
            tracing::info!(session_id = %self.session_id, "Initialized persistent session file on first prompt");
        }
        Ok(())
    }

    /// Removes the session file if it only contains the initial header line (<= 1 non-empty line).
    pub(super) fn cleanup_empty_session_file(&self) {
        if self.session_persisted {
            let path = self.session_store.session_file_path(&self.session_id);
            if let Ok(content) = std::fs::read_to_string(&path) {
                let non_empty_lines = content.lines().filter(|l| !l.trim().is_empty()).count();
                if non_empty_lines <= 1 {
                    let _ = std::fs::remove_file(&path);
                    tracing::debug!(path = %path.display(), "Cleaned up placeholder session file");
                }
            }
        }
    }

    /// Rebuilds, optionally compacts, and deterministically sorts the active tool schema list.
    pub(super) fn rebuild_active_tools(
        &self,
        user_prompt: &str,
        active_categories: &std::collections::HashSet<crate::tools::category::ToolCategory>,
        mcp_tools_by_server: &std::collections::HashMap<
            String,
            Vec<crate::agent::provider::ToolSchema>,
        >,
        active_mcp_servers: &std::collections::HashSet<String>,
        active_tool_names: &std::collections::HashSet<String>,
    ) -> Vec<crate::agent::provider::ToolSchema> {
        let mut tools = crate::tools::category::assemble_active_tools_full(
            self.config.agent.tool_mode,
            user_prompt,
            active_categories,
            mcp_tools_by_server,
            active_mcp_servers,
            active_tool_names,
        );
        let active_caveman_level = if self.config.agent.caveman_mode {
            crate::agent::caveman::CavemanLevel::from_str_active(&self.config.agent.caveman_level)
        } else {
            crate::agent::caveman::read_flag_safe(&crate::agent::caveman::default_flag_path(
                &self.workspace_root,
            ))
            .unwrap_or(crate::agent::caveman::CavemanLevel::Off)
        };
        if self.config.agent.compact_tool_schemas || active_caveman_level.is_enabled() {
            tools =
                crate::context::budget::headroom::CacheAligner::compact_tool_schemas_for_pressure(
                    &tools,
                    active_caveman_level,
                );
        } else {
            crate::context::budget::headroom::CacheAligner::normalize_tool_schemas(&mut tools);
        }
        tools
    }

    /// Estimates or counts the token usage of `self.messages`.
    pub(super) fn count_current_context_tokens(&self) -> usize {
        crate::context::compressor::ContextCompressor::new()
            .ok()
            .map(|c| c.count_messages_tokens(&self.messages))
            .filter(|&t| t > 0)
            .unwrap_or_else(|| self.messages.iter().map(|m| m.content.len() / 4).sum())
    }

    /// Resets the conversation history and allocates a fresh, unpersisted session ID.
    pub fn reset_session(&mut self) {
        self.cleanup_empty_session_file();
        self.messages.clear();
        self.current_turn_id = 0;
        self.cumulative_tokens_used = 0;
        self.session_id = self.session_store.generate_session_id();
        self.session_persisted = false;
        self.last_seen_swarm_msg_id = None;
        if let Ok(mut guard) = self.last_emitted_plan_sig.lock() {
            *guard = None;
        }
        self.fetch_tracker.clear();
        self._active_guard = None;
        tracing::info!(session_id = %self.session_id, "Reset to fresh lazy session");
    }

    /// Extracts multimodal `ImageAttachment`s from `<attached_images>` blocks (`[img 1]`, `[img 2]`)
    /// or image paths referenced in the user's prompt.
    pub(super) fn extract_user_image_attachments(
        user_prompt: &str,
        workspace_root: &std::path::Path,
    ) -> Vec<crate::agent::types::ImageAttachment> {
        const IMAGE_EXTENSIONS: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".webp"];
        let mut attachments = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for token in user_prompt.split_whitespace() {
            // Match `path="..."` from `<attached_images>` or direct `.png/.jpg/.jpeg/.gif/.webp` paths
            let raw = if let Some(rest) = token.strip_prefix("path=\"") {
                rest.trim_end_matches('"')
            } else {
                token.trim_matches(|c: char| {
                    matches!(
                        c,
                        '`' | '\'' | '"' | '(' | ')' | '[' | ']' | ',' | ';' | ':' | '<' | '>'
                    )
                })
            };

            if raw.is_empty() {
                continue;
            }
            let lower = raw.to_lowercase();
            if !IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
                continue;
            }

            let candidate = std::path::Path::new(raw);
            let resolved = if candidate.is_absolute() {
                candidate.to_path_buf()
            } else {
                workspace_root.join(candidate)
            };

            if resolved.is_file() && seen.insert(resolved.clone()) {
                if let Some(att) = crate::agent::types::ImageAttachment::from_path(&resolved) {
                    attachments.push(att);
                    if attachments.len() >= 6 {
                        break;
                    }
                }
            }
        }

        attachments
    }

    /// Detects high-intensity autonomous execution cues (`ultrawork`, `ulw`, `ultrathink`, `do not stop until`)
    /// in the user prompt without requiring a manual `/command`.
    pub(crate) fn detect_ultrawork_intent(user_prompt: &str) -> bool {
        let lower = user_prompt.to_lowercase();
        if lower.contains("ultrawork")
            || lower.contains("ultrathink")
            || lower.contains("do not stop until")
            || lower.contains("don't stop until")
        {
            return true;
        }
        // Check standalone `ulw` token
        lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|tok| tok == "ulw")
    }

    /// Reconstructs the agent conversation history, turn counter, and working set
    /// from a sequence of recorded `AgentEvent`s loaded from session persistence.
    pub fn hydrate_from_events(&mut self, session_id: &str, events: &[AgentEvent]) {
        if self.session_id != session_id {
            self.cleanup_empty_session_file();
        }
        self.session_id = session_id.to_string();
        self.session_persisted = true;
        let session_file = self.session_store.session_file_path(&self.session_id);
        self._active_guard = crate::logging::register_active_session(
            &self.session_id,
            &self.workspace_root,
            &self.config.provider.default,
            &self.config.provider.model,
            &session_file,
        )
        .ok();
        self.messages.clear();
        self.active_working_set.clear();

        let mut current_delta = String::new();
        let mut pending_tool_calls: Vec<ToolCall> = Vec::new();
        let mut unanswered_tool_calls: Vec<(String, String)> = Vec::new();
        let mut max_turn_id: usize = 0;
        let mut total_tokens: usize = 0;
        let mut first_turn_prompt: Option<String> = None;

        let flush_incomplete_tool_batch =
            |messages: &mut Vec<Message>,
             current_delta: &mut String,
             pending_tool_calls: &mut Vec<ToolCall>,
             unanswered_tool_calls: &mut Vec<(String, String)>| {
                if !pending_tool_calls.is_empty() {
                    let clean = crate::agent::prompt::strip_thought_blocks(current_delta);
                    for tc in pending_tool_calls.iter() {
                        unanswered_tool_calls.push((tc.id.clone(), tc.name.clone()));
                    }
                    messages.push(Message::assistant_with_tools(
                        clean,
                        std::mem::take(pending_tool_calls),
                    ));
                    current_delta.clear();
                }
                for (id, name) in unanswered_tool_calls.drain(..) {
                    messages.push(Message::tool_result(
                        id,
                        name,
                        "[Tool execution interrupted before completion]".to_string(),
                    ));
                }
            };

        for event in events {
            match event {
                AgentEvent::UserPrompt {
                    turn_id, prompt, ..
                } => {
                    flush_incomplete_tool_batch(
                        &mut self.messages,
                        &mut current_delta,
                        &mut pending_tool_calls,
                        &mut unanswered_tool_calls,
                    );
                    // If there was an in-flight assistant response from previous turn, flush it
                    if !current_delta.is_empty() {
                        let clean = crate::agent::prompt::strip_thought_blocks(&current_delta);
                        self.messages.push(Message::assistant(clean));
                        current_delta.clear();
                    }
                    max_turn_id = max_turn_id.max(*turn_id);

                    if first_turn_prompt.is_none()
                        && crate::context::search::intent_filter::IntentClassifier::analyze_signal(
                            prompt, None,
                        )
                        .has_actionable_task_intent()
                    {
                        first_turn_prompt = Some(prompt.clone());
                    }

                    self.messages.push(Message::user(prompt.clone()));
                }
                AgentEvent::StreamDelta { delta, .. } => {
                    if !unanswered_tool_calls.is_empty() {
                        flush_incomplete_tool_batch(
                            &mut self.messages,
                            &mut current_delta,
                            &mut pending_tool_calls,
                            &mut unanswered_tool_calls,
                        );
                    }
                    current_delta.push_str(delta);
                }
                AgentEvent::ToolCall {
                    tool_id,
                    tool,
                    args,
                    ..
                } => {
                    if !unanswered_tool_calls.is_empty() {
                        flush_incomplete_tool_batch(
                            &mut self.messages,
                            &mut current_delta,
                            &mut pending_tool_calls,
                            &mut unanswered_tool_calls,
                        );
                    }
                    pending_tool_calls.push(ToolCall {
                        id: tool_id.clone(),
                        name: tool.clone(),
                        arguments: args.clone(),
                    });
                }
                AgentEvent::ToolResult {
                    tool_id,
                    tool,
                    output,
                    ..
                } => {
                    // Before the first tool result of a batch, flush the assistant message with tools
                    if !pending_tool_calls.is_empty() {
                        let clean = crate::agent::prompt::strip_thought_blocks(&current_delta);
                        for tc in pending_tool_calls.iter() {
                            unanswered_tool_calls.push((tc.id.clone(), tc.name.clone()));
                        }
                        self.messages.push(Message::assistant_with_tools(
                            clean,
                            std::mem::take(&mut pending_tool_calls),
                        ));
                        current_delta.clear();
                    }
                    unanswered_tool_calls.retain(|(id, _)| id != tool_id);

                    self.messages.push(Message::tool_result(
                        tool_id.clone(),
                        tool.clone(),
                        output.clone(),
                    ));
                }
                AgentEvent::FileModified { path, .. } => {
                    self.record_file_access(path);
                }
                AgentEvent::TurnEnd {
                    turn_id,
                    total_tokens_used,
                    files_modified,
                    ..
                } => {
                    flush_incomplete_tool_batch(
                        &mut self.messages,
                        &mut current_delta,
                        &mut pending_tool_calls,
                        &mut unanswered_tool_calls,
                    );
                    // If assistant produced final text, flush it
                    if !current_delta.is_empty() {
                        let clean = crate::agent::prompt::strip_thought_blocks(&current_delta);
                        self.messages.push(Message::assistant(clean));
                        current_delta.clear();
                    }
                    max_turn_id = max_turn_id.max(*turn_id);
                    total_tokens = total_tokens.saturating_add(*total_tokens_used);
                    for path in files_modified {
                        self.record_file_access(path);
                    }
                }
                _ => {}
            }
        }

        // Final cleanup if delta or tool calls were trailing
        flush_incomplete_tool_batch(
            &mut self.messages,
            &mut current_delta,
            &mut pending_tool_calls,
            &mut unanswered_tool_calls,
        );
        if !current_delta.is_empty() {
            let clean = crate::agent::prompt::strip_thought_blocks(&current_delta);
            self.messages.push(Message::assistant(clean));
        }

        self.current_turn_id = max_turn_id;
        self.cumulative_tokens_used = total_tokens;

        let intent_path = self
            .workspace_root
            .join(&self.config.agent.intent.persistence_file);
        if self.config.agent.intent.enabled && intent_path.exists() {
            self.intent_ledger =
                crate::context::memory::intent::IntentLedger::load_from_disk(&intent_path).ok();
        }

        if let Some(first_prompt) = first_turn_prompt {
            self.compactor.set_working_context(&first_prompt);
        }

        tracing::info!(
            session_id = %self.session_id,
            turns = self.current_turn_id,
            messages = self.messages.len(),
            working_set = self.active_working_set.len(),
            "Successfully hydrated AgentLoop from session history"
        );
    }

    /// Rolls back the agent's turn counter and truncates message history
    pub fn rollback_turn(&mut self, target_turn_id: usize, message_index: usize) {
        self.current_turn_id = target_turn_id.saturating_sub(1);
        if message_index < self.messages.len() {
            self.messages.truncate(message_index);
            tracing::info!(
                target_turn = target_turn_id,
                remaining_messages = self.messages.len(),
                "Agent conversation history rolled back"
            );
        }

        // Drop all in-flight approval senders — the conversation state that would
        // answer them no longer exists. Callers who already received ApprovalRequest
        // events will simply never receive a decision; the cancellation path handles
        // those cleanly.
        let mut guard = self
            .pending_approvals
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let count = guard.len();
        guard.clear();
        if count > 0 {
            tracing::debug!(
                cleared_approvals = count,
                "Cleared in-flight approvals during rollback"
            );
        }

        let mut inq_guard = self
            .pending_inquiries
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let inq_count = inq_guard.len();
        inq_guard.clear();
        if inq_count > 0 {
            tracing::debug!(
                cleared_inquiries = inq_count,
                "Cleared in-flight inquiries during rollback"
            );
        }
    }

    /// Removes entries whose sender half has already been dropped (e.g. host closed
    /// without responding, or turn was rolled back). Calling this at the start of
    /// each turn prevents the registry from growing unboundedly across long sessions.
    pub fn prune_stale_approvals(&self) {
        let mut guard = self
            .pending_approvals
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let before = guard.len();
        guard.retain(|_id, sender| !sender.is_closed());
        let pruned = before.saturating_sub(guard.len());
        if pruned > 0 {
            tracing::debug!(
                pruned = pruned,
                remaining = guard.len(),
                "Pruned stale approval senders from registry"
            );
        }
    }

    /// Removes inquiry entries whose sender half has already been dropped. Calling this
    /// at the start of each turn prevents the inquiry registry from growing unboundedly.
    pub fn prune_stale_inquiries(&self) {
        let mut guard = self
            .pending_inquiries
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let before = guard.len();
        guard.retain(|_id, sender| !sender.is_closed());
        let pruned = before.saturating_sub(guard.len());
        if pruned > 0 {
            tracing::debug!(
                pruned = pruned,
                remaining = guard.len(),
                "Pruned stale inquiry senders from registry"
            );
        }
    }

    /// Emits events, session records, and LLM-visible messages for a denied
    /// or cancelled tool call so function-call/result pairing stays valid.
    pub(super) fn emit_rejected_tool_result(
        &mut self,
        event_sender: &mpsc::UnboundedSender<AgentEvent>,
        turn_id: usize,
        tool_call: &ToolCall,
        reason: &str,
        turn_tool_results: &mut Vec<crate::agent::types::ToolResult>,
    ) {
        let output = format!(
            "Execution of '{}' was {}: approve via the prompt, or rerun with --yes.",
            tool_call.name, reason
        );
        let rejected = crate::agent::types::ToolResult {
            tool_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            success: false,
            output: output.clone(),
            display_output: String::new(),
            duration_ms: 0,
        };
        let res_event = AgentEvent::ToolResult {
            turn_id,
            tool_id: rejected.tool_id.clone(),
            tool: rejected.tool_name.clone(),
            success: false,
            output: output.clone(),
            duration_ms: 0,
        };
        let _ = self
            .session_store
            .append_event(&self.session_id, &res_event);
        let _ = event_sender.send(res_event);
        self.messages.push(Message::tool_result(
            tool_call.id.clone(),
            tool_call.name.clone(),
            output,
        ));
        turn_tool_results.push(rejected);
    }

    /// Checks WorkingMemory for current active execution plan tasks and emits
    /// an `AgentEvent::PlanUpdated` only when the plan state has actually changed.
    pub fn emit_current_plan(
        &self,
        turn_id: Option<usize>,
        event_sender: &mpsc::UnboundedSender<AgentEvent>,
    ) {
        let wm = crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
        if let Some(summary) = wm.read_active_plan_summary() {
            let sig = serde_json::to_string(&(
                &summary.phase_label,
                summary.total_tasks,
                summary.completed_tasks,
                &summary.active_task,
                &summary.tasks,
            ))
            .unwrap_or_default();
            if let Ok(mut last_sig) = self.last_emitted_plan_sig.lock() {
                if last_sig.as_deref() == Some(sig.as_str()) {
                    return;
                }
                *last_sig = Some(sig);
            }
            let event = AgentEvent::PlanUpdated {
                turn_id,
                phase_label: Some(summary.phase_label.clone()),
                total_tasks: summary.total_tasks,
                completed_tasks: summary.completed_tasks,
                active_task: summary.active_task,
                tasks: summary.tasks,
            };
            let _ = self.session_store.append_event(&self.session_id, &event);
            let _ = event_sender.send(event);
        } else {
            if let Ok(mut last_sig) = self.last_emitted_plan_sig.lock() {
                if last_sig.as_deref() == Some("EMPTY") {
                    return;
                }
                *last_sig = Some("EMPTY".to_string());
            }
            let event = AgentEvent::PlanUpdated {
                turn_id,
                phase_label: None,
                total_tasks: 0,
                completed_tasks: 0,
                active_task: None,
                tasks: Vec::new(),
            };
            let _ = event_sender.send(event);
        }
    }
}

impl Drop for AgentLoop {
    fn drop(&mut self) {
        self.cleanup_empty_session_file();
    }
}
