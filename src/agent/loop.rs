use crate::agent::prompt::PromptBuilder;
use crate::agent::provider::{CompletionOptions, Provider, StreamChunk};
use crate::agent::types::{AgentEvent, ApprovalDecision, Message, ToolCall, Turn};
use crate::config::Config;
use crate::constants::{
    CONTEXT_MIN_PRESERVED_MESSAGES, DEFAULT_MAX_RETRIES, DEFAULT_MAX_TOOL_ITERATIONS,
    FILE_MODIFYING_TOOLS, MCP_TOOL_PREFIX, RETRY_BACKOFF_SECS,
};
use crate::error::Result;
use crate::mcp::McpClientManager;
use crate::session::backup::BackupManager;
use crate::session::store::SessionStore;
use std::collections::HashMap;
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
    mcp_client: McpClientManager,
    /// Composable middleware pipeline applied to every tool result.
    tool_pipeline: crate::tools::middleware::ToolPipeline,
    messages: Vec<Message>,
    current_turn_id: usize,
    /// In-flight approval requests: tool_id → oneshot responder.
    pending_approvals: super::types::ApprovalRegistry,
    /// Whether a live host can answer approval requests (false in headless mode).
    interactive_approvals: bool,
    /// Smart 4-tier progressive context auto-compactor with memory anchor.
    compactor: crate::context::auto_compact::AutoCompactor,
    /// Active working set tracking recently accessed/modified files (Zone 3 Recency).
    active_working_set: std::collections::VecDeque<String>,
}

impl AgentLoop {
    pub fn new(workspace_root: &Path, config: Config, provider: Box<dyn Provider>) -> Self {
        let session_store = SessionStore::with_workspace(workspace_root);
        let session_id = session_store
            .create_session(workspace_root)
            .unwrap_or_else(|_| "ephemeral-session".to_string());

        let mcp_client = McpClientManager::new();

        // Ensure internal .minicode directories are ignored by git
        let git_service = crate::git::GitService::new(workspace_root.to_path_buf());
        git_service.ensure_git_exclude();

        let compactor = crate::context::auto_compact::AutoCompactor::new(&config.provider.model)
            .unwrap_or_else(|_| crate::context::auto_compact::AutoCompactor::default_safe());

        Self {
            workspace_root: workspace_root.to_path_buf(),
            config,
            provider,
            session_store,
            session_id,
            backup_manager: BackupManager::new(workspace_root),
            mcp_client,
            tool_pipeline: crate::tools::middleware::ToolPipeline::default(),
            messages: Vec::new(),
            current_turn_id: 0,
            pending_approvals: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            interactive_approvals: true,
            compactor,
            active_working_set: std::collections::VecDeque::with_capacity(10),
        }
    }

    /// Returns a handle to the in-flight approval registry for hosts (TUI/NDJSON).
    pub fn approval_registry(&self) -> super::types::ApprovalRegistry {
        self.pending_approvals.clone()
    }

    /// Disables blocking approval waits (headless mode): dangerous tools are
    /// rejected immediately unless `config.agent.auto_approve` is set.
    pub fn set_interactive_approvals(&mut self, enabled: bool) {
        self.interactive_approvals = enabled;
    }

    /// True when this dispatch must pause for an approval decision first.
    fn requires_approval(&self, tool_name: &str) -> bool {
        crate::constants::APPROVAL_REQUIRED_TOOLS.contains(&tool_name)
            && !self.config.agent.auto_approve
            && self.interactive_approvals
    }
    #[allow(dead_code)]
    pub fn session_id(&self) -> &str {
        &self.session_id
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

    pub fn update_config(&mut self, config: Config, provider: Box<dyn Provider>) {
        self.config = config;
        self.provider = provider;
        tracing::info!(
            provider = %self.config.provider.default,
            model = %self.config.provider.model,
            "Updated agent configuration at runtime"
        );
    }

    /// Runs a single non-streaming LLM iteration: calls completion(), collects all
    /// tool calls and usage metadata internally, then emits aggregated events.
    async fn run_nonstreaming_iteration(
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
                Ok(StreamChunk::ToolCallChunk(tc)) => {
                    pending_tool_calls.push(tc);
                }
                Ok(StreamChunk::Usage {
                    prompt_tokens,
                    completion_tokens,
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
        if self.messages.len() <= CONTEXT_MIN_PRESERVED_MESSAGES {
            return None;
        }

        // 1. Deduplicate identical file reads and repetitive compiler checks across turns
        crate::context::dedup::ObservationDeduplicator::deduplicate_messages(&mut self.messages);

        // 2. Smart 4-tier progressive auto-compaction
        self.compactor
            .compact(&mut self.messages, self.current_turn_id)
    }

    /// Executes a single interactive or autonomous turn with the ReAct tool-use loop.
    /// Emits structured `AgentEvent`s over the provided MPSC channel for UI or NDJSON rendering.
    pub async fn execute_turn(
        &mut self,
        user_prompt: &str,
        event_sender: mpsc::UnboundedSender<AgentEvent>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<Turn> {
        // Drop stale approval senders from abandoned turns before doing anything else.
        // This prevents the registry from growing unboundedly when hosts disappear
        // or turns get rolled back without draining their approval entries.
        self.prune_stale_approvals();

        self.current_turn_id += 1;
        let turn_id = self.current_turn_id;

        if turn_id == 1 {
            self.compactor.set_working_context(user_prompt);
        }

        // 1. Prune/compact conversation context if approaching budget
        let compaction_metrics = self.prune_context();
        let message_index = self.messages.len();

        // 2. Build Tri-Zone context:
        // Zone 1: Primacy Zone (100% static, immutable system prompt for cache hits)
        let system_prompt = PromptBuilder::build_static_system_prompt(&self.workspace_root, None);

        // Zone 3: Recency Zone (dynamic workspace context: git, active files, memory anchor)
        let git_service = crate::git::GitService::new(self.workspace_root.clone());
        let git_status = git_service.get_status().await.ok();
        let anchor_block = self.compactor.anchor().to_prompt_block();
        let working_set_vec: Vec<String> = self.active_working_set.iter().cloned().collect();
        let recency_block = PromptBuilder::build_recency_context(
            &self.workspace_root,
            if anchor_block.is_empty() {
                None
            } else {
                Some(&anchor_block)
            },
            &working_set_vec,
            git_status.as_ref(),
        );

        let prompt_with_context = format!("{}{}", user_prompt, recency_block);
        self.messages.push(Message::user(prompt_with_context));

        if let Some(metrics) = compaction_metrics {
            let compact_event = AgentEvent::ContextCompacted {
                turn_id,
                tier: metrics.tier,
                turns_summarized: metrics.turns_summarized,
                tokens_before: metrics.tokens_before,
                tokens_after: metrics.tokens_after,
                savings_percent: metrics.savings_percent,
            };
            if let Err(e) = self
                .session_store
                .append_event(&self.session_id, &compact_event)
            {
                tracing::warn!("Failed to persist ContextCompacted event: {}", e);
            }
            let _ = event_sender.send(compact_event);
        }

        // Record turn start checkpoint with prompt and message boundary
        let backup_manager = crate::session::backup::BackupManager::new(&self.workspace_root);
        if let Err(e) = backup_manager.record_turn_start(turn_id, user_prompt, message_index) {
            tracing::warn!(turn = turn_id, error = %e, "Failed to record turn start checkpoint");
        }

        let mut active_categories: std::collections::HashSet<crate::tools::category::ToolCategory> =
            std::collections::HashSet::new();

        let mut tools = crate::tools::category::assemble_active_tools(
            self.config.agent.tool_mode,
            user_prompt,
            &active_categories,
        );

        // Idempotent initialization of MCP client
        if !self.mcp_client.is_initialized() {
            if let Err(e) = self.mcp_client.init_from_config(&self.config.mcp).await {
                tracing::warn!("Failed to initialize MCP client: {}", e);
            }
        }
        let mcp_tools = self.mcp_client.get_tool_schemas().await;
        tools.extend(mcp_tools.clone());

        let now_ts = chrono::Utc::now().to_rfc3339();

        let prompt_event = AgentEvent::UserPrompt {
            turn_id,
            timestamp: now_ts.clone(),
            prompt: user_prompt.to_string(),
        };
        if let Err(e) = self
            .session_store
            .append_event(&self.session_id, &prompt_event)
        {
            tracing::warn!("Failed to persist UserPrompt event: {}", e);
        }
        let _ = event_sender.send(prompt_event);

        let initial_context_tokens = crate::context::compressor::ContextCompressor::new()
            .map(|c| c.count_messages_tokens(&self.messages))
            .unwrap_or(0);

        let start_event = AgentEvent::TurnStart {
            turn_id,
            timestamp: now_ts,
            model: self.config.provider.model.clone(),
            context_tokens: initial_context_tokens,
        };
        if let Err(e) = self
            .session_store
            .append_event(&self.session_id, &start_event)
        {
            tracing::warn!("Failed to persist TurnStart event: {}", e);
        }
        event_sender.send(start_event)?;

        let mut turn_response = String::new();
        let mut turn_tool_calls = Vec::new();
        let mut turn_tool_results = Vec::new();
        let mut last_prompt_tokens: usize = 0;
        let mut cumulative_completion_tokens: usize = 0;
        let mut turn_files_modified = Vec::new();

        let max_iterations = DEFAULT_MAX_TOOL_ITERATIONS; // Prevent infinite tool loops
        let mut iteration = 0;
        let mut heal_attempts = 0;
        let mut was_cancelled = false;

        let options = CompletionOptions {
            model: self.config.provider.model.clone(),
            temperature: self.config.provider.temperature,
            max_tokens: self.config.provider.max_tokens,
            system_instruction: Some(system_prompt),
        };

        let max_retries = DEFAULT_MAX_RETRIES;

        while iteration < max_iterations {
            // Check cancellation before each LLM call
            if let Some(cancel) = &cancel_token {
                if cancel.is_cancelled() {
                    tracing::info!("Turn #{} cancelled before LLM request", turn_id);
                    was_cancelled = true;
                    break;
                }
            }

            iteration += 1;
            let mut retry_count = 0;

            let mut iteration_text = String::new();
            let mut pending_tool_calls: Vec<ToolCall> = Vec::new();
            let turn_response_len_before = turn_response.len();

            let mut success = false;
            while !success && retry_count <= max_retries {
                let completions_before = cumulative_completion_tokens;

                if self.config.agent.streaming {
                    // --- Streaming mode: consume chunks incrementally, emit deltas live ---
                    let mut stream = match self
                        .provider
                        .stream_completion(&self.messages, &tools, &options)
                        .await
                    {
                        Ok(s) => s,
                        Err(e) => {
                            if retry_count < max_retries {
                                retry_count += 1;
                                let delay_secs = retry_count as u64 * RETRY_BACKOFF_SECS;
                                let retry_msg = format!(
                                    "Provider connection error. Retrying in {}s (attempt {}/{})...",
                                    delay_secs, retry_count, max_retries
                                );
                                let event = AgentEvent::Error {
                                    turn_id: Some(turn_id),
                                    code: "provider_error".to_string(),
                                    message: retry_msg,
                                    retrying: true,
                                    retry_after_ms: Some(delay_secs * 1000),
                                };
                                if let Err(e) = event_sender.send(event) {
                                    tracing::debug!(error = %e, "Failed to send retry error event");
                                }
                                if let Some(cancel) = &cancel_token {
                                    tokio::select! {
                                        _ = tokio::time::sleep(std::time::Duration::from_secs(delay_secs)) => {}
                                        _ = cancel.cancelled() => {
                                            tracing::info!("Turn #{} cancelled during retry backoff", turn_id);
                                            was_cancelled = true;
                                            break;
                                        }
                                    }
                                } else {
                                    tokio::time::sleep(std::time::Duration::from_secs(delay_secs))
                                        .await;
                                }
                                iteration_text.clear();
                                pending_tool_calls.clear();
                                turn_response.truncate(turn_response_len_before);
                                cumulative_completion_tokens = completions_before;
                                continue;
                            }
                            return Err(e);
                        }
                    };

                    let mut stream_error = None;
                    let mut cancelled = false;

                    loop {
                        let next_chunk = if let Some(ref cancel) = cancel_token {
                            tokio::select! {
                                _ = cancel.cancelled() => {
                                    cancelled = true;
                                    break;
                                }
                                chunk = stream.next() => chunk,
                            }
                        } else {
                            stream.next().await
                        };

                        match next_chunk {
                            Some(Ok(StreamChunk::Delta(delta))) => {
                                iteration_text.push_str(&delta);
                                turn_response.push_str(&delta);
                                let delta_event = AgentEvent::StreamDelta {
                                    turn_id,
                                    delta: delta.clone(),
                                };
                                if let Err(e) = self
                                    .session_store
                                    .append_event(&self.session_id, &delta_event)
                                {
                                    tracing::warn!("Failed to persist StreamDelta event: {}", e);
                                }
                                if let Err(e) = event_sender.send(delta_event) {
                                    tracing::debug!(error = %e, "Failed to send stream delta event");
                                }
                            }
                            Some(Ok(StreamChunk::ToolCallChunk(tool_call))) => {
                                let call_event = AgentEvent::ToolCall {
                                    turn_id,
                                    tool_id: tool_call.id.clone(),
                                    tool: tool_call.name.clone(),
                                    args: tool_call.arguments.clone(),
                                };
                                if let Err(e) = self
                                    .session_store
                                    .append_event(&self.session_id, &call_event)
                                {
                                    tracing::warn!("Failed to persist ToolCall event: {}", e);
                                }
                                if let Err(e) = event_sender.send(call_event) {
                                    tracing::debug!(error = %e, "Failed to send tool call event");
                                }
                                pending_tool_calls.push(tool_call);
                            }
                            Some(Ok(StreamChunk::Usage {
                                prompt_tokens,
                                completion_tokens,
                            })) => {
                                last_prompt_tokens = prompt_tokens;
                                cumulative_completion_tokens += completion_tokens;
                            }
                            Some(Ok(StreamChunk::Done)) => {
                                break;
                            }
                            Some(Err(e)) => {
                                stream_error = Some(e);
                                break;
                            }
                            None => {
                                break;
                            }
                        }
                    }

                    if cancelled {
                        tracing::info!("Turn #{} cancelled during stream consumption", turn_id);
                        was_cancelled = true;
                        break;
                    }

                    if let Some(err) = stream_error {
                        let err_msg = err.to_string().to_lowercase();
                        let is_benign_eof = err_msg.contains("stream ended")
                            || err_msg.contains("connection reset")
                            || err_msg.contains("broken pipe");

                        if is_benign_eof
                            && (!iteration_text.is_empty() || !pending_tool_calls.is_empty())
                        {
                            tracing::info!(
                                "Tolerating trailing stream closure after receiving response ({} chars)",
                                iteration_text.len()
                            );
                        } else if retry_count < max_retries {
                            retry_count += 1;
                            let delay_secs = retry_count as u64 * RETRY_BACKOFF_SECS;
                            let retry_msg = format!(
                                "Rate limit or network error. Retrying in {}s (attempt {}/{})...",
                                delay_secs, retry_count, max_retries
                            );
                            let event = AgentEvent::Error {
                                turn_id: Some(turn_id),
                                code: "rate_limited".to_string(),
                                message: retry_msg,
                                retrying: true,
                                retry_after_ms: Some(delay_secs * 1000),
                            };
                            if let Err(e) = event_sender.send(event) {
                                tracing::debug!(error = %e, "Failed to send retry error event");
                            }
                            if let Some(cancel) = &cancel_token {
                                tokio::select! {
                                    _ = tokio::time::sleep(std::time::Duration::from_secs(delay_secs)) => {}
                                    _ = cancel.cancelled() => {
                                        tracing::info!("Turn #{} cancelled during rate-limit backoff", turn_id);
                                        was_cancelled = true;
                                        break;
                                    }
                                }
                            } else {
                                tokio::time::sleep(std::time::Duration::from_secs(delay_secs))
                                    .await;
                            }
                            iteration_text.clear();
                            pending_tool_calls.clear();
                            turn_response.truncate(turn_response_len_before);
                            cumulative_completion_tokens = completions_before;
                            continue;
                        } else {
                            return Err(err);
                        }
                    }

                    success = true;
                } else {
                    // --- Non-streaming mode: collect full response, emit aggregated events ---
                    match self
                        .run_nonstreaming_iteration(
                            &self.messages,
                            &tools,
                            &options,
                            &event_sender,
                            &self.session_id,
                            turn_id,
                        )
                        .await
                    {
                        Ok((iter_text, tool_calls, last_pt, cum_ct)) => {
                            iteration_text = iter_text;
                            pending_tool_calls = tool_calls;
                            last_prompt_tokens = last_pt;
                            cumulative_completion_tokens = cum_ct;
                        }
                        Err(e) => {
                            if retry_count < max_retries {
                                retry_count += 1;
                                let delay_secs = retry_count as u64 * RETRY_BACKOFF_SECS;
                                let retry_msg = format!(
                                    "Provider connection error. Retrying in {}s (attempt {}/{})...",
                                    delay_secs, retry_count, max_retries
                                );
                                let event = AgentEvent::Error {
                                    turn_id: Some(turn_id),
                                    code: "provider_error".to_string(),
                                    message: retry_msg,
                                    retrying: true,
                                    retry_after_ms: Some(delay_secs * 1000),
                                };
                                let _ = event_sender.send(event);
                                if let Some(cancel) = &cancel_token {
                                    tokio::select! {
                                        _ = tokio::time::sleep(std::time::Duration::from_secs(delay_secs)) => {}
                                        _ = cancel.cancelled() => {
                                            was_cancelled = true;
                                            break;
                                        }
                                    }
                                } else {
                                    tokio::time::sleep(std::time::Duration::from_secs(delay_secs))
                                        .await;
                                }
                                iteration_text.clear();
                                pending_tool_calls.clear();
                                turn_response.truncate(turn_response_len_before);
                                cumulative_completion_tokens = completions_before;
                                continue;
                            }
                            return Err(e);
                        }
                    }
                    success = true;
                }
            }

            if was_cancelled {
                break;
            }

            // If assistant produced tool calls, execute them and continue ReAct loop
            if !pending_tool_calls.is_empty() {
                self.messages.push(Message::assistant_with_tools(
                    iteration_text,
                    pending_tool_calls.clone(),
                ));

                for tool_call in pending_tool_calls {
                    // Check cancellation before each tool execution
                    if let Some(cancel) = &cancel_token {
                        if cancel.is_cancelled() {
                            was_cancelled = true;
                            tracing::info!(
                                "Turn #{} cancelled before tool dispatch: {}",
                                turn_id,
                                tool_call.name
                            );
                            Self::emit_rejected_tool_result(
                                self,
                                &event_sender,
                                turn_id,
                                &tool_call,
                                "cancelled before dispatch",
                                &mut turn_tool_results,
                            );
                            continue;
                        }
                    }

                    turn_tool_calls.push(tool_call.clone());

                    // Record file access into active working set (Zone 3 Recency)
                    if let Some(path) = tool_call.arguments.get("path").and_then(|p| p.as_str()) {
                        self.record_file_access(path);
                        if FILE_MODIFYING_TOOLS.contains(&tool_call.name.as_str()) {
                            turn_files_modified.push(path.to_string());
                        }
                    }

                    // === Approval gate: pause for user decision on dangerous tools ===
                    if self.requires_approval(&tool_call.name) {
                        let (resp_tx, resp_rx) =
                            tokio::sync::oneshot::channel::<ApprovalDecision>();
                        self.pending_approvals
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .insert(tool_call.id.clone(), resp_tx);

                        let req_event = AgentEvent::ApprovalRequest {
                            turn_id,
                            tool_id: tool_call.id.clone(),
                            tool: tool_call.name.clone(),
                            args: tool_call.arguments.clone(),
                            reason: "requires user approval in strict mode".to_string(),
                        };
                        if let Err(e) = self
                            .session_store
                            .append_event(&self.session_id, &req_event)
                        {
                            tracing::warn!("Failed to persist ApprovalRequest event: {}", e);
                        }
                        if event_sender.send(req_event).is_err() {
                            tracing::debug!("Failed to send ApprovalRequest event");
                        }

                        let decision = if let Some(cancel) = &cancel_token {
                            tokio::select! {
                                _ = cancel.cancelled() => None,
                                r = resp_rx => r.ok(),
                            }
                        } else {
                            resp_rx.await.ok()
                        };
                        self.pending_approvals
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&tool_call.id);

                        if decision != Some(ApprovalDecision::Approve) {
                            let reason = if decision.is_none() {
                                "cancelled while awaiting approval"
                            } else {
                                "rejected by user"
                            };
                            Self::emit_rejected_tool_result(
                                self,
                                &event_sender,
                                turn_id,
                                &tool_call,
                                reason,
                                &mut turn_tool_results,
                            );
                            continue;
                        }
                    }

                    // Snapshot file content before dispatch for inline diff preview
                    let file_before: Option<String> = if FILE_MODIFYING_TOOLS
                        .contains(&tool_call.name.as_str())
                    {
                        match tool_call.arguments.get("path").and_then(|p| p.as_str()) {
                            Some(rel) => {
                                let full_path = self.workspace_root.join(rel);
                                match tokio::fs::metadata(&full_path).await {
                                    Ok(meta)
                                        if meta.len()
                                            <= crate::constants::MAX_DIFF_SNAPSHOT_BYTES as u64 =>
                                    {
                                        tokio::fs::read_to_string(&full_path).await.ok()
                                    }
                                    _ => None,
                                }
                            }
                            None => None,
                        }
                    } else {
                        None
                    };

                    // Execute tool (MCP vs Built-in)
                    let tool_result = if tool_call.name.starts_with(MCP_TOOL_PREFIX) {
                        let start = std::time::Instant::now();
                        let res = self
                            .mcp_client
                            .call_tool(&tool_call.name, &tool_call.arguments)
                            .await;
                        let duration_ms = start.elapsed().as_millis() as u64;
                        match res {
                            Ok(output) => crate::agent::types::ToolResult {
                                tool_id: tool_call.id.clone(),
                                tool_name: tool_call.name.clone(),
                                success: true,
                                output,
                                duration_ms,
                            },
                            Err(e) => crate::agent::types::ToolResult {
                                tool_id: tool_call.id.clone(),
                                tool_name: tool_call.name.clone(),
                                success: false,
                                output: format!("MCP tool error: {}", e),
                                duration_ms,
                            },
                        }
                    } else {
                        crate::tools::ToolRegistry::dispatch(
                            &self.workspace_root,
                            &tool_call.id,
                            &tool_call.name,
                            &tool_call.arguments,
                            Some(&self.backup_manager),
                            turn_id,
                        )
                        .await
                    };

                    // === Tool Middleware Pipeline: timing → redact → checkpoint → diff ===
                    let tool_result = self.tool_pipeline.run(
                        tool_result,
                        &tool_call.name,
                        &self.workspace_root,
                        &tool_call.arguments,
                        file_before.as_deref(),
                    );

                    // If LLM invoked activate_tools and succeeded, dynamically reload active schemas for subsequent steps
                    if tool_call.name == "activate_tools" && tool_result.success {
                        if let Some(cat_str) =
                            tool_call.arguments.get("category").and_then(|v| v.as_str())
                        {
                            if cat_str == "all" {
                                active_categories.extend(crate::tools::category::ToolCategory::ALL);
                            } else if let Ok(cat) =
                                cat_str.parse::<crate::tools::category::ToolCategory>()
                            {
                                active_categories.insert(cat);
                            }
                            tools = crate::tools::category::assemble_active_tools(
                                self.config.agent.tool_mode,
                                user_prompt,
                                &active_categories,
                            );
                            tools.extend(mcp_tools.clone());
                            tracing::info!(
                                "Dynamically activated '{}' tool category; active schemas count now {}",
                                cat_str,
                                tools.len()
                            );
                        }
                    }

                    let res_event = AgentEvent::ToolResult {
                        turn_id,
                        tool_id: tool_result.tool_id.clone(),
                        tool: tool_result.tool_name.clone(),
                        success: tool_result.success,
                        output: tool_result.output.clone(),
                        duration_ms: tool_result.duration_ms,
                    };
                    if let Err(e) = self
                        .session_store
                        .append_event(&self.session_id, &res_event)
                    {
                        tracing::warn!("Failed to persist ToolResult event: {}", e);
                    }
                    event_sender.send(res_event)?;

                    // Append tool result message for LLM context
                    self.messages.push(Message::tool_result(
                        tool_call.id,
                        tool_call.name,
                        tool_result.output.clone(),
                    ));

                    turn_tool_results.push(tool_result);
                }
            } else {
                // If files were modified and auto-healing is active, verify workspace compiles cleanly
                if !turn_files_modified.is_empty()
                    && self.config.agent.auto_heal
                    && heal_attempts < 2
                {
                    if let Ok(diag_report) =
                        crate::lsp::LspEngine::run_diagnostics(&self.workspace_root).await
                    {
                        if !diag_report.is_clean() && !diag_report.errors.is_empty() {
                            heal_attempts += 1;
                            let error_feedback = format!(
                                "Automatic compiler check detected {} errors after changes. Please review and fix:\n{}",
                                diag_report.errors.len(),
                                diag_report.format_for_agent(&self.workspace_root, 6)
                            );
                            let error_feedback = crate::sandbox::redact::SecretRedactor::global()
                                .redact(&error_feedback);
                            tracing::warn!("Compiler errors detected post-turn, triggering auto-healing attempt #{}", heal_attempts);
                            let status_event = AgentEvent::ToolResult {
                                turn_id,
                                tool_id: format!("auto_heal_{}", heal_attempts),
                                tool: "compiler_check".to_string(),
                                success: false,
                                output: error_feedback.clone(),
                                duration_ms: 100,
                            };
                            let _ = event_sender.send(status_event);

                            self.messages.push(Message::assistant(iteration_text));
                            self.messages.push(Message::user(error_feedback));
                            continue;
                        }
                    }
                }

                // No more tool calls and workspace compiles cleanly; assistant finished turn
                self.messages.push(Message::assistant(iteration_text));
                break;
            }
        }

        let turn_tokens_used = last_prompt_tokens + cumulative_completion_tokens;

        // Autonomous Git Auto-Commit if files were modified during this turn
        if !turn_files_modified.is_empty() && self.config.git.auto_commit {
            let git = crate::git::GitService::new(self.workspace_root.clone());
            if git.is_git_repo().await {
                let commit_svc = crate::git::GitCommitService::new(&git)
                    .with_stage_all(self.config.git.dirty_commit);
                let commit_msg = if self.config.git.ai_commit_messages {
                    crate::git::GitCommitService::generate_conventional_message(
                        &turn_files_modified,
                        Some(user_prompt),
                    )
                } else {
                    format!("chore: update {}", turn_files_modified.join(", "))
                };
                match commit_svc
                    .commit(&commit_msg, Some(&turn_files_modified))
                    .await
                {
                    Ok(hash) => {
                        let git_event = AgentEvent::GitCommit {
                            turn_id,
                            hash,
                            message: commit_msg,
                            files: turn_files_modified.clone(),
                        };
                        if let Err(e) = self
                            .session_store
                            .append_event(&self.session_id, &git_event)
                        {
                            tracing::warn!("Failed to persist GitCommit event: {}", e);
                        }
                        if let Err(e) = event_sender.send(git_event) {
                            tracing::debug!(error = %e, "Failed to send git commit event");
                        }
                        crate::ui::status::StatusWidgets::invalidate_git_cache();
                    }
                    Err(e) => {
                        tracing::warn!("Auto-commit skipped: {}", e);
                    }
                }
            }
        }
        let end_event = AgentEvent::TurnEnd {
            turn_id,
            status: if was_cancelled {
                "cancelled"
            } else {
                "complete"
            }
            .to_string(),
            total_tokens_used: turn_tokens_used,
            files_modified: turn_files_modified.clone(),
        };
        if let Err(e) = self
            .session_store
            .append_event(&self.session_id, &end_event)
        {
            tracing::warn!("Failed to persist TurnEnd event: {}", e);
        }
        event_sender.send(end_event)?;

        // Sync turn learnings & facts into Progressive Multi-Tier Memory
        let mut prog_mem =
            crate::context::progressive_memory::ProgressiveMemory::load(&self.workspace_root);
        prog_mem.extract_and_store_facts(&turn_response, &format!("turn_{}", turn_id));
        prog_mem.extract_and_store_facts(user_prompt, "user_prompt");
        if !turn_files_modified.is_empty() {
            prog_mem.record_l1_anchor(
                &format!("Modified {} files", turn_files_modified.len()),
                &turn_files_modified,
                user_prompt,
                &format!("turn_{}", turn_id),
            );
        }
        let _ = prog_mem.save(&self.workspace_root);

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
        // answer them no longer exists.  Callers who already received ApprovalRequest
        // events will simply never receive a decision; the cancellation path handles
        // those cleanly (see cancelled_while_awaiting_approval_cleans_registry_and_reports_cancelled).
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
    }

    /// Removes entries whose sender half has already been dropped (e.g. host closed
    /// without responding, or turn was rolled back).  Calling this at the start of
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

    /// Emits events, session records, and LLM-visible messages for a denied
    /// or cancelled tool call so function-call/result pairing stays valid.
    fn emit_rejected_tool_result(
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::provider::{ChunkStream, CompletionOptions, Provider, ToolSchema};
    use async_trait::async_trait;

    struct DummyProvider;

    #[async_trait]
    impl Provider for DummyProvider {
        fn name(&self) -> &str {
            "dummy"
        }
        fn default_model(&self) -> &str {
            "dummy-model"
        }
        async fn stream_completion(
            &self,
            _messages: &[Message],
            _tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> Result<ChunkStream> {
            let stream = tokio_stream::empty();
            Ok(Box::pin(stream))
        }
    }

    /// Streams exactly one scripted tool call, then Done on every later call
    /// (models see the rejection and stop calling tools).
    struct ToolCallProvider {
        call: ToolCall,
        spent: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl Provider for ToolCallProvider {
        fn name(&self) -> &str {
            "dummy-toolcall"
        }
        fn default_model(&self) -> &str {
            "dummy-model"
        }
        async fn stream_completion(
            &self,
            _messages: &[Message],
            _tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> Result<ChunkStream> {
            if self.spent.swap(true, std::sync::atomic::Ordering::SeqCst) {
                let stream = tokio_stream::iter(vec![Ok(StreamChunk::Done)]);
                return Ok(Box::pin(stream));
            }
            let stream = tokio_stream::iter(vec![
                Ok(StreamChunk::ToolCallChunk(self.call.clone())),
                Ok(StreamChunk::Done),
            ]);
            Ok(Box::pin(stream))
        }
    }

    #[tokio::test]
    async fn rejected_approval_produces_failed_tool_result_without_hang() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default(); // agent.auto_approve == false
        let provider = Box::new(ToolCallProvider {
            call: ToolCall {
                id: "call_gate_1".into(),
                name: "write_file".into(),
                arguments: serde_json::json!({"path": "gate.txt", "content": "hi"}),
            },
            spent: std::sync::atomic::AtomicBool::new(false),
        });
        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        agent_loop.set_interactive_approvals(true);
        let reg = agent_loop.approval_registry();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let turn_task =
            tokio::spawn(async move { agent_loop.execute_turn("make a file", tx, None).await });

        // Gate must block: wait for the ApprovalRequest event.
        let mut requested_tool_id = None;
        for _ in 0..50 {
            match rx.recv().await {
                Some(AgentEvent::ApprovalRequest { tool_id, .. }) => {
                    requested_tool_id = Some(tool_id);
                    break;
                }
                Some(_) => {}
                None => panic!("event stream ended before ApprovalRequest"),
            }
        }
        let tool_id = requested_tool_id.expect("ApprovalRequest must be emitted");
        assert!(
            !turn_task.is_finished(),
            "turn must block awaiting decision"
        );

        // Reject through the registry by key.
        let sender = reg
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&tool_id)
            .expect("pending approval must be registered under its tool_id");
        assert!(sender.send(ApprovalDecision::Reject).is_ok());

        let turn = turn_task.await.expect("join ok").expect("turn completes");
        assert_eq!(turn.tool_results.len(), 1);
        assert!(!turn.tool_results[0].success);
        assert!(turn.tool_results[0].output.contains("rejected by user"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn cancelled_while_awaiting_approval_cleans_registry_and_reports_cancelled() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(ToolCallProvider {
            call: ToolCall {
                id: "call_cancel_1".into(),
                name: "exec_cmd".into(),
                arguments: serde_json::json!({"command": "echo hi"}),
            },
            spent: std::sync::atomic::AtomicBool::new(false),
        });
        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        agent_loop.set_interactive_approvals(true); // exec_cmd gates on approval
        let reg = agent_loop.approval_registry();

        let cancel = tokio_util::sync::CancellationToken::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cancel_for_task = cancel.clone();
        let turn_task = tokio::spawn(async move {
            agent_loop
                .execute_turn("run echo", tx, Some(cancel_for_task))
                .await
        });

        // Wait until the gate registers the pending approval.
        loop {
            match rx.recv().await {
                Some(AgentEvent::ApprovalRequest { .. }) => break,
                Some(_) => {}
                None => panic!("stream ended before ApprovalRequest"),
            }
        }
        cancel.cancel();

        let turn = turn_task.await.expect("join ok").expect("turn completes");
        assert!(
            reg.lock().unwrap_or_else(|p| p.into_inner()).is_empty(),
            "registry must not leak entries after cancellation"
        );
        assert_eq!(turn.tool_results.len(), 1);
        assert!(!turn.tool_results[0].success);
        assert!(turn.tool_results[0].output.contains("cancelled"));

        // TurnEnd must report cancelled, not complete.
        let saw_cancelled = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
            matches!(
                e,
                AgentEvent::TurnEnd { ref status, .. } if status == "cancelled"
            )
        });
        assert!(saw_cancelled, "expected TurnEnd status=cancelled");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn precancelled_turn_emits_cancelled_turnend() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let mut agent_loop = AgentLoop::new(&temp_dir, config, Box::new(DummyProvider));
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _ = agent_loop.execute_turn("hi", tx, Some(cancel)).await;
        let saw_cancelled = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
            matches!(
                e,
                AgentEvent::TurnEnd { ref status, .. } if status == "cancelled"
            )
        });
        assert!(saw_cancelled, "expected TurnEnd status=cancelled");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_prune_context_preserves_minimum_messages() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(DummyProvider);
        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);

        // Add small number of messages <= CONTEXT_MIN_PRESERVED_MESSAGES
        agent_loop.messages.push(Message::user("Hello 1"));
        agent_loop.messages.push(Message::assistant("Hi 1"));
        agent_loop.prune_context();
        assert_eq!(agent_loop.messages.len(), 2);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_prune_context_compacts_and_prunes_large_history() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(DummyProvider);
        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);

        // Add 10 messages with huge content
        let huge_text = "word ".repeat(15_000);
        for i in 0..10 {
            agent_loop
                .messages
                .push(Message::user(format!("User {}", i)));
            agent_loop
                .messages
                .push(Message::assistant(huge_text.clone()));
        }
        assert_eq!(agent_loop.messages.len(), 20);

        agent_loop.prune_context();
        // Should have pruned oldest messages down while preserving at least min preserved
        assert!(agent_loop.messages.len() < 20);
        assert!(agent_loop.messages.len() >= CONTEXT_MIN_PRESERVED_MESSAGES);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
