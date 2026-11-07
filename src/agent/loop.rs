use crate::agent::prompt::PromptBuilder;
use crate::agent::provider::{CompletionOptions, Provider, StreamChunk};
use crate::agent::types::{AgentEvent, ApprovalDecision, Message, ToolCall, Turn};
use crate::config::Config;
use crate::constants::{
    DEFAULT_MAX_RETRIES, FILE_MODIFYING_TOOLS, MCP_TOOL_PREFIX, RETRY_BACKOFF_SECS,
    TURN_STATUS_CANCELLED, TURN_STATUS_CIRCUIT_TRIPPED, TURN_STATUS_COMPLETE,
    TURN_STATUS_ITERATION_LIMIT,
};
use crate::error::Result;
use crate::mcp::McpClientManager;
use crate::session::backup::BackupManager;
use crate::session::store::SessionStore;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

#[path = "loop_impl/approval.rs"]
mod approval;
#[path = "loop_impl/session_state.rs"]
mod session_state;
#[path = "loop_impl/tool_finalize.rs"]
mod tool_finalize;

pub use approval::{classify_mandatory_approval, classify_mandatory_shell_command};

#[cfg(test)]
#[path = "loop_impl/tests.rs"]
mod tests;

pub struct AgentLoop {
    workspace_root: PathBuf,
    config: Config,
    provider: std::sync::Arc<dyn Provider>,
    session_store: SessionStore,
    session_id: String,
    session_persisted: bool,
    backup_manager: BackupManager,
    mcp_client: McpClientManager,
    /// Composable middleware pipeline applied to every tool result.
    tool_pipeline: crate::tools::middleware::ToolPipeline,
    messages: Vec<Message>,
    current_turn_id: usize,
    /// In-flight approval requests: tool_id → oneshot responder.
    pending_approvals: super::types::ApprovalRegistry,
    /// In-flight inquiry requests: inquiry_id → oneshot responder.
    pending_inquiries: crate::agent::inquiry::InquiryRegistry,
    /// Whether a live host can answer approval requests (false in headless mode).
    interactive_approvals: bool,
    /// Smart 4-tier progressive context auto-compactor with memory anchor.
    compactor: crate::context::auto_compact::AutoCompactor,
    /// Active working set tracking recently accessed/modified files (Zone 3 Recency).
    active_working_set: std::collections::VecDeque<String>,
    /// Algorithmic stuck detector halting repetitive tool loops.
    pub stuck_detector: crate::agent::stuck_detector::StuckDetector,
    /// Redundant fetch/query call tracker warning on 2nd and blocking on 3rd+ identical read call.
    pub fetch_tracker: crate::context::budget::dedup::FetchCallTracker,
    /// Speculative early dispatch and parallel execution engine for read-only tools.
    pub speculative_executor: crate::agent::speculative::SpeculativeExecutor,
    /// Cumulative tokens expended across all turns in this session.
    pub cumulative_tokens_used: usize,
    /// Living intent anchor and execution ledger.
    pub intent_ledger: Option<crate::context::memory::intent::IntentLedger>,
    /// Reactive agent-to-agent FIFO mailbox for receiving subagent coordination messages.
    pub mailbox: Option<crate::agent::subagent::mailbox::AgentMailbox>,
    /// Last seen swarm message ID for reactive turn-start ingestion.
    pub last_seen_swarm_msg_id: Option<String>,
    /// Signature of the last emitted `PlanUpdated` payload to suppress redundant duplicate events.
    last_emitted_plan_sig: std::sync::Mutex<Option<String>>,
    /// RAII Guard registering this active agent process in the runtime registry.
    _active_guard: Option<crate::logging::ActiveSessionGuard>,
}

impl AgentLoop {
    pub fn new(workspace_root: &Path, config: Config, provider: Box<dyn Provider>) -> Self {
        Self::with_session(workspace_root, config, provider, None)
    }

    pub fn with_session(
        workspace_root: &Path,
        config: Config,
        provider: Box<dyn Provider>,
        resume_session_id: Option<&str>,
    ) -> Self {
        let session_store = SessionStore::with_workspace(workspace_root);
        let (session_id, session_persisted, active_guard) = if let Some(sid) = resume_session_id {
            let session_file = session_store.session_file_path(sid);
            let guard = crate::logging::register_active_session(
                sid,
                workspace_root,
                &config.provider.default,
                &config.provider.model,
                &session_file,
            )
            .ok();
            (sid.to_string(), true, guard)
        } else {
            let pending_id = session_store.generate_session_id();
            (pending_id, false, None)
        };

        let mcp_client = McpClientManager::new();

        // Ensure internal .minicode directories are ignored by git
        let git_service = crate::git::GitService::new(workspace_root.to_path_buf());
        git_service.ensure_git_exclude();

        let mut compactor =
            crate::context::auto_compact::AutoCompactor::new(&config.provider.model)
                .unwrap_or_else(|_| crate::context::auto_compact::AutoCompactor::default_safe());
        compactor.update_model(&config.provider.model, config.provider.context_window);

        crate::context::budget::donut::set_active_context_limit(compactor.model_token_limit());

        let speculative_executor = crate::agent::speculative::SpeculativeExecutor::new(
            workspace_root.to_path_buf(),
            config.agent.max_parallel_tools,
            config.agent.speculative_execution,
            config.agent.parallel_tools,
        );

        let intent_path = workspace_root.join(&config.agent.intent.persistence_file);
        let intent_ledger = if config.agent.intent.enabled && intent_path.exists() {
            crate::context::memory::intent::IntentLedger::load_from_disk(&intent_path).ok()
        } else {
            None
        };

        let mailbox = crate::agent::subagent::mailbox::AgentMailbox::new(
            crate::agent::subagent::types::AgentId::parent(),
            &workspace_root
                .join(".minicode")
                .join("agents")
                .join("parent"),
        )
        .ok();

        let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::from(provider);
        crate::agent::subagent::sync_live_orchestrator_runtime(
            workspace_root,
            std::sync::Arc::clone(&provider),
            &config.provider.model,
            None,
        );

        Self {
            workspace_root: workspace_root.to_path_buf(),
            config,
            provider,
            session_store,
            session_id,
            session_persisted,
            backup_manager: BackupManager::new(workspace_root),
            mcp_client,
            tool_pipeline: crate::tools::middleware::ToolPipeline::default(),
            messages: Vec::new(),
            current_turn_id: 0,
            pending_approvals: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_inquiries: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            interactive_approvals: true,
            compactor,
            active_working_set: std::collections::VecDeque::with_capacity(10),
            stuck_detector: crate::agent::stuck_detector::StuckDetector::new(),
            fetch_tracker: crate::context::budget::dedup::FetchCallTracker::new(),
            speculative_executor,
            cumulative_tokens_used: 0,
            intent_ledger,
            mailbox,
            last_seen_swarm_msg_id: None,
            last_emitted_plan_sig: std::sync::Mutex::new(None),
            _active_guard: active_guard,
        }
    }

    /// Executes a single interactive or autonomous turn with the ReAct tool-use loop.
    /// Emits structured `AgentEvent`s over the provided MPSC channel for UI or NDJSON rendering.
    pub async fn execute_turn(
        &mut self,
        user_prompt: &str,
        event_sender: mpsc::UnboundedSender<AgentEvent>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<Turn> {
        self.ensure_session_persisted()?;
        crate::agent::subagent::sync_live_orchestrator_runtime(
            &self.workspace_root,
            std::sync::Arc::clone(&self.provider),
            &self.config.provider.model,
            Some(event_sender.clone()),
        );

        // Drop stale approval senders from abandoned turns before doing anything else.
        // This prevents the registry from growing unboundedly when hosts disappear
        // or turns get rolled back without draining their approval entries.
        self.prune_stale_approvals();
        self.prune_stale_inquiries();

        // Drain any incoming reactive A2A messages into conversation history
        if let Some(ref mb) = self.mailbox {
            if let Ok(unread) = mb.drain_unread() {
                for msg in unread {
                    tracing::info!(from = %msg.sender.0, intent = ?msg.intent, "Ingested incoming A2A message");
                    self.messages.push(Message::user(msg.format_for_prompt()));
                }
            }
        }

        self.current_turn_id += 1;
        let turn_id = self.current_turn_id;

        // Synchronize active model context window globally for tool execution & donut truncation (Phase 117)
        let model_token_limit = self.compactor.model_token_limit();
        crate::context::budget::donut::set_active_context_limit(model_token_limit);

        if turn_id == 1 {
            self.compactor.set_working_context(user_prompt);
        }

        // Direct `/skill-name` slash invocation (Claude Code / Codex / Pi Agent style):
        // If the user prompt starts with `/<skill-name>`, automatically load/activate that skill in the project.
        let mut slash_loaded_skill: Option<String> = None;
        if let Some(first_token) = user_prompt.trim().split_whitespace().next() {
            if let Some(slash_skill) = first_token.strip_prefix('/') {
                let candidate = slash_skill
                    .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
                if !candidate.is_empty() {
                    let store = crate::vault::store::VaultStore::new(&self.workspace_root);
                    if let Ok(loaded) = store.load_to_project(candidate) {
                        slash_loaded_skill = Some(loaded.name);
                    }
                }
            }
        }

        // Initialize or refresh intent ledger from user prompt if enabled and auto_extract is on
        if self.config.agent.intent.enabled && self.config.agent.intent.auto_extract {
            let persistence_path = self
                .workspace_root
                .join(&self.config.agent.intent.persistence_file);
            if let Some(ref mut ledger) = self.intent_ledger {
                if turn_id > 1 {
                    ledger.refresh_for_new_turn(
                        user_prompt,
                        self.config.agent.intent.max_ledger_items,
                        turn_id,
                    );
                    let _ = ledger.save_to_disk(&persistence_path);
                }
            } else {
                let ledger = crate::context::memory::intent::IntentLedger::from_prompt(
                    user_prompt,
                    self.config.agent.intent.max_ledger_items,
                );
                if let Err(e) = ledger.save_to_disk(&persistence_path) {
                    tracing::warn!(
                        error = %e,
                        path = %persistence_path.display(),
                        "Failed to persist initial intent ledger"
                    );
                }
                self.intent_ledger = Some(ledger);
            }
        }

        // 1. Prune/compact conversation context if approaching budget
        let micro_metrics =
            crate::context::budget::MicroCompactor::compact_messages(&mut self.messages, 2);
        if micro_metrics.tokens_saved_estimate > 0 {
            tracing::info!(
                superseded_reads = micro_metrics.superseded_reads_compacted,
                duplicate_reads = micro_metrics.duplicate_reads_compacted,
                mutation_echoes = micro_metrics.mutation_echoes_compacted,
                search_results = micro_metrics.search_results_compacted,
                tokens_saved = micro_metrics.tokens_saved_estimate,
                "Applied semantic micro-compaction to conversation history"
            );
        }
        crate::context::auto_compact::AutoCompactor::strip_older_workspace_context(
            &mut self.messages,
        );
        let mut compaction_metrics = self.prune_context();

        // 2. Build Tri-Zone context:
        // Zone 1: Primacy Zone (100% static, immutable system prompt for cache hits)
        let system_prompt = PromptBuilder::build_static_system_prompt(&self.workspace_root, None);

        // Swarm Turn-Start Ingestion: Check for unread peer messages
        let mut peer_messages_block = String::new();
        if let (Ok(swarm_dir_str), Ok(task_id)) = (
            std::env::var("MINICODE_SWARM_DIR"),
            std::env::var("MINICODE_SWARM_TASK_ID"),
        ) {
            let swarm_dir_trimmed = swarm_dir_str.trim().to_string();
            let task_id_trimmed = task_id.trim().to_string();
            if !swarm_dir_trimmed.is_empty() && !task_id_trimmed.is_empty() {
                let last_seen = self.last_seen_swarm_msg_id.clone();
                let unread_res = tokio::task::spawn_blocking(move || {
                    let swarm_dir = std::path::Path::new(&swarm_dir_trimmed);
                    let bus = crate::agent::swarm::bus::SwarmMessageBus::new(swarm_dir)?;
                    bus.read_unread(&task_id_trimmed, last_seen.as_deref())
                })
                .await;

                if let Ok(Ok(unread)) = unread_res {
                    if !unread.is_empty() {
                        if let Some(newest) = unread.last() {
                            self.last_seen_swarm_msg_id = Some(newest.id.clone());
                        }
                        peer_messages_block =
                            crate::agent::prompt::format_peer_messages_prompt(&unread);
                    }
                }
            }
        }

        // Zone 3: Recency Zone (dynamic workspace context: git, active files, memory anchor, context budget)
        let git_service = crate::git::GitService::new(self.workspace_root.clone());
        let git_status = git_service.get_status().await.ok();
        let anchor_block = self.compactor.anchor().to_prompt_block();
        let working_set_vec: Vec<String> = self.active_working_set.iter().cloned().collect();

        let active_context_tokens = self.compactor.count_tokens(&self.messages);
        let model_token_limit = self.compactor.model_token_limit();
        let context_budget = crate::context::budget::ContextBudget::new(
            active_context_tokens,
            model_token_limit,
            self.cumulative_tokens_used,
        );

        // Autonomous State-Driven Context Enrichment
        let workflow_enrichment =
            crate::agent::orchestrator::WorkflowRouter::enrich_state_driven_context(
                &self.workspace_root,
                user_prompt,
            )
            .await;

        // Auto-initialize living IntentLedger for non-slash prompts if not already active
        if self.config.agent.intent.enabled
            && self.intent_ledger.is_none()
            && !user_prompt.trim().starts_with('/')
        {
            self.intent_ledger = Some(crate::context::memory::intent::IntentLedger::new(
                user_prompt,
            ));
        }

        // Auto-detect natural-language Caveman Mode triggers ("talk like caveman", "ultracave", "megacave", "stop caveman")
        if let Some(triggered_level) =
            crate::agent::caveman::CavemanLevel::detect_natural_trigger(user_prompt)
        {
            self.config.agent.caveman_mode = triggered_level.is_enabled();
            if triggered_level.is_enabled() {
                self.config.agent.caveman_level = triggered_level.as_str().to_string();
            }
            let flag_path = crate::agent::caveman::default_flag_path(&self.workspace_root);
            let _ = crate::agent::caveman::write_flag_safe(&flag_path, triggered_level);
        }

        let recency_block = PromptBuilder::build_recency_context(
            &self.workspace_root,
            if anchor_block.is_empty() {
                None
            } else {
                Some(&anchor_block)
            },
            &working_set_vec,
            git_status.as_ref(),
            Some(&context_budget),
            if self.config.agent.intent.enabled {
                self.intent_ledger.as_ref()
            } else {
                None
            },
            workflow_enrichment.as_deref(),
        );

        let mut prompt_with_context = if recency_block.trim().is_empty() {
            format!("<user_request>\n{}\n</user_request>", user_prompt.trim())
        } else {
            format!(
                "{}\n\n<user_request>\n{}\n</user_request>",
                recency_block.trim(),
                user_prompt.trim()
            )
        };

        if !peer_messages_block.is_empty() {
            prompt_with_context =
                format!("{}\n\n{}", peer_messages_block.trim(), prompt_with_context);
        }

        let matched_directive = crate::agent::boulder::BoulderState::match_directive(user_prompt);
        let is_ralph_loop = matched_directive
            .map(|(r, _)| r.enables_ralph_promise)
            .unwrap_or(false);
        let is_ultrawork =
            Self::detect_ultrawork_intent(user_prompt) || matched_directive.is_some();
        if is_ultrawork {
            let ralph_contract = if is_ralph_loop {
                "\n4. RALPH LOOP CONTRACT: Do NOT conclude until all work and verification gates pass, then emit `<promise>DONE</promise>` in your final completion message."
            } else {
                ""
            };
            prompt_with_context = format!(
                "[AUTONOMOUS ULTRAWORK MODE ACTIVE]\n\
                 1. Execute with maximum thoroughness and zero premature stopping.\n\
                 2. Structure multi-step work in `create_plan` (or `create_task_dag`), delegate parallelizable subtasks to specialized `MiniPub` workers (`dispatch_subagent` / `delegate_task` with `category` and `load_skills`), and verify all changes via `power_verify`.\n\
                 3. Do NOT stop until 100% of planned steps and verification gates are complete.{}\n\n{}",
                ralph_contract,
                prompt_with_context
            );
        }

        let attached_images =
            Self::extract_user_image_attachments(user_prompt, &self.workspace_root);
        if attached_images.is_empty() {
            self.messages.push(Message::user(prompt_with_context));
        } else {
            self.messages.push(Message::user_with_images(
                prompt_with_context,
                attached_images,
            ));
        }
        if compaction_metrics.is_none() {
            compaction_metrics = self.prune_context();
        }
        let message_index = self.messages.len().saturating_sub(1);

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
        if let Err(e) = self
            .backup_manager
            .record_turn_start(turn_id, user_prompt, message_index)
        {
            tracing::warn!(turn = turn_id, error = %e, "Failed to record turn start checkpoint");
        }

        // Reset stuck detector at start of user turn
        self.stuck_detector.reset();

        // Emit current plan without auto-completing tasks against pre-existing files on disk
        self.emit_current_plan(Some(turn_id), &event_sender);

        let mut active_categories =
            crate::agent::orchestrator::WorkflowRouter::state_driven_tool_categories(
                &self.workspace_root,
            );

        let mut active_mcp_servers: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut active_tool_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for allowed in crate::vault::store::VaultStore::new(&self.workspace_root)
            .collect_active_skill_allowed_tools(Some(user_prompt), None, &working_set_vec)
        {
            if let Ok(cat) = allowed.parse::<crate::tools::category::ToolCategory>() {
                active_categories.insert(cat);
            }
            active_tool_names.insert(allowed);
        }

        // Idempotent initialization of MCP client
        if !self.mcp_client.is_initialized() {
            if let Err(e) = self.mcp_client.init_from_config(&self.config.mcp).await {
                tracing::warn!("Failed to initialize MCP client: {}", e);
            }
        }
        let mcp_tools_by_server = self.mcp_client.get_tools_by_server().await;

        let mut tools = self.rebuild_active_tools(
            user_prompt,
            &active_categories,
            &mcp_tools_by_server,
            &active_mcp_servers,
            &active_tool_names,
        );

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

        let initial_context_tokens = self.count_current_context_tokens();

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

        if let Some(loaded_name) = slash_loaded_skill {
            let skill_event = AgentEvent::SkillActivity {
                turn_id,
                action: "loaded".to_string(),
                skills: vec![loaded_name],
                detail: Some("Activated via slash command".to_string()),
            };
            let _ = self
                .session_store
                .append_event(&self.session_id, &skill_event);
            let _ = event_sender.send(skill_event);
        }

        if is_ultrawork {
            let _ = event_sender.send(AgentEvent::Heartbeat {
                turn_id: Some(turn_id),
                timestamp: chrono::Utc::now().to_rfc3339(),
                status: "toast:ultrawork\tUltrawork Mode Active\tExtended 32K thinking + 5x Boulder continuation enforcer".to_string(),
            });
        }

        let turn_active_skills = crate::vault::store::VaultStore::new(&self.workspace_root)
            .resolve_turn_active_skills(user_prompt, None, &working_set_vec);
        if !turn_active_skills.is_empty() {
            let skill_event = AgentEvent::SkillActivity {
                turn_id,
                action: "using".to_string(),
                skills: turn_active_skills,
                detail: None,
            };
            let _ = self
                .session_store
                .append_event(&self.session_id, &skill_event);
            let _ = event_sender.send(skill_event);
        }

        let mut turn_response = String::new();
        let mut turn_tool_calls = Vec::new();
        let mut turn_tool_results = Vec::new();
        let mut last_prompt_tokens: usize = 0;
        let mut last_cached_prompt_tokens: usize = 0;
        let mut cumulative_completion_tokens: usize = 0;
        let mut turn_files_modified = Vec::new();
        let mut items_progressed_this_turn = false;

        // Automatically begin a workspace transaction if none is active to provide Senior Engineer rollback safety
        let turn_tx_id = if let Ok(None) =
            crate::session::transaction::TransactionManager::get_active(&self.workspace_root)
        {
            match crate::session::transaction::TransactionManager::begin(
                &self.workspace_root,
                &format!(
                    "Autonomous turn #{}: {}",
                    turn_id,
                    user_prompt.chars().take(80).collect::<String>()
                ),
            ) {
                Ok(m) => Some(m.tx_id),
                Err(e) => {
                    tracing::debug!(error = %e, "Could not begin automatic transaction for turn");
                    None
                }
            }
        } else {
            None
        };

        let mut max_iterations = self.config.agent.max_tool_iterations;
        let mut auto_continues_remaining = if self.config.agent.auto_continue {
            if is_ultrawork {
                self.config.agent.max_auto_continues.max(8)
            } else {
                self.config.agent.max_auto_continues
            }
        } else {
            0
        };
        let max_boulder_nudges: usize = if is_ultrawork { 5 } else { 3 };
        let initial_boulder = crate::agent::boulder::BoulderState::sync_from_workspace(
            &self.workspace_root,
            is_ralph_loop,
            max_boulder_nudges,
        );
        let mut iteration = 0;
        let mut heal_attempts = 0;
        let mut last_verification_failure: Option<(String, String)> = None;
        let mut plan_continuation_nudges: usize = initial_boulder
            .continuation_count
            .min(max_boulder_nudges.saturating_sub(1));
        let mut was_cancelled = false;
        let mut circuit_tripped = false;
        let mut hit_iteration_limit = false;

        let effective_thinking_budget = if is_ultrawork {
            Some(
                self.config
                    .provider
                    .thinking_budget
                    .unwrap_or(0)
                    .max(16_000),
            )
        } else {
            self.config.provider.thinking_budget
        };

        let mut options = CompletionOptions {
            model: self.config.provider.model.clone(),
            temperature: self.config.provider.temperature,
            max_tokens: self.config.provider.max_tokens,
            system_instruction: Some(system_prompt),
            thinking_budget: effective_thinking_budget,
            reasoning_effort: self.config.provider.reasoning_effort.clone(),
        };

        let max_retries = DEFAULT_MAX_RETRIES;

        while max_iterations == 0 || iteration < max_iterations {
            // Check cancellation before each LLM call
            if let Some(cancel) = &cancel_token {
                if cancel.is_cancelled() {
                    tracing::info!("Turn #{} cancelled before LLM request", turn_id);
                    was_cancelled = true;
                    break;
                }
            }

            iteration += 1;

            // In long-horizon multi-step turns, proactively prune context and strip older reasoning
            // to prevent context bloat before requesting next LLM completion.
            if iteration > 1 {
                self.prune_context();
            }
            crate::agent::subagent::relay::get_global_relay_store()
                .sync_parent_messages(&self.workspace_root, &self.messages);
            let mut retry_count = 0;

            let mut iteration_text = String::new();
            let mut pending_tool_calls: Vec<ToolCall> = Vec::new();
            let turn_response_len_before = turn_response.len();

            let mut success = false;
            self.speculative_executor.reset_turn().await;
            while !success && retry_count <= max_retries {
                let completions_before = cumulative_completion_tokens;

                if self.config.agent.streaming {
                    // --- Streaming mode: consume chunks incrementally, emit deltas live ---
                    let stream_fut =
                        self.provider
                            .stream_completion(&self.messages, &tools, &options);
                    let stream_res = if let Some(cancel) = &cancel_token {
                        tokio::select! {
                            _ = cancel.cancelled() => {
                                was_cancelled = true;
                                self.speculative_executor.cancel_all().await;
                                break;
                            }
                            res = stream_fut => res,
                        }
                    } else {
                        stream_fut.await
                    };

                    let mut stream = match stream_res {
                        Ok(s) => s,
                        Err(e) => {
                            self.speculative_executor.cancel_all().await;
                            if retry_count < max_retries {
                                retry_count += 1;
                                let delay_secs = retry_count as u64 * RETRY_BACKOFF_SECS;
                                let retry_msg = format!(
                                    "⚠ Provider connection error. Retrying in {}s (attempt {}/{})...",
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
                            let err_event = AgentEvent::Error {
                                turn_id: Some(turn_id),
                                code: "provider_error".to_string(),
                                message: e.to_string(),
                                retrying: false,
                                retry_after_ms: None,
                            };
                            let _ = self
                                .session_store
                                .append_event(&self.session_id, &err_event);
                            let _ = event_sender.send(err_event);
                            let end_event = AgentEvent::TurnEnd {
                                turn_id,
                                status: "error".to_string(),
                                total_tokens_used: last_prompt_tokens
                                    .saturating_add(cumulative_completion_tokens),
                                files_modified: turn_files_modified.clone(),
                                cached_prompt_tokens: last_cached_prompt_tokens,
                            };
                            let _ = self
                                .session_store
                                .append_event(&self.session_id, &end_event);
                            let _ = event_sender.send(end_event);
                            let _ =
                                crate::tools::browser::BrowserManager::shutdown_live_engine().await;
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
                            Some(Ok(StreamChunk::ToolCallProgress { name, args_preview })) => {
                                let _ = event_sender.send(AgentEvent::Heartbeat {
                                    timestamp: String::new(),
                                    status: format!("tool_stream:{}\t{}", name, args_preview),
                                    turn_id: Some(turn_id),
                                });
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

                                // Speculatively pre-execute read-only tools in background
                                self.speculative_executor
                                    .on_tool_call_streamed(turn_id, &tool_call)
                                    .await;

                                pending_tool_calls.push(tool_call);
                            }
                            Some(Ok(StreamChunk::Usage {
                                prompt_tokens,
                                completion_tokens,
                                cached_prompt_tokens,
                            })) => {
                                last_prompt_tokens = prompt_tokens;
                                cumulative_completion_tokens += completion_tokens;
                                last_cached_prompt_tokens = cached_prompt_tokens;
                                let live_used = prompt_tokens.saturating_add(completion_tokens);
                                let update_event = AgentEvent::ContextTokensUpdated {
                                    turn_id,
                                    used_tokens: live_used,
                                    cached_tokens: cached_prompt_tokens,
                                };
                                let _ = event_sender.send(update_event);
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
                        self.speculative_executor.cancel_all().await;
                        tracing::info!("Turn #{} cancelled during stream consumption", turn_id);
                        was_cancelled = true;
                        break;
                    }

                    if let Some(err) = stream_error {
                        const BENIGN_STREAM_EOF_PATTERNS: &[&str] = &[
                            "stream ended",
                            "connection reset",
                            "broken pipe",
                            "unexpected eof",
                            "connection closed",
                            "error decoding response body",
                            "incomplete message",
                            "channel closed",
                        ];
                        let err_msg = err.to_string().to_lowercase();
                        let is_benign_eof = BENIGN_STREAM_EOF_PATTERNS
                            .iter()
                            .any(|pat| err_msg.contains(pat))
                            && !err_msg.contains("incomplete tool call");

                        let visible_text =
                            crate::agent::prompt::strip_thought_blocks(&iteration_text);
                        let has_unfinished_plan =
                            crate::context::memory::working_memory::WorkingMemory::new(
                                &self.workspace_root,
                            )
                            .read_active_plan_summary()
                            .map(|s| s.total_tasks > 0 && s.completed_tasks < s.total_tasks)
                            .unwrap_or(false);

                        if !pending_tool_calls.is_empty() {
                            tracing::info!(
                                "Tolerating trailing stream closure after receiving {} complete tool calls: {}",
                                pending_tool_calls.len(),
                                err_msg
                            );
                        } else if is_benign_eof
                            && !visible_text.trim().is_empty()
                            && !has_unfinished_plan
                        {
                            tracing::info!(
                                "Tolerating trailing stream closure after receiving response ({} visible chars): {}",
                                visible_text.len(),
                                err_msg
                            );
                        } else if retry_count < max_retries {
                            retry_count += 1;
                            let delay_secs = retry_count as u64 * RETRY_BACKOFF_SECS;
                            let retry_msg = format!(
                                "⚠ Rate limit or network error. Retrying in {}s (attempt {}/{})...",
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
                            let err_event = AgentEvent::Error {
                                turn_id: Some(turn_id),
                                code: "provider_error".to_string(),
                                message: err.to_string(),
                                retrying: false,
                                retry_after_ms: None,
                            };
                            let _ = self
                                .session_store
                                .append_event(&self.session_id, &err_event);
                            let _ = event_sender.send(err_event);
                            let end_event = AgentEvent::TurnEnd {
                                turn_id,
                                status: "error".to_string(),
                                total_tokens_used: last_prompt_tokens
                                    .saturating_add(cumulative_completion_tokens),
                                files_modified: turn_files_modified.clone(),
                                cached_prompt_tokens: last_cached_prompt_tokens,
                            };
                            let _ = self
                                .session_store
                                .append_event(&self.session_id, &end_event);
                            let _ = event_sender.send(end_event);
                            let _ =
                                crate::tools::browser::BrowserManager::shutdown_live_engine().await;
                            return Err(err);
                        }
                    }

                    success = true;
                } else {
                    // --- Non-streaming mode: collect full response, emit aggregated events ---
                    let nonstreaming_fut = self.run_nonstreaming_iteration(
                        &self.messages,
                        &tools,
                        &options,
                        &event_sender,
                        &self.session_id,
                        turn_id,
                    );
                    let nonstreaming_res = if let Some(cancel) = &cancel_token {
                        tokio::select! {
                            _ = cancel.cancelled() => {
                                was_cancelled = true;
                                break;
                            }
                            res = nonstreaming_fut => res,
                        }
                    } else {
                        nonstreaming_fut.await
                    };

                    match nonstreaming_res {
                        Ok((iter_text, tool_calls, last_pt, cum_ct)) => {
                            iteration_text = iter_text;
                            pending_tool_calls = tool_calls;
                            last_prompt_tokens = last_pt;
                            cumulative_completion_tokens = cum_ct;
                            let live_used = last_pt.saturating_add(cum_ct);
                            let update_event = AgentEvent::ContextTokensUpdated {
                                turn_id,
                                used_tokens: live_used,
                                cached_tokens: last_cached_prompt_tokens,
                            };
                            let _ = event_sender.send(update_event);
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
                            let err_event = AgentEvent::Error {
                                turn_id: Some(turn_id),
                                code: "provider_error".to_string(),
                                message: e.to_string(),
                                retrying: false,
                                retry_after_ms: None,
                            };
                            let _ = self
                                .session_store
                                .append_event(&self.session_id, &err_event);
                            let _ = event_sender.send(err_event);
                            let end_event = AgentEvent::TurnEnd {
                                turn_id,
                                status: "error".to_string(),
                                total_tokens_used: last_prompt_tokens
                                    .saturating_add(cumulative_completion_tokens),
                                files_modified: turn_files_modified.clone(),
                                cached_prompt_tokens: last_cached_prompt_tokens,
                            };
                            let _ = self
                                .session_store
                                .append_event(&self.session_id, &end_event);
                            let _ = event_sender.send(end_event);
                            let _ =
                                crate::tools::browser::BrowserManager::shutdown_live_engine().await;
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
                let clean_text = crate::agent::prompt::strip_thought_blocks(&iteration_text);
                self.messages.push(Message::assistant_with_tools(
                    clean_text,
                    pending_tool_calls.clone(),
                ));
                crate::agent::subagent::relay::get_global_relay_store()
                    .sync_parent_messages(&self.workspace_root, &self.messages);

                let raw_stages =
                    crate::agent::speculative::ExecutionPlanner::plan(pending_tool_calls.clone());
                let mut stages = Vec::with_capacity(raw_stages.len());
                for st in raw_stages {
                    match st {
                        crate::agent::speculative::ExecutionStage::Parallel(batch)
                            if batch.iter().any(|c| {
                                c.name
                                    == crate::tools::registry::agent_tools::inquiry::ASK_USER_TOOL_NAME
                                    || self.requires_approval(&c.name, &c.arguments).is_some()
                            }) =>
                        {
                            for c in batch {
                                stages
                                    .push(crate::agent::speculative::ExecutionStage::Sequential(c));
                            }
                        }
                        other => stages.push(other),
                    }
                }

                for stage in stages {
                    // Check cancellation before each stage execution
                    if let Some(cancel) = &cancel_token {
                        if cancel.is_cancelled() {
                            was_cancelled = true;
                            self.speculative_executor.cancel_all().await;
                            tracing::info!("Turn #{} cancelled before stage execution", turn_id);
                            break;
                        }
                    }

                    match stage {
                        crate::agent::speculative::ExecutionStage::Parallel(parallel_calls) => {
                            let mut pre_snapshots: Vec<(bool, Option<String>)> =
                                Vec::with_capacity(parallel_calls.len());
                            for call in &parallel_calls {
                                turn_tool_calls.push(call.clone());
                                let opt_rel_path = crate::tools::param::opt_path(&call.arguments);
                                if let Some(path) = opt_rel_path {
                                    self.record_file_access(path);
                                }
                                let existed_before = opt_rel_path
                                    .map(|p| self.workspace_root.join(p).exists())
                                    .unwrap_or(false);
                                let before_content =
                                    if FILE_MODIFYING_TOOLS.contains(&call.name.as_str()) {
                                        match opt_rel_path {
                                            Some(rel) => {
                                                let full_path = self.workspace_root.join(rel);
                                                match tokio::fs::metadata(&full_path).await {
                                                Ok(meta)
                                                    if meta.len()
                                                        <= crate::constants::MAX_DIFF_SNAPSHOT_BYTES
                                                            as u64 =>
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
                                pre_snapshots.push((existed_before, before_content));
                            }

                            let hb_label = if parallel_calls.len() == 1 {
                                parallel_calls[0].name.clone()
                            } else {
                                format!("{} parallel tools", parallel_calls.len())
                            };
                            let _hb_guard = crate::agent::speculative::HeartbeatGuard::spawn(
                                event_sender.clone(),
                                turn_id,
                                hb_label,
                                1500,
                                2000,
                            );

                            let parallel_fut = self
                                .speculative_executor
                                .execute_parallel_stage_with_backup(
                                    &parallel_calls,
                                    turn_id,
                                    Some(&self.backup_manager),
                                );
                            let parallel_results = if let Some(cancel) = &cancel_token {
                                tokio::select! {
                                    _ = cancel.cancelled() => {
                                        was_cancelled = true;
                                        self.speculative_executor.cancel_all().await;
                                        break;
                                    }
                                    res = parallel_fut => res,
                                }
                            } else {
                                parallel_fut.await
                            };
                            drop(_hb_guard);

                            let mut batch_had_mutation = false;
                            let mut batch_reconciled = 0usize;
                            let used_tokens_est =
                                last_prompt_tokens.saturating_add(cumulative_completion_tokens);
                            for ((tool_call, tool_result), (file_existed_before, file_before)) in
                                parallel_calls
                                    .into_iter()
                                    .zip(parallel_results)
                                    .zip(pre_snapshots)
                            {
                                let (had_mutation, reconciled) = self.finalize_executed_tool(
                                    tool_call,
                                    tool_result,
                                    file_existed_before,
                                    file_before,
                                    turn_id,
                                    user_prompt,
                                    used_tokens_est,
                                    &event_sender,
                                    &mut options,
                                    &mut tools,
                                    &mut active_categories,
                                    &mcp_tools_by_server,
                                    &mut active_mcp_servers,
                                    &mut active_tool_names,
                                    &mut turn_files_modified,
                                    &mut items_progressed_this_turn,
                                    &mut circuit_tripped,
                                    &mut turn_tool_results,
                                    false,
                                )?;
                                batch_had_mutation |= had_mutation;
                                batch_reconciled += reconciled;
                            }

                            if batch_had_mutation {
                                let wm = crate::context::memory::working_memory::WorkingMemory::new(
                                    &self.workspace_root,
                                );
                                if let Some(summary) = wm.read_active_plan_summary() {
                                    if let Some(last_msg) = self.messages.last_mut() {
                                        if batch_reconciled > 0 {
                                            let rem_hint = wm.format_remaining_tasks_hint();
                                            if let Some(ref next_active) = summary.active_task {
                                                last_msg.content.push_str(&format!(
                                                    "\n\n{}\n[Plan Auto-Advancement: Steps 1..={} are ALREADY marked [x] ({}/{} completed — DO NOT call `update_progress` on them). Active step [>] is now #{}: \"{}\". Focus ONLY on this step.]",
                                                    rem_hint,
                                                    summary.completed_tasks,
                                                    summary.completed_tasks,
                                                    summary.total_tasks,
                                                    summary.completed_tasks.saturating_add(1),
                                                    next_active
                                                ));
                                            } else if summary.completed_tasks == summary.total_tasks
                                                && summary.total_tasks > 0
                                            {
                                                last_msg.content.push_str(&format!(
                                                    "\n\n{}\n[Plan Auto-Advancement: All {} tasks are ALREADY marked [x] (100% — DO NOT call `update_progress`). Next: run `power_review` + `power_verify`, then `archive_plan`.]",
                                                    rem_hint,
                                                    summary.total_tasks
                                                ));
                                            }
                                        } else if let Some(ref active) = summary.active_task {
                                            last_msg.content.push_str(&format!(
                                                "\n\n[Active Step ({}/{}): [>] \"{}\". Continue implementing the remaining deliverables for this step (plan status auto-advances when all step deliverables exist on disk).]",
                                                summary.completed_tasks.saturating_add(1),
                                                summary.total_tasks,
                                                active
                                            ));
                                        }
                                    }
                                }
                                let _ = crate::context::budget::MicroCompactor::compact_messages(
                                    &mut self.messages,
                                    1,
                                );
                            }

                            if circuit_tripped {
                                break;
                            }
                        }
                        crate::agent::speculative::ExecutionStage::Sequential(tool_call) => {
                            // Check cancellation before tool execution
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
                            if let Some(path) = crate::tools::param::opt_path(&tool_call.arguments)
                            {
                                self.record_file_access(path);
                            }

                            // === Inquiry gate: intercept ask_user tool calls ===
                            if tool_call.name
                                == crate::tools::registry::agent_tools::inquiry::ASK_USER_TOOL_NAME
                            {
                                let parsed_res =
                                    crate::agent::inquiry::InquiryRequest::parse_from_value(
                                        &tool_call.arguments,
                                    );
                                let request = match parsed_res {
                                    Ok(mut req) => {
                                        req.inquiry_id = tool_call.id.clone();
                                        req
                                    }
                                    Err(err_msg) => {
                                        let err_output = serde_json::json!({
                                            "error": err_msg
                                        })
                                        .to_string();
                                        let res_event = AgentEvent::ToolResult {
                                            turn_id,
                                            tool_id: tool_call.id.clone(),
                                            tool: tool_call.name.clone(),
                                            success: false,
                                            output: err_output.clone(),
                                            duration_ms: 0,
                                        };
                                        if let Err(e) = self
                                            .session_store
                                            .append_event(&self.session_id, &res_event)
                                        {
                                            tracing::warn!(
                                                "Failed to persist ToolResult event: {}",
                                                e
                                            );
                                        }
                                        let _ = event_sender.send(res_event);
                                        self.messages.push(Message::tool_result(
                                            tool_call.id.clone(),
                                            tool_call.name.clone(),
                                            err_output.clone(),
                                        ));
                                        turn_tool_results.push(crate::agent::types::ToolResult {
                                            tool_id: tool_call.id.clone(),
                                            tool_name: tool_call.name.clone(),
                                            success: false,
                                            output: err_output,
                                            display_output: String::new(),
                                            duration_ms: 0,
                                        });
                                        continue;
                                    }
                                };

                                if !self.interactive_approvals {
                                    // Non-interactive / headless auto-resolve
                                    let auto_resp = request.auto_resolve_defaults();
                                    if let Err(e) = crate::agent::decisions::record(
                                        &self.workspace_root,
                                        &request,
                                        &auto_resp,
                                        crate::agent::decisions::DecisionSource::AutoDefault,
                                    ) {
                                        tracing::warn!("Failed to record decisions: {}", e);
                                    }
                                    let output = auto_resp.into_tool_output();
                                    let res_event = AgentEvent::ToolResult {
                                        turn_id,
                                        tool_id: tool_call.id.clone(),
                                        tool: tool_call.name.clone(),
                                        success: true,
                                        output: output.clone(),
                                        duration_ms: 0,
                                    };
                                    if let Err(e) = self
                                        .session_store
                                        .append_event(&self.session_id, &res_event)
                                    {
                                        tracing::warn!("Failed to persist ToolResult event: {}", e);
                                    }
                                    let _ = event_sender.send(res_event);
                                    self.messages.push(Message::tool_result(
                                        tool_call.id.clone(),
                                        tool_call.name.clone(),
                                        output.clone(),
                                    ));
                                    turn_tool_results.push(crate::agent::types::ToolResult {
                                        tool_id: tool_call.id.clone(),
                                        tool_name: tool_call.name.clone(),
                                        success: true,
                                        output,
                                        display_output: String::new(),
                                        duration_ms: 0,
                                    });
                                    continue;
                                }

                                // Interactive suspension gate
                                let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<
                                    crate::agent::inquiry::InquiryResponse,
                                >();
                                self.pending_inquiries
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .insert(tool_call.id.clone(), resp_tx);

                                let inq_event = AgentEvent::UserInquiry {
                                    turn_id,
                                    tool_id: tool_call.id.clone(),
                                    request: request.clone(),
                                };
                                if let Err(e) = self
                                    .session_store
                                    .append_event(&self.session_id, &inq_event)
                                {
                                    tracing::warn!("Failed to persist UserInquiry event: {}", e);
                                }
                                let _ = event_sender.send(inq_event);

                                let response = if let Some(cancel) = &cancel_token {
                                    tokio::select! {
                                        _ = cancel.cancelled() => None,
                                        r = resp_rx => r.ok(),
                                    }
                                } else {
                                    resp_rx.await.ok()
                                };

                                self.pending_inquiries
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .remove(&tool_call.id);

                                let output = match response {
                                    Some(resp) => {
                                        if let Err(e) = crate::agent::decisions::record(
                                            &self.workspace_root,
                                            &request,
                                            &resp,
                                            crate::agent::decisions::DecisionSource::User,
                                        ) {
                                            tracing::warn!("Failed to record decisions: {}", e);
                                        }
                                        resp.into_tool_output()
                                    }
                                    None => serde_json::json!({
                                        "status": "cancelled",
                                        "message": "Inquiry cancelled or timed out."
                                    })
                                    .to_string(),
                                };

                                let res_event = AgentEvent::ToolResult {
                                    turn_id,
                                    tool_id: tool_call.id.clone(),
                                    tool: tool_call.name.clone(),
                                    success: true,
                                    output: output.clone(),
                                    duration_ms: 0,
                                };
                                if let Err(e) = self
                                    .session_store
                                    .append_event(&self.session_id, &res_event)
                                {
                                    tracing::warn!("Failed to persist ToolResult event: {}", e);
                                }
                                let _ = event_sender.send(res_event);
                                self.messages.push(Message::tool_result(
                                    tool_call.id.clone(),
                                    tool_call.name.clone(),
                                    output.clone(),
                                ));
                                turn_tool_results.push(crate::agent::types::ToolResult {
                                    tool_id: tool_call.id.clone(),
                                    tool_name: tool_call.name.clone(),
                                    success: true,
                                    output,
                                    display_output: String::new(),
                                    duration_ms: 0,
                                });
                                continue;
                            }

                            // === Approval gate: pause for user decision on dangerous or mandatory-confirm tools ===
                            if let Some(approval_reason) =
                                self.requires_approval(&tool_call.name, &tool_call.arguments)
                            {
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
                                    reason: approval_reason,
                                };
                                if let Err(e) = self
                                    .session_store
                                    .append_event(&self.session_id, &req_event)
                                {
                                    tracing::warn!(
                                        "Failed to persist ApprovalRequest event: {}",
                                        e
                                    );
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

                            // Check whether the target file already existed before dispatch
                            let opt_rel_path = crate::tools::param::opt_path(&tool_call.arguments);
                            let file_existed_before = opt_rel_path
                                .map(|p| self.workspace_root.join(p).exists())
                                .unwrap_or(false);

                            // Snapshot file content before dispatch for inline diff preview
                            let file_before: Option<String> =
                                if FILE_MODIFYING_TOOLS.contains(&tool_call.name.as_str()) {
                                    match opt_rel_path {
                                        Some(rel) => {
                                            let full_path = self.workspace_root.join(rel);
                                            match tokio::fs::metadata(&full_path).await {
                                                Ok(meta)
                                                    if meta.len()
                                                        <= crate::constants::MAX_DIFF_SNAPSHOT_BYTES
                                                            as u64 =>
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

                            // === Pre-Fanout Context Pressure Guard (72% threshold) ===
                            // Proactively compact & checkpoint parent context before launching heavy subagent/swarm fan-out
                            if matches!(
                                tool_call.name.as_str(),
                                "spawn_subagent"
                                    | "delegate_fanout"
                                    | "run_swarm"
                                    | "council_deliberate"
                            ) {
                                let ctx_limit = self.compactor.model_token_limit().max(1);
                                let est_tokens = last_prompt_tokens
                                    .saturating_add(cumulative_completion_tokens)
                                    .max(self.compactor.count_tokens(&self.messages));
                                let pct_used = (est_tokens.saturating_mul(100)) / ctx_limit;
                                if pct_used >= 72 {
                                    tracing::info!(
                                        tool = %tool_call.name,
                                        pct_used,
                                        "Pre-fanout context pressure guard triggered (>= 72%); compacting before subagent fan-out"
                                    );
                                    let _ =
                                        crate::context::budget::MicroCompactor::compact_messages(
                                            &mut self.messages,
                                            1,
                                        );
                                    if let Some(metrics) = self.prune_context() {
                                        let compact_event = AgentEvent::ContextCompacted {
                                            turn_id,
                                            tier: metrics.tier,
                                            turns_summarized: metrics.turns_summarized,
                                            tokens_before: metrics.tokens_before,
                                            tokens_after: metrics.tokens_after,
                                            savings_percent: metrics.savings_percent,
                                        };
                                        let _ = self
                                            .session_store
                                            .append_event(&self.session_id, &compact_event);
                                        let _ = event_sender.send(compact_event);
                                    }
                                }
                            }

                            // Execute tool (MCP vs Built-in)
                            let is_mcp = tool_call.name.starts_with(MCP_TOOL_PREFIX);
                            let mcp_client = self.mcp_client.clone();
                            let ws_root = self.workspace_root.clone();
                            let backup_mgr = self.backup_manager.clone();
                            let tool_id = tool_call.id.clone();
                            let tool_name = tool_call.name.clone();
                            let tool_args = tool_call.arguments.clone();

                            let tool_fut = async move {
                                if is_mcp {
                                    let start = std::time::Instant::now();
                                    let res = mcp_client.call_tool(&tool_name, &tool_args).await;
                                    let duration_ms = start.elapsed().as_millis() as u64;
                                    match res {
                                        Ok(output) => crate::agent::types::ToolResult {
                                            tool_id,
                                            tool_name,
                                            success: true,
                                            output,
                                            display_output: String::new(),
                                            duration_ms,
                                        },
                                        Err(e) => crate::agent::types::ToolResult {
                                            tool_id,
                                            tool_name,
                                            success: false,
                                            output: format!("MCP tool error: {}", e),
                                            display_output: String::new(),
                                            duration_ms,
                                        },
                                    }
                                } else {
                                    crate::tools::ToolRegistry::dispatch(
                                        &ws_root,
                                        &tool_id,
                                        &tool_name,
                                        &tool_args,
                                        Some(&backup_mgr),
                                        turn_id,
                                    )
                                    .await
                                }
                            };

                            let _hb_guard = crate::agent::speculative::HeartbeatGuard::spawn(
                                event_sender.clone(),
                                turn_id,
                                tool_call.name.clone(),
                                1500,
                                2000,
                            );
                            let tool_result = if let Some(cancel) = &cancel_token {
                                tokio::select! {
                                    _ = cancel.cancelled() => {
                                        was_cancelled = true;
                                        Self::emit_rejected_tool_result(
                                            self,
                                            &event_sender,
                                            turn_id,
                                            &tool_call,
                                            "cancelled by user (Esc)",
                                            &mut turn_tool_results,
                                        );
                                        break;
                                    }
                                    res = tool_fut => res,
                                }
                            } else {
                                tool_fut.await
                            };
                            drop(_hb_guard);

                            let used_tokens_est =
                                last_prompt_tokens.saturating_add(cumulative_completion_tokens);
                            self.finalize_executed_tool(
                                tool_call,
                                tool_result,
                                file_existed_before,
                                file_before,
                                turn_id,
                                user_prompt,
                                used_tokens_est,
                                &event_sender,
                                &mut options,
                                &mut tools,
                                &mut active_categories,
                                &mcp_tools_by_server,
                                &mut active_mcp_servers,
                                &mut active_tool_names,
                                &mut turn_files_modified,
                                &mut items_progressed_this_turn,
                                &mut circuit_tripped,
                                &mut turn_tool_results,
                                true,
                            )?;
                            if circuit_tripped {
                                break;
                            }
                        }
                    }

                    // If a live user prompt arrived during this stage, defer any remaining queued stages
                    // in this iteration so the Orchestrator immediately answers the user's live prompt.
                    if crate::dev::registry::get_global_dev_registry().has_steering_prompts() {
                        break;
                    }
                }

                if was_cancelled || circuit_tripped {
                    // Guarantee that every tool call in pending_tool_calls has a corresponding tool result in self.messages
                    for call in &pending_tool_calls {
                        if !turn_tool_results.iter().any(|r| r.tool_id == call.id) {
                            Self::emit_rejected_tool_result(
                                self,
                                &event_sender,
                                turn_id,
                                call,
                                if circuit_tripped {
                                    "aborted due to circuit breaker trip"
                                } else {
                                    "cancelled before dispatch"
                                },
                                &mut turn_tool_results,
                            );
                        }
                    }
                    break;
                }

                // If we broke out of `stages` early to answer a live user steering prompt, ensure every
                // tool call in `pending_tool_calls` has a matching tool result message for LLM protocol integrity.
                for call in &pending_tool_calls {
                    if !turn_tool_results.iter().any(|r| r.tool_id == call.id) {
                        Self::emit_rejected_tool_result(
                            self,
                            &event_sender,
                            turn_id,
                            call,
                            "deferred to handle live user prompt immediately (background tasks continue running in minitask)",
                            &mut turn_tool_results,
                        );
                    }
                }

                // === Mid-Turn Reactive MiniPub Boomerang Inbox ===
                // If any background `MiniPub` worker(s) completed while the Orchestrator was executing
                // tools in this iteration, inject their Boomerang completion reports immediately into the
                // latest tool result so the Orchestrator can react on the very next step without waiting for turn exit.
                if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                    if let Some((completed_ids, boomerang_block)) =
                        pool.harvest_newly_completed_boomerang_block().await
                    {
                        if let Some(last_msg) = self.messages.last_mut() {
                            last_msg.content.push_str(&format!(
                                "\n\n[SYSTEM - MID-TURN MINIPUB BOOMERANG COMPLETION REPORT ({})]\n{}\n",
                                completed_ids.join(", "),
                                boomerang_block
                            ));
                        }
                        let _ = event_sender.send(AgentEvent::Heartbeat {
                            turn_id: Some(turn_id),
                            status: format!(
                                "MiniPub Boomerang: {} completed",
                                completed_ids.join(", ")
                            ),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        });
                    }
                }

                // === Mid-Turn Reactive MiniTask Background Command Inbox ===
                // If any non-daemon background build, compile, test, bundle, or script task in `MiniDevRegistry`
                // (`minitask`) finished while the Orchestrator was executing other tools, inject its completion
                // report and output immediately so the Orchestrator can react on the very next LLM call.
                if let Some((completed_task_ids, tasks_block)) =
                    crate::dev::registry::get_global_dev_registry()
                        .harvest_newly_completed_tasks_block(&self.workspace_root)
                        .await
                {
                    if let Some(last_msg) = self.messages.last_mut() {
                        last_msg.content.push_str(&format!(
                            "\n\n[SYSTEM - MID-TURN MINITASK COMPLETION REPORT ({} task(s): {})]\n\
                             Background command task(s) in `minitask` finished while you were executing other tools:\n\n{}\n\n\
                             You can react to these completed build/test results immediately in your next step.",
                            completed_task_ids.len(),
                            completed_task_ids.join(", "),
                            tasks_block
                        ));
                    }
                    let _ = event_sender.send(AgentEvent::Heartbeat {
                        turn_id: Some(turn_id),
                        status: format!("MiniTask: {} completed", completed_task_ids.join(", ")),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }

                // === Mid-Turn Live User Steering Inbox ===
                // If the user submitted a live prompt in the TUI while tools or background `minitask`
                // processes were running, inject it immediately with active `minitask` telemetry and
                // `todo.md` continuation state so the Orchestrator can triage without interrupting builds.
                let mid_turn_steering =
                    crate::dev::registry::get_global_dev_registry().drain_steering_prompts();
                if !mid_turn_steering.is_empty() {
                    let steering_block = self.build_live_steering_block(&mid_turn_steering).await;
                    if let Some(last_msg) = self.messages.last_mut() {
                        last_msg.content.push_str("\n\n");
                        last_msg.content.push_str(&steering_block);
                    }
                    let _ = event_sender.send(AgentEvent::Heartbeat {
                        turn_id: Some(turn_id),
                        status: "Live user prompt received — triaging asynchronously".to_string(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }

                // Emit updated real-time context token count after tool results are added
                let current_context_tokens = self.count_current_context_tokens();
                let live_used =
                    current_context_tokens.max(last_prompt_tokens + cumulative_completion_tokens);
                let update_event = AgentEvent::ContextTokensUpdated {
                    turn_id,
                    used_tokens: live_used,
                    cached_tokens: last_cached_prompt_tokens,
                };
                let _ = event_sender.send(update_event);

                // Check if we hit an explicit iteration limit while work was actively occurring
                if max_iterations > 0 && iteration >= max_iterations {
                    if auto_continues_remaining > 0 {
                        auto_continues_remaining -= 1;
                        let extend_amount = self.config.agent.max_tool_iterations.max(1);
                        max_iterations = max_iterations.saturating_add(extend_amount);
                        tracing::info!(
                            turn_id,
                            extended_max = max_iterations,
                            remaining_continues = auto_continues_remaining,
                            "Auto-continuing turn execution: iteration limit extended"
                        );
                        let notice = format!(
                            "\n\n🔄 *Auto-continuing execution (tool iteration limit extended to {})*...\n",
                            max_iterations
                        );
                        let _ = event_sender.send(AgentEvent::StreamDelta {
                            turn_id,
                            delta: notice,
                        });
                    } else {
                        tracing::warn!(
                            turn_id,
                            max_iterations,
                            "Turn reached tool iteration limit with active tool executions; pausing"
                        );
                        hit_iteration_limit = true;
                        let pause_msg = format!(
                            "\n\n⚠️ *Reached tool iteration limit ({}). Pausing turn. Type 'continue' to resume.*",
                            max_iterations
                        );
                        let _ = event_sender.send(AgentEvent::StreamDelta {
                            turn_id,
                            delta: pause_msg.clone(),
                        });
                        turn_response.push_str(&pause_msg);
                        self.messages.push(Message::assistant(pause_msg.trim()));
                        break;
                    }
                }
            } else {
                if was_cancelled || circuit_tripped {
                    break;
                }

                let clean_text = crate::agent::prompt::strip_thought_blocks(&iteration_text);
                let any_cancelled_tool = turn_tool_results.iter().any(|r| {
                    r.output.contains("cancelled") || r.output.contains("rejected by user")
                });
                let asked_user_recently = turn_tool_calls
                    .last()
                    .map(|t| t.name == "ask_user")
                    .unwrap_or(false);

                // === Reactive MiniPub & MiniTask Background Barrier & Live Steering Wakeup ===
                // Run BEFORE the 4-Gate Pre-Completion Verification Barrier so that:
                // 1. If the Orchestrator just answered a live user prompt (`> whats happening`) while
                //    background `MiniPub` subagents or `minitask` builds are still running, we seamlessly
                //    resume awaiting the background workers (without running pre-completion verification
                //    while workers are still in flight!).
                // 2. If another live user steering prompt arrives while awaiting background workers,
                //    `wait_until_active_workers_settle` wakes in <1ms and the Orchestrator answers immediately.
                if !was_cancelled && !circuit_tripped && !any_cancelled_tool && !asked_user_recently
                {
                    let dev_registry = crate::dev::registry::get_global_dev_registry();

                    let pool_opt = crate::agent::subagent::try_get_global_subagent_pool();
                    let active_workers = pool_opt
                        .as_ref()
                        .map(|p| p.active_worker_count())
                        .unwrap_or(0);
                    let active_tasks = dev_registry.active_non_daemon_task_count().await;

                    if (active_workers > 0 || active_tasks > 0)
                        && !dev_registry.has_steering_prompts()
                    {
                        let status_msg = match (active_workers > 0, active_tasks > 0) {
                            (true, true) => format!(
                                "Awaiting {} active MiniPub worker(s) and {} background task(s)...",
                                active_workers, active_tasks
                            ),
                            (true, false) => format!(
                                "Awaiting {} active MiniPub background worker(s)...",
                                active_workers
                            ),
                            (false, true) => format!(
                                "Awaiting {} active background task(s) in minitask...",
                                active_tasks
                            ),
                            (false, false) => String::new(),
                        };
                        let _ = event_sender.send(AgentEvent::Heartbeat {
                            turn_id: Some(turn_id),
                            status: status_msg,
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        });
                        let wait_timeout = std::time::Duration::from_secs(
                            self.config
                                .agent
                                .minipub
                                .inactivity_timeout_secs
                                .saturating_mul(2)
                                .max(180),
                        );
                        let wait_fut = async {
                            match (pool_opt.as_ref(), active_workers > 0, active_tasks > 0) {
                                (Some(pool), true, true) => {
                                    tokio::select! {
                                        _ = pool.wait_until_active_workers_settle(wait_timeout) => {}
                                        _ = dev_registry.wait_until_active_tasks_settle(wait_timeout) => {}
                                    }
                                }
                                (Some(pool), true, false) => {
                                    pool.wait_until_active_workers_settle(wait_timeout).await;
                                }
                                (_, false, true) => {
                                    dev_registry
                                        .wait_until_active_tasks_settle(wait_timeout)
                                        .await;
                                }
                                _ => {}
                            }
                        };
                        if let Some(ref cancel) = cancel_token {
                            tokio::select! {
                                _ = wait_fut => {}
                                _ = cancel.cancelled() => {
                                    was_cancelled = true;
                                }
                            }
                        } else {
                            wait_fut.await;
                        }
                    }

                    if !was_cancelled {
                        let completed_minipubs = if let Some(pool) =
                            crate::agent::subagent::try_get_global_subagent_pool()
                        {
                            pool.harvest_newly_completed_boomerang_block().await
                        } else {
                            None
                        };
                        let completed_tasks = dev_registry
                            .harvest_newly_completed_tasks_block(&self.workspace_root)
                            .await;
                        let steering_prompts = dev_registry.drain_steering_prompts();

                        if completed_minipubs.is_some()
                            || completed_tasks.is_some()
                            || !steering_prompts.is_empty()
                        {
                            if !clean_text.trim().is_empty() {
                                self.messages.push(Message::assistant(clean_text));
                            }
                            let mut combined_wakeup = String::new();
                            if let Some((_completed_ids, minipub_block)) = completed_minipubs {
                                combined_wakeup.push_str(&format!(
                                    "[SYSTEM - MINIPUB BOOMERANG COMPLETION REPORT]\n\
                                     Background `MiniPub` subagent worker(s) have completed execution:\n\n{}\n\n\
                                     Synthesize their findings or proceed with the next orchestration step.\n\n",
                                    minipub_block
                                ));
                            }
                            if let Some((_completed_task_ids, tasks_block)) = completed_tasks {
                                combined_wakeup.push_str(&format!(
                                    "[SYSTEM - MINITASK BACKGROUND TASK COMPLETION REPORT]\n\
                                     Background build/compile/test/command task(s) in `minitask` have completed execution:\n\n{}\n\n\
                                     Inspect the exit status and output above and proceed with the next step.\n\n",
                                    tasks_block
                                ));
                            }
                            if !steering_prompts.is_empty() {
                                let steering_block =
                                    self.build_live_steering_block(&steering_prompts).await;
                                combined_wakeup.push_str(&steering_block);
                            }
                            self.messages
                                .push(Message::user(combined_wakeup.trim().to_string()));
                            continue;
                        }
                    } else {
                        break;
                    }
                }

                // === 4-Gate Pre-Completion Verification Barrier (Phase 89 & Superpowers Protocol) ===
                let had_untracked_mutation = turn_tool_calls.iter().any(|tc| {
                    matches!(
                        tc.name.as_str(),
                        "exec_cmd"
                            | "kit_stack_add"
                            | "block_scaffold"
                            | "block_insert"
                            | "replace_in_files"
                            | "ast_grep_rewrite"
                    ) || (tc.name == "ast_refactor"
                        && !tc
                            .arguments
                            .get("dry_run")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                        && !matches!(
                            tc.arguments.get("action").and_then(|v| v.as_str()),
                            Some("scan_symbol" | "find_unused")
                        ))
                });
                let files_to_verify: Vec<String> = if !turn_files_modified.is_empty() {
                    turn_files_modified.clone()
                } else if had_untracked_mutation {
                    let git = crate::git::GitService::new(self.workspace_root.clone());
                    if git.is_git_repo().await {
                        if let Ok(st) = git.get_status().await {
                            let mut all = st.staged;
                            all.extend(st.unstaged);
                            all.sort();
                            all.dedup();
                            all
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                if !files_to_verify.is_empty()
                    && !circuit_tripped
                    && heal_attempts < crate::constants::VERIFICATION_MAX_ATTEMPTS
                {
                    let report = crate::agent::verification_barrier::VerificationBarrier::verify(
                        &self.workspace_root,
                        &files_to_verify,
                    )
                    .await;

                    if !report.all_passed {
                        if let Some(summary) = report.first_failure_summary() {
                            last_verification_failure = Some(summary);
                        }
                        heal_attempts += 1;
                        if heal_attempts >= crate::constants::VERIFICATION_MAX_ATTEMPTS {
                            tracing::warn!(
                                "Pre-completion verification barrier failed after {} attempts. Preserving workspace changes and committing transaction snapshot.",
                                heal_attempts
                            );
                            if let Ok(Some(active_tx)) =
                                crate::session::transaction::TransactionManager::get_active(
                                    &self.workspace_root,
                                )
                            {
                                // Preserve user/agent work on disk rather than wiping created/modified files;
                                // the pre-turn backup remains available for manual `/tx rollback` or `/undo`.
                                let _ = crate::session::transaction::TransactionManager::commit(
                                    &self.workspace_root,
                                    Some(&active_tx.tx_id),
                                );
                            }

                            // Record Discarded Approach / Negative Knowledge in .minicode/NOTES.md
                            let wm = crate::context::working_memory::WorkingMemory::new(
                                &self.workspace_root,
                            );
                            let failed_summary = format!(
                                "Approach failed verification barrier on files: {}. Remediation failure summary: {}",
                                files_to_verify.join(", "),
                                report
                                    .format_remediation_prompt()
                                    .lines()
                                    .take(3)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            );
                            let _ = wm.append_note(
                                crate::context::memory::working_memory::NoteSection::NegativeKnowledge,
                                &failed_summary,
                            );

                            let error_feedback = format!(
                                "Verification barrier failed after {} auto-heal attempts. Workspace files have been preserved on disk (use `/undo` or `/tx rollback` if you wish to revert).\nRecorded diagnostic summary in `.minicode/NOTES.md`. Please review the remaining errors and resolve them.",
                                heal_attempts
                            );
                            let status_event = AgentEvent::ToolResult {
                                turn_id,
                                tool_id: format!("verification_barrier_{}", heal_attempts),
                                tool: "verification_barrier".to_string(),
                                success: false,
                                output: error_feedback.clone(),
                                duration_ms: 50,
                            };
                            let _ = event_sender.send(status_event);

                            let clean_text =
                                crate::agent::prompt::strip_thought_blocks(&iteration_text);
                            self.messages.push(Message::assistant(clean_text));
                            self.messages.push(Message::user(error_feedback));
                            break;
                        }

                        let error_feedback = report.format_remediation_prompt();
                        let error_feedback = crate::sandbox::redact::SecretRedactor::global()
                            .redact(&error_feedback);
                        tracing::warn!(
                            "Pre-completion verification barrier rejected completion, attempt #{}/{}",
                            heal_attempts,
                            crate::constants::VERIFICATION_MAX_ATTEMPTS
                        );
                        let status_event = AgentEvent::ToolResult {
                            turn_id,
                            tool_id: format!("verification_barrier_{}", heal_attempts),
                            tool: "verification_barrier".to_string(),
                            success: false,
                            output: error_feedback.clone(),
                            duration_ms: 50,
                        };
                        let _ = event_sender.send(status_event);

                        let clean_text =
                            crate::agent::prompt::strip_thought_blocks(&iteration_text);
                        self.messages.push(Message::assistant(clean_text));
                        self.messages.push(Message::user(error_feedback));
                        continue;
                    } else {
                        // Verification passed! Commit active transaction and sync task progress
                        if let Ok(Some(_)) =
                            crate::session::transaction::TransactionManager::get_active(
                                &self.workspace_root,
                            )
                        {
                            let _ = crate::session::transaction::TransactionManager::commit(
                                &self.workspace_root,
                                None,
                            );
                        }
                        let wm = crate::context::working_memory::WorkingMemory::new(
                            &self.workspace_root,
                        );
                        let _reconciled = wm.reconcile_workspace_tasks(&turn_files_modified);
                        self.emit_current_plan(Some(turn_id), &event_sender);

                        // Autonomous Self-Improvement / Reflexion Hook:
                        // If this task required heal_attempts > 0 to pass verification,
                        // extract the hard-won lesson and persist it to universal learned gotchas.
                        if heal_attempts > 0 {
                            let tech_scope = turn_files_modified
                                .first()
                                .and_then(|f| Path::new(f).extension().and_then(|e| e.to_str()))
                                .map(|ext| match ext {
                                    "rs" => "Rust",
                                    "ts" | "tsx" => "TypeScript/React",
                                    "js" | "jsx" => "JavaScript",
                                    "py" => "Python",
                                    "go" => "Go",
                                    "toml" | "json" | "yaml" | "yml" => "Config",
                                    _ => "General",
                                })
                                .unwrap_or("General");

                            let (trigger, failed_attempt, verified_fix) = if let Some((
                                ref fail_reason,
                                ref remediation,
                            )) =
                                last_verification_failure
                            {
                                (
                                        fail_reason.clone(),
                                        format!(
                                            "Edit on {} failed verification: {}",
                                            turn_files_modified.join(", "),
                                            fail_reason
                                        ),
                                        format!(
                                            "Applied remediation ({}) and verified all gates pass on {}.",
                                            remediation,
                                            turn_files_modified.join(", ")
                                        ),
                                    )
                            } else {
                                (
                                        format!(
                                            "Initial modifications to {} failed verification barrier ({} auto-heal iterations)",
                                            turn_files_modified.join(", "),
                                            heal_attempts
                                        ),
                                        format!(
                                            "Initial code edit did not pass verification gates on {}",
                                            turn_files_modified.join(", ")
                                        ),
                                        format!(
                                            "Auto-healed and verified: all tests, lints, and syntax gates succeeded on {}.",
                                            turn_files_modified.join(", ")
                                        ),
                                    )
                            };

                            let gotcha = crate::vault::models::GlobalGotcha {
                                id: format!("gotcha-{}", uuid::Uuid::new_v4().simple()),
                                trigger,
                                context_scope: tech_scope.to_string(),
                                failed_attempt,
                                verified_fix,
                                created_at: chrono::Utc::now().to_rfc3339(),
                                occurrence_count: 1,
                            };

                            let store = crate::vault::store::VaultStore::new(&self.workspace_root);
                            if let Err(e) = store.add_or_update_global_gotcha(gotcha) {
                                tracing::debug!("Failed to persist auto-learned gotcha: {}", e);
                            } else {
                                tracing::info!(
                                    scope = %tech_scope,
                                    "Autonomous Reflexion: recorded hard-won lesson into universal vault gotchas"
                                );
                            }
                        }
                    }
                } else if !turn_files_modified.is_empty()
                    && self.config.agent.auto_heal
                    && heal_attempts < 2
                {
                    if let Ok(diag_report) =
                        crate::lsp::LspEngine::run_diagnostics(&self.workspace_root).await
                    {
                        if !diag_report.is_clean() && !diag_report.errors.is_empty() {
                            if let Some(first_err) = diag_report.errors.first() {
                                let diag_summary = crate::utils::strings::truncate_ellipsis(
                                    &format!(
                                        "[LSP/Compiler] {}:{}: {}",
                                        first_err.file.display(),
                                        first_err.line,
                                        first_err
                                            .message
                                            .lines()
                                            .next()
                                            .unwrap_or(&first_err.message)
                                    ),
                                    220,
                                );
                                last_verification_failure = Some((
                                    diag_summary,
                                    "Resolve the compiler/type error reported by LSP diagnostics."
                                        .to_string(),
                                ));
                            }
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

                            let clean_text =
                                crate::agent::prompt::strip_thought_blocks(&iteration_text);
                            self.messages.push(Message::assistant(clean_text));
                            self.messages.push(Message::user(error_feedback));
                            continue;
                        }
                    }
                }

                // No more tool calls and workspace compiles cleanly; reconcile deliverables first
                let wm = crate::context::memory::working_memory::WorkingMemory::new(
                    &self.workspace_root,
                );
                let _ = wm.reconcile_workspace_tasks(&turn_files_modified);
                self.emit_current_plan(Some(turn_id), &event_sender);

                // === Boulder State Todo & TaskDAG Continuation Enforcer (Phase 175) ===
                // If an active plan (.minicode/plan/task_plan.md / todo.md) or TaskDAG (.minicode/task_dag.json)
                // still has unfinished tasks and the model stopped mid-execution (without asking the user),
                // inject a structured Boulder continuation reminder so it finishes all planned steps.
                if !was_cancelled
                    && !circuit_tripped
                    && !any_cancelled_tool
                    && !asked_user_recently
                    && plan_continuation_nudges < max_boulder_nudges
                    && (!turn_tool_calls.is_empty() || !clean_text.trim().is_empty())
                {
                    let allow_boulder_plan = is_ralph_loop
                        || wm.has_fresh_task_plan(crate::constants::BOULDER_MAX_PLAN_AGE_SECS);
                    if allow_boulder_plan {
                        if let Some(plan_summary) = wm.read_active_plan_summary() {
                            if plan_summary.total_tasks > 0
                                && plan_summary.completed_tasks < plan_summary.total_tasks
                            {
                                plan_continuation_nudges += 1;
                                let _ =
                                    crate::agent::boulder::BoulderState::record_continuation_step(
                                        &self.workspace_root,
                                        &clean_text,
                                    );
                                let _ = crate::agent::boulder::BoulderState::sync_from_workspace(
                                    &self.workspace_root,
                                    is_ralph_loop,
                                    max_boulder_nudges,
                                );
                                let remaining = plan_summary
                                    .total_tasks
                                    .saturating_sub(plan_summary.completed_tasks);
                                let next_step = plan_summary
                                    .active_task
                                    .unwrap_or_else(|| "the next incomplete step".to_string());
                                tracing::info!(
                                    completed = plan_summary.completed_tasks,
                                    total = plan_summary.total_tasks,
                                    nudge = plan_continuation_nudges,
                                    max_nudges = max_boulder_nudges,
                                    "Boulder Todo Enforcer: active plan has incomplete tasks ({}/{}); nudging continuation",
                                    plan_summary.completed_tasks,
                                    plan_summary.total_tasks
                                );
                                let _ = event_sender.send(AgentEvent::Heartbeat {
                                    turn_id: Some(turn_id),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                    status: format!(
                                        "toast:boulder\tBoulder Continuation ({}/{})\tAdvancing to `{}` ({} remaining)",
                                        plan_continuation_nudges,
                                        max_boulder_nudges,
                                        next_step,
                                        remaining
                                    ),
                                });
                                if !clean_text.trim().is_empty() {
                                    self.messages.push(Message::assistant(clean_text));
                                }
                                self.messages.push(Message::user(format!(
                                    "[SYSTEM REMINDER - BOULDER TODO CONTINUATION ({}/{})]\n\
                                     Incomplete tasks remain in your active plan ({} of {} completed, {} remaining).\n\
                                     • Active Step `[>]`: `{}`\n\
                                     • Action Required: Proceed immediately to execute `{}` using the required tools (or delegate to a `MiniPub` worker via `dispatch_subagent` / `delegate_task`), then call `update_progress` as each step finishes. If all tasks are already verified complete, call `archive_plan`.",
                                    plan_continuation_nudges,
                                    max_boulder_nudges,
                                    plan_summary.completed_tasks,
                                    plan_summary.total_tasks,
                                    remaining,
                                    next_step,
                                    next_step
                                )));
                                continue;
                            }
                        }
                    }

                    // Also check active TaskDAG (.minicode/task_dag.json) for unblocked incomplete tasks
                    let dag_path = self.workspace_root.join(".minicode").join("task_dag.json");
                    let allow_boulder_dag = is_ralph_loop
                        || std::fs::metadata(&dag_path)
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|mtime| mtime.elapsed().ok())
                            .is_some_and(|elapsed| {
                                elapsed.as_secs() <= crate::constants::BOULDER_MAX_PLAN_AGE_SECS
                            });
                    if allow_boulder_dag {
                        if let Ok(dag) = crate::agent::task_dag::TaskDag::load(&self.workspace_root)
                        {
                            if !dag.tasks.is_empty() {
                                let unblocked = dag.next_executable_tasks();
                                if !unblocked.is_empty() {
                                    plan_continuation_nudges += 1;
                                    let completed_count = dag
                                        .tasks
                                        .values()
                                        .filter(|t| {
                                            t.status
                                                == crate::agent::task_dag::TaskStatus::Completed
                                        })
                                        .count();
                                    let total_count = dag.tasks.len();
                                    let next_dag_task = &unblocked[0];
                                    tracing::info!(
                                        completed = completed_count,
                                        total = total_count,
                                        next_task_id = %next_dag_task.id,
                                        nudge = plan_continuation_nudges,
                                        "Boulder DAG Enforcer: active TaskDAG has unblocked incomplete tasks; nudging continuation"
                                    );
                                    if !clean_text.trim().is_empty() {
                                        self.messages.push(Message::assistant(clean_text));
                                    }
                                    self.messages.push(Message::user(format!(
                                        "[SYSTEM REMINDER - BOULDER DAG CONTINUATION ({}/{})]\n\
                                         Your active Task DAG has {}/{} tasks completed and {} unblocked task(s) ready for execution.\n\
                                         • Next Unblocked Task: `{}` — **{}**\n\
                                         • Action Required: Execute `{}` now and mark it complete via `complete_task(task_id=\"{}\", status=\"completed\")`.",
                                        plan_continuation_nudges,
                                        max_boulder_nudges,
                                        completed_count,
                                        total_count,
                                        unblocked.len(),
                                        next_dag_task.id,
                                        next_dag_task.title,
                                        next_dag_task.id,
                                        next_dag_task.id
                                    )));
                                    continue;
                                }
                            }
                        }
                    }

                    // Self-Referential Ralph Loop (`ralph:` / `ulw-loop:`): continue until `<promise>DONE</promise>`
                    if is_ralph_loop
                        && !crate::agent::boulder::BoulderState::contains_completion_promise(
                            &clean_text,
                        )
                    {
                        plan_continuation_nudges += 1;
                        let _ = crate::agent::boulder::BoulderState::record_continuation_step(
                            &self.workspace_root,
                            &clean_text,
                        );
                        let _ = event_sender.send(AgentEvent::Heartbeat {
                            turn_id: Some(turn_id),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            status: format!(
                                "toast:ralph\tRalph Loop ({}/{})\tAwaiting verification & <promise>DONE</promise>",
                                plan_continuation_nudges, max_boulder_nudges
                            ),
                        });
                        if !clean_text.trim().is_empty() {
                            self.messages.push(Message::assistant(clean_text));
                        }
                        self.messages.push(Message::user(format!(
                            "[SYSTEM REMINDER - RALPH LOOP ENFORCER ({}/{})]\n\
                             You are running in `Ralph Loop` mode and have not yet emitted `<promise>DONE</promise>`.\n\
                             Verify that 100% of requirements and `power_verify` gates pass, then conclude with `<promise>DONE</promise>`.",
                            plan_continuation_nudges, max_boulder_nudges
                        )));
                        continue;
                    }
                }

                let _ = crate::agent::boulder::BoulderState::sync_from_workspace(
                    &self.workspace_root,
                    is_ralph_loop,
                    max_boulder_nudges,
                );
                self.messages.push(Message::assistant(clean_text));
                break;
            }
        }

        // Finalize transaction if one was initiated for this turn and is still active.
        // Always commit the transaction so created/modified files are preserved on disk
        // (while the pre-turn backup snapshot remains available for manual `/tx rollback` or `/undo`).
        if let Some(ref tx_id) = turn_tx_id {
            if let Ok(Some(active)) =
                crate::session::transaction::TransactionManager::get_active(&self.workspace_root)
            {
                if active.tx_id == *tx_id {
                    let _ = crate::session::transaction::TransactionManager::commit(
                        &self.workspace_root,
                        Some(tx_id),
                    );
                }
            }
        }

        let mut turn_tokens_used = last_prompt_tokens + cumulative_completion_tokens;
        if turn_tokens_used == 0 {
            turn_tokens_used = self.count_current_context_tokens();
        }
        self.cumulative_tokens_used = self.cumulative_tokens_used.saturating_add(turn_tokens_used);

        // Bi-Temporal Architecture Memory & Decision Record Logging (Graphiti & Semantica integration)
        if !turn_files_modified.is_empty() && !was_cancelled && !circuit_tripped {
            let mut mem_store =
                crate::context::temporal_memory::TemporalMemoryStore::load_or_default(
                    &self.workspace_root,
                );
            let mut invalidated_any = false;
            for f in &turn_files_modified {
                if mem_store.invalidate_facts_for_file(f) > 0 {
                    invalidated_any = true;
                }
            }
            if invalidated_any {
                let _ = mem_store.save(&self.workspace_root);
            }

            let decision_rec = crate::context::decision_record::DecisionRecord {
                id: format!("DEC-{}-{}", chrono::Utc::now().timestamp(), turn_id),
                timestamp: chrono::Utc::now().to_rfc3339(),
                turn_id,
                intent: user_prompt.to_string(),
                decision: format!(
                    "Modified {} file(s) to fulfill requirements: {}",
                    turn_files_modified.len(),
                    turn_files_modified.join(", ")
                ),
                rationale: "Autonomous execution of user task requirements".to_string(),
                alternatives_considered: Vec::new(),
                files_affected: turn_files_modified.clone(),
                verified: heal_attempts == 0,
            };
            let _ = crate::context::decision_record::DecisionLogger::append(
                &self.workspace_root,
                &decision_rec,
            );
        }

        // Autonomous Git Auto-Commit if files were modified during this turn
        if !turn_files_modified.is_empty()
            && self.config.git.auto_commit
            && !was_cancelled
            && !circuit_tripped
        {
            let git = crate::git::GitService::new(self.workspace_root.clone());
            if git.is_git_repo().await {
                let commit_svc = crate::git::GitCommitService::new(&git)
                    .with_stage_all(self.config.git.dirty_commit);
                let commit_msg = if self.config.git.ai_commit_messages {
                    let effective_goal =
                        if crate::git::GitCommitService::extract_meaningful_headline(user_prompt)
                            .is_some()
                        {
                            Some(user_prompt)
                        } else {
                            self.messages
                                .iter()
                                .filter(|m| m.role == crate::agent::types::Role::User)
                                .find_map(|m| {
                                    crate::git::GitCommitService::extract_meaningful_headline(
                                        &m.content,
                                    )
                                    .map(|_| m.content.as_str())
                                })
                                .or_else(|| {
                                    self.compactor
                                        .anchor()
                                        .working_context
                                        .as_deref()
                                        .filter(|s| !s.is_empty())
                                })
                        };
                    crate::git::GitCommitService::generate_conventional_message(
                        &turn_files_modified,
                        effective_goal,
                    )
                } else {
                    format!("chore: update {}", turn_files_modified.join(", "))
                };
                match commit_svc
                    .commit(&commit_msg, Some(&turn_files_modified))
                    .await
                {
                    Ok(hash) => {
                        if let Err(e) = self.backup_manager.record_turn_commit(turn_id, &hash) {
                            tracing::warn!(
                                turn_id,
                                error = %e,
                                "Failed to record git commit hash in turn backup manifest"
                            );
                        }
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
        if last_cached_prompt_tokens > 0 {
            tracing::info!(
                turn_id,
                turn_tokens_used,
                cached_prompt_tokens = last_cached_prompt_tokens,
                "Turn completed with prompt cache hits"
            );
        }

        let _ = crate::tools::browser::BrowserManager::shutdown_live_engine().await;

        let end_event = AgentEvent::TurnEnd {
            turn_id,
            status: if was_cancelled {
                TURN_STATUS_CANCELLED
            } else if circuit_tripped {
                TURN_STATUS_CIRCUIT_TRIPPED
            } else if hit_iteration_limit {
                TURN_STATUS_ITERATION_LIMIT
            } else {
                TURN_STATUS_COMPLETE
            }
            .to_string(),
            total_tokens_used: turn_tokens_used,
            files_modified: turn_files_modified.clone(),
            cached_prompt_tokens: last_cached_prompt_tokens,
        };
        if let Err(e) = self
            .session_store
            .append_event(&self.session_id, &end_event)
        {
            tracing::warn!("Failed to persist TurnEnd event: {}", e);
        }
        event_sender.send(end_event)?;
        crate::agent::subagent::clear_live_event_sender();

        if !was_cancelled && !circuit_tripped {
            // Sync turn learnings & facts into Progressive Multi-Tier Memory
            let mut prog_mem =
                crate::context::progressive_memory::ProgressiveMemory::load(&self.workspace_root);
            prog_mem.extract_and_store_facts(user_prompt, "user_prompt");
            if !turn_files_modified.is_empty() {
                prog_mem.record_l1_anchor(
                    &format!("Modified {} files", turn_files_modified.len()),
                    &turn_files_modified,
                    user_prompt,
                    &format!("turn_{}", turn_id),
                );
            }

            // Continual Harness Error-Recovery Learning:
            // If a tool call failed during this turn and a subsequent DIFFERENT tool call succeeded,
            // automatically distill and persist the cross-tool recovery trajectory into ProgressiveMemory.
            if let Some(fail_idx) = turn_tool_results.iter().position(|r| !r.success) {
                let failed_res = &turn_tool_results[fail_idx];
                if let Some(recover_res) = turn_tool_results[fail_idx + 1..]
                    .iter()
                    .find(|r| r.success && r.tool_name != failed_res.tool_name)
                {
                    let err_summary = failed_res
                        .output
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty())
                        .take(3)
                        .collect::<Vec<_>>()
                        .join(" ");
                    let err_text = if err_summary.is_empty() {
                        "tool error"
                    } else {
                        err_summary.as_str()
                    };
                    let clamped_err = crate::utils::strings::truncate_chars(err_text, 180);
                    let lesson = format!(
                        "Note: `{}` failed with ({}) and was resolved by `{}`.",
                        failed_res.tool_name, clamped_err, recover_res.tool_name
                    );
                    prog_mem
                        .extract_and_store_facts(&lesson, &format!("turn_{}_recovery", turn_id));
                }
            }

            let _ = prog_mem.save(&self.workspace_root);
        }

        if let Some(ref mut ledger) = self.intent_ledger {
            if !items_progressed_this_turn {
                ledger.consecutive_turns_without_progress += 1;
            }
            let persistence_path = self
                .workspace_root
                .join(&self.config.agent.intent.persistence_file);
            if let Err(e) = ledger.save_to_disk(&persistence_path) {
                tracing::warn!(
                    error = %e,
                    path = %persistence_path.display(),
                    "Failed to persist intent ledger at turn end"
                );
            }
        }

        // Persist deterministic Continuity Ledger (.minicode/ledgers/CONTINUITY.md) after turn
        self.compactor
            .write_continuity_ledger(&self.workspace_root, &self.session_id);

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
