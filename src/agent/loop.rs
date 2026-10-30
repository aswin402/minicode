use crate::agent::prompt::PromptBuilder;
use crate::agent::provider::{CompletionOptions, Provider, StreamChunk};
use crate::agent::stuck_detector::BreakerAction;
use crate::agent::types::{AgentEvent, ApprovalDecision, Message, ToolCall, Turn};
use crate::config::Config;
use crate::constants::{
    CONTEXT_MIN_PRESERVED_MESSAGES, DEFAULT_MAX_RETRIES, FILE_MODIFYING_TOOLS, MCP_TOOL_PREFIX,
    RETRY_BACKOFF_SECS, TURN_STATUS_CANCELLED, TURN_STATUS_CIRCUIT_TRIPPED, TURN_STATUS_COMPLETE,
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

/// Classifies whether a tool invocation is a critical remote-mutating or destructive operation
/// (`git push`, `git pull`, `git reset --hard`, `git clean -f`, `gh pr merge/create`, `cargo/npm publish`,
/// `rm -rf`, `sudo`, etc.) that MUST ALWAYS ask for explicit user permission in interactive mode,
/// even when `auto_approve` ("Allow session") is enabled.
pub fn classify_mandatory_approval(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    match tool_name {
        "git_push" => Some("Remote Git push requires explicit user permission".to_string()),
        "git_pull" => Some("Remote Git pull requires explicit user permission".to_string()),
        "create_pr" | "github_pr_create" => Some(
            "Creating a remote GitHub Pull Request requires explicit user permission".to_string(),
        ),
        "github_issue_create" => {
            Some("Creating a remote GitHub Issue requires explicit user permission".to_string())
        }
        "exec_cmd" | "sandbox_exec" => {
            let parsed_str_args: Option<serde_json::Value> =
                args.as_str().and_then(|s| serde_json::from_str(s).ok());
            let effective_args = parsed_str_args.as_ref().unwrap_or(args);
            let raw_cmd = effective_args
                .get("command")
                .or_else(|| effective_args.get("cmd"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if raw_cmd.trim().is_empty() {
                return None;
            }
            classify_mandatory_shell_command(raw_cmd)
        }
        _ => None,
    }
}

/// Inspects a shell command string for critical operations (`git push`, `git pull`, `git reset --hard`,
/// `git clean -f`, `gh pr merge`, `cargo/npm/bun publish`, `rm -rf`, `sudo`, etc.) that must always prompt.
pub fn classify_mandatory_shell_command(command_str: &str) -> Option<String> {
    for segment in command_str.split([';', '\n']) {
        for sub in segment.split("&&").flat_map(|s| s.split("||")) {
            let tokens: Vec<&str> = sub.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }

            // Skip leading env assignments (KEY=VAL) and wrapper commands (timeout, nice, env)
            let mut idx = 0;
            while idx < tokens.len() {
                let tok = tokens[idx];
                if tok.contains('=') && !tok.starts_with('-') {
                    idx += 1;
                    continue;
                }
                if matches!(tok, "env" | "command" | "nohup") {
                    idx += 1;
                    continue;
                }
                if tok == "timeout" {
                    idx += 1;
                    while idx < tokens.len() && tokens[idx].starts_with('-') {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1; // duration arg
                    }
                    continue;
                }
                if tok == "nice" {
                    idx += 1;
                    if idx < tokens.len() && tokens[idx] == "-n" {
                        idx += 2;
                    }
                    continue;
                }
                break;
            }

            let rem = &tokens[idx..];
            if rem.is_empty() {
                continue;
            }

            let bin = rem[0]
                .rsplit('/')
                .next()
                .unwrap_or(rem[0])
                .to_ascii_lowercase();

            if bin == "sudo" || bin == "doas" {
                return Some(format!(
                    "Privilege escalation (`{}`) requires explicit user permission",
                    bin
                ));
            }

            if bin == "git" {
                // Find the git subcommand (skipping global flags like `-C <path>`, `--no-pager`, `-c key=val`)
                let mut g_idx = 1;
                while g_idx < rem.len() {
                    let g_tok = rem[g_idx];
                    if matches!(g_tok, "-C" | "-c" | "--git-dir" | "--work-tree") {
                        g_idx += 2;
                        continue;
                    }
                    if g_tok.starts_with('-') {
                        g_idx += 1;
                        continue;
                    }
                    break;
                }
                if let Some(&subcmd_raw) = rem.get(g_idx) {
                    let subcmd = subcmd_raw.to_ascii_lowercase();
                    let sub_args = &rem[g_idx + 1..];
                    match subcmd.as_str() {
                        "push" => {
                            return Some(
                                "`git push` (pushing commits to remote repository) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "pull" => {
                            return Some(
                                "`git pull` (pulling remote changes into workspace) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "reset" if sub_args.iter().any(|a| *a == "--hard" || *a == "--merge") => {
                            return Some(
                                "`git reset --hard` (discarding uncommitted work) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "clean"
                            if sub_args.iter().any(|a| {
                                a.starts_with('-') && (a.contains('f') || *a == "--force")
                            }) =>
                        {
                            return Some(
                                "`git clean -f` (deleting untracked files) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "checkout"
                            if sub_args.contains(&".")
                                || sub_args.windows(2).any(|w| w == ["--", "."]) =>
                        {
                            return Some(
                                "`git checkout -- .` (discarding working tree edits) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "restore" if sub_args.contains(&".") => {
                            return Some(
                                "`git restore .` (discarding working tree edits) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "branch" if sub_args.contains(&"-D") => {
                            return Some(
                                "`git branch -D` (force-deleting a branch) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        _ => {}
                    }
                }
            }

            if bin == "gh" && rem.len() >= 3 {
                let domain = rem[1].to_ascii_lowercase();
                let action = rem[2].to_ascii_lowercase();
                if matches!(
                    (domain.as_str(), action.as_str()),
                    ("pr", "create" | "merge" | "close")
                        | ("release", "create" | "delete")
                        | ("repo", "delete" | "archive" | "create")
                        | ("issue", "create" | "delete" | "close")
                ) {
                    return Some(format!(
                        "`gh {} {}` (remote GitHub operation) requires explicit user permission",
                        domain, action
                    ));
                }
            }

            if matches!(bin.as_str(), "cargo" | "npm" | "bun" | "pnpm" | "yarn")
                && rem
                    .get(1)
                    .is_some_and(|a| a.eq_ignore_ascii_case("publish"))
            {
                return Some(format!(
                    "`{} publish` (publishing package to remote registry) requires explicit user permission",
                    bin
                ));
            }

            if bin == "docker" && rem.get(1).is_some_and(|a| a.eq_ignore_ascii_case("push")) {
                return Some("`docker push` requires explicit user permission".to_string());
            }

            if bin == "rm" {
                let has_rf = rem[1..].iter().any(|a| {
                    *a == "-rf" || *a == "-fr" || *a == "-r" || *a == "-R" || *a == "--recursive"
                });
                if has_rf {
                    let targets: Vec<&str> = rem[1..]
                        .iter()
                        .copied()
                        .filter(|a| !a.starts_with('-'))
                        .collect();
                    let all_safe_caches = !targets.is_empty()
                        && targets.iter().all(|t| {
                            let clean = t.trim_start_matches("./").trim_end_matches('/');
                            let root_seg = clean.split('/').next().unwrap_or(clean);
                            matches!(
                                root_seg,
                                "dist"
                                    | "node_modules"
                                    | "target"
                                    | ".next"
                                    | "build"
                                    | "coverage"
                                    | ".turbo"
                                    | ".vite"
                            )
                        });
                    if !all_safe_caches {
                        return Some(
                            "Recursive deletion (`rm -rf`) requires explicit user permission"
                                .to_string(),
                        );
                    }
                }
            }
        }
    }
    None
}

pub struct AgentLoop {
    workspace_root: PathBuf,
    config: Config,
    provider: Box<dyn Provider>,
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

        let compactor = crate::context::auto_compact::AutoCompactor::new(&config.provider.model)
            .unwrap_or_else(|_| crate::context::auto_compact::AutoCompactor::default_safe());

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

    /// Returns a reference to the reactive agent-to-agent mailbox, if available.
    #[allow(dead_code)]
    pub fn mailbox(&self) -> Option<&crate::agent::subagent::mailbox::AgentMailbox> {
        self.mailbox.as_ref()
    }

    /// Returns a handle to the in-flight approval registry for hosts (TUI/NDJSON).
    pub fn approval_registry(&self) -> super::types::ApprovalRegistry {
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
    fn requires_approval(&self, tool_name: &str, args: &serde_json::Value) -> Option<String> {
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
    fn cleanup_empty_session_file(&self) {
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
    fn rebuild_active_tools(
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
        if self.config.agent.compact_tool_schemas {
            tools = crate::tools::schema_compactor::ToolSchemaCompactor::compact_schemas(&tools);
        }
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools
    }

    /// Estimates or counts the token usage of `self.messages`.
    fn count_current_context_tokens(&self) -> usize {
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
    fn extract_user_image_attachments(
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

    /// Executes a single interactive or autonomous turn with the ReAct tool-use loop.
    /// Emits structured `AgentEvent`s over the provided MPSC channel for UI or NDJSON rendering.
    pub async fn execute_turn(
        &mut self,
        user_prompt: &str,
        event_sender: mpsc::UnboundedSender<AgentEvent>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<Turn> {
        self.ensure_session_persisted()?;

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

        let is_ultrawork = Self::detect_ultrawork_intent(user_prompt);
        if is_ultrawork {
            prompt_with_context = format!(
                "[AUTONOMOUS ULTRAWORK MODE ACTIVE]\n\
                 1. Execute with maximum thoroughness and zero premature stopping.\n\
                 2. Structure multi-step work in `create_plan` (or `create_task_dag`), delegate parallelizable subtasks to specialized `MiniPub` workers (`dispatch_subagent` / `delegate_task` with `category` and `load_skills`), and verify all changes via `power_verify`.\n\
                 3. Do NOT stop until 100% of planned steps and verification gates are complete.\n\n{}",
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
        let mut iteration = 0;
        let mut heal_attempts = 0;
        let mut last_verification_failure: Option<(String, String)> = None;
        let mut plan_continuation_nudges: usize = 0;
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
                                self.requires_approval(&c.name, &c.arguments).is_some()
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
                            for (
                                (tool_call, mut tool_result),
                                (file_existed_before, file_before),
                            ) in parallel_calls
                                .into_iter()
                                .zip(parallel_results)
                                .zip(pre_snapshots)
                            {
                                // === Tool Middleware Pipeline: timing → redact → checkpoint → diff → spillover ===
                                tool_result = self.tool_pipeline.run(
                                    tool_result,
                                    &tool_call.name,
                                    &self.workspace_root,
                                    &tool_call.arguments,
                                    file_before.as_deref(),
                                );

                                // === Smart Donut Truncator (Phase 88, 115 + Dynamic Context-Remaining Scaling) ===
                                let ctx_limit = self.compactor.model_token_limit();
                                let used_tokens_est =
                                    last_prompt_tokens.saturating_add(cumulative_completion_tokens);
                                tool_result.output =
                                    crate::context::donut::SmartDonutTruncator::truncate_for_remaining_context(
                                        &tool_result.output,
                                        ctx_limit,
                                        used_tokens_est,
                                    )
                                    .content;
                                if tool_result.success {
                                    self.compactor.record_tool_file_ops(
                                        &tool_call.name,
                                        &tool_call.arguments,
                                    );
                                }

                                // === Algorithmic Stuck Detector & Anti-Thrashing Circuit Breaker (Phase 112) ===
                                match self.stuck_detector.check(
                                    &tool_call.name,
                                    &tool_call.arguments,
                                    tool_result.success,
                                ) {
                                    BreakerAction::Pass => {}
                                    BreakerAction::Warning(intervention) => {
                                        tracing::warn!(
                                            tool = %tool_call.name,
                                            "Anti-thrashing stuck detector warning injected"
                                        );
                                        tool_result.output.push_str("\n\n");
                                        tool_result.output.push_str(&intervention);
                                    }
                                    BreakerAction::Trip { reason, loop_type } => {
                                        tracing::warn!(
                                            tool = %tool_call.name,
                                            pattern = %loop_type.pattern_name(),
                                            "Anti-thrashing circuit breaker intervention injected"
                                        );
                                        tool_result.output.push_str("\n\n");
                                        tool_result.output.push_str(&reason);

                                        let trip_event = AgentEvent::AntiThrashTripped {
                                            turn_id,
                                            pattern: loop_type.pattern_name().to_string(),
                                            target: loop_type.target_file(),
                                            failures: loop_type.failures(),
                                            intervention: reason,
                                        };
                                        if let Err(e) = self
                                            .session_store
                                            .append_event(&self.session_id, &trip_event)
                                        {
                                            tracing::warn!(
                                                "Failed to persist AntiThrashTripped event: {}",
                                                e
                                            );
                                        }
                                        let _ = event_sender.send(trip_event);
                                        circuit_tripped = true;
                                    }
                                }

                                // Auto-sync active tool schemas if LLM invoked a lazy tool directly or hit a near-miss tool name
                                if tool_result.success
                                    && tool_call.name != "activate_tools"
                                    && !tools.iter().any(|t| t.name == tool_call.name)
                                    && active_tool_names.insert(tool_call.name.clone())
                                {
                                    tools = self.rebuild_active_tools(
                                        user_prompt,
                                        &active_categories,
                                        &mcp_tools_by_server,
                                        &active_mcp_servers,
                                        &active_tool_names,
                                    );
                                } else if !tool_result.success
                                    && tool_result.output.contains("Tool '")
                                    && tool_result.output.contains("not found")
                                {
                                    let mut added = false;
                                    for matched in crate::tools::category::search_individual_tools(
                                        &tool_call.name,
                                        &mcp_tools_by_server,
                                    ) {
                                        if active_tool_names.insert(matched.name) {
                                            added = true;
                                        }
                                    }
                                    if added {
                                        tools = self.rebuild_active_tools(
                                            user_prompt,
                                            &active_categories,
                                            &mcp_tools_by_server,
                                            &active_mcp_servers,
                                            &active_tool_names,
                                        );
                                    }
                                }

                                let res_event = AgentEvent::ToolResult {
                                    turn_id,
                                    tool_id: tool_result.tool_id.clone(),
                                    tool: tool_result.tool_name.clone(),
                                    success: tool_result.success,
                                    output: tool_result.display_output().to_string(),
                                    duration_ms: tool_result.duration_ms,
                                };
                                if let Err(e) = self
                                    .session_store
                                    .append_event(&self.session_id, &res_event)
                                {
                                    tracing::warn!("Failed to persist ToolResult event: {}", e);
                                }
                                event_sender.send(res_event)?;

                                let is_noop_patch =
                                    tool_result.output.starts_with("No changes needed in ");
                                if tool_result.success
                                    && FILE_MODIFYING_TOOLS.contains(&tool_call.name.as_str())
                                    && !is_noop_patch
                                {
                                    batch_had_mutation = true;
                                    let _ = std::fs::remove_file(
                                        self.workspace_root.join(".minicode/plan/last_verify.json"),
                                    );
                                    if let Some(path_str) =
                                        crate::tools::param::opt_path(&tool_call.arguments)
                                    {
                                        let p_owned = path_str.to_string();
                                        if !turn_files_modified.contains(&p_owned) {
                                            turn_files_modified.push(p_owned.clone());
                                        }
                                        let (created, modified) = if file_existed_before {
                                            (Vec::new(), vec![p_owned.clone()])
                                        } else {
                                            (vec![p_owned.clone()], Vec::new())
                                        };
                                        let wm = crate::context::memory::working_memory::WorkingMemory::new(
                                            &self.workspace_root,
                                        );
                                        let reconciled = wm.reconcile_workspace_tasks(&[p_owned]);
                                        if reconciled > 0 {
                                            batch_reconciled += reconciled;
                                            self.emit_current_plan(Some(turn_id), &event_sender);
                                        }
                                        if let Some(ref mut ledger) = self.intent_ledger {
                                            let mut updated = ledger.update_from_file_activity(
                                                &created, &modified, turn_id,
                                            );
                                            if reconciled > 0 {
                                                updated +=
                                                    ledger.sync_with_working_memory(&wm, turn_id);
                                            }
                                            if updated > 0 {
                                                items_progressed_this_turn = true;
                                            }
                                            let persistence_path = self
                                                .workspace_root
                                                .join(&self.config.agent.intent.persistence_file);
                                            let _ = ledger.save_to_disk(&persistence_path);
                                        }
                                    }
                                }

                                let subagent_modified =
                                    crate::agent::subagent::relay::get_global_relay_store()
                                        .drain_modified_files(&self.workspace_root);
                                if !subagent_modified.is_empty() {
                                    batch_had_mutation = true;
                                    for p in &subagent_modified {
                                        if !turn_files_modified.contains(p) {
                                            turn_files_modified.push(p.clone());
                                        }
                                    }
                                    let wm =
                                        crate::context::memory::working_memory::WorkingMemory::new(
                                            &self.workspace_root,
                                        );
                                    let reconciled =
                                        wm.reconcile_workspace_tasks(&subagent_modified);
                                    if reconciled > 0 {
                                        batch_reconciled += reconciled;
                                        self.emit_current_plan(Some(turn_id), &event_sender);
                                    }
                                    if let Some(ref mut ledger) = self.intent_ledger {
                                        let mut updated = ledger.update_from_file_activity(
                                            &[],
                                            &subagent_modified,
                                            turn_id,
                                        );
                                        if reconciled > 0 {
                                            updated +=
                                                ledger.sync_with_working_memory(&wm, turn_id);
                                        }
                                        if updated > 0 {
                                            items_progressed_this_turn = true;
                                        }
                                        let persistence_path = self
                                            .workspace_root
                                            .join(&self.config.agent.intent.persistence_file);
                                        let _ = ledger.save_to_disk(&persistence_path);
                                    }
                                }

                                // Append tool result message for LLM context with smart observation pruning (JSON + Logs)
                                let mut output_for_llm =
                                    crate::context::budget::ObservationPruner::prune_for_llm(
                                        &tool_call.name,
                                        &tool_result.output,
                                    );
                                if tool_result.success {
                                    if crate::context::dedup::FetchCallTracker::should_invalidate_local_reads(
                                        &tool_call.name,
                                    ) {
                                        self.fetch_tracker.invalidate_local_reads();
                                    } else {
                                        output_for_llm = self.fetch_tracker.record_and_annotate(
                                            &tool_call.name,
                                            &tool_call.arguments,
                                            output_for_llm,
                                        );
                                    }
                                    self.fetch_tracker.record_preloaded_screenshot(
                                        &tool_call.name,
                                        &tool_call.arguments,
                                    );
                                }
                                let is_fetch_blocked = output_for_llm.starts_with("<fetch-blocked");
                                let mut tool_msg = Message::tool_result(
                                    tool_call.id,
                                    tool_call.name.clone(),
                                    output_for_llm,
                                );
                                if tool_result.success && !is_fetch_blocked {
                                    let tool_images = crate::agent::subagent::worker::extract_tool_image_attachments(
                                        &self.workspace_root,
                                        &tool_call.name,
                                        &tool_call.arguments,
                                    );
                                    if !tool_images.is_empty() {
                                        tool_msg = tool_msg.with_images(tool_images);
                                        let mut seen_image_tools = 0usize;
                                        for prev in self.messages.iter_mut().rev() {
                                            if prev.role == crate::agent::types::Role::Tool
                                                && prev
                                                    .images
                                                    .as_ref()
                                                    .is_some_and(|imgs| !imgs.is_empty())
                                            {
                                                seen_image_tools += 1;
                                                if seen_image_tools >= 6 {
                                                    prev.images = None;
                                                }
                                            }
                                        }
                                    }
                                }
                                self.messages.push(tool_msg);

                                turn_tool_results.push(tool_result);
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

                            // === Tool Middleware Pipeline: timing → redact → checkpoint → diff ===
                            let mut tool_result = self.tool_pipeline.run(
                                tool_result,
                                &tool_call.name,
                                &self.workspace_root,
                                &tool_call.arguments,
                                file_before.as_deref(),
                            );
                            if tool_result.success {
                                // Refresh static system prompt when stack scaffolding or skill installation
                                // updates workspace skills (.minicode/skills/), minikit.json, or AGENTS.md.
                                if matches!(
                                    tool_call.name.as_str(),
                                    "kit_stack_add"
                                        | "kit_skill_install"
                                        | "kit_skill_remove"
                                        | "vault_load"
                                        | "vault_unload"
                                        | "vault_bundle_load"
                                ) {
                                    let refreshed = crate::agent::prompt::PromptBuilder::build_static_system_prompt(
                                        &self.workspace_root,
                                        None,
                                    );
                                    if let Some(first_msg) = self.messages.first_mut() {
                                        if first_msg.role == crate::agent::types::Role::System {
                                            first_msg.content = refreshed.clone();
                                        }
                                    }
                                    options.system_instruction = Some(refreshed);
                                }
                            }

                            // === Smart Donut Truncator (Phase 88, 115 + Dynamic Context-Remaining Scaling) ===
                            let ctx_limit = self.compactor.model_token_limit();
                            let used_tokens_est =
                                last_prompt_tokens.saturating_add(cumulative_completion_tokens);
                            tool_result.output =
                                crate::context::donut::SmartDonutTruncator::truncate_for_remaining_context(
                                    &tool_result.output,
                                    ctx_limit,
                                    used_tokens_est,
                                )
                                .content;
                            if tool_result.success {
                                self.compactor
                                    .record_tool_file_ops(&tool_call.name, &tool_call.arguments);
                            }

                            // === Algorithmic Stuck Detector & Anti-Thrashing Circuit Breaker (Phase 112) ===
                            match self.stuck_detector.check(
                                &tool_call.name,
                                &tool_call.arguments,
                                tool_result.success,
                            ) {
                                BreakerAction::Pass => {}
                                BreakerAction::Warning(intervention) => {
                                    tracing::warn!(
                                        tool = %tool_call.name,
                                        "Anti-thrashing stuck detector warning injected"
                                    );
                                    tool_result.output.push_str("\n\n");
                                    tool_result.output.push_str(&intervention);
                                }
                                BreakerAction::Trip { reason, loop_type } => {
                                    tracing::warn!(
                                        tool = %tool_call.name,
                                        pattern = %loop_type.pattern_name(),
                                        "Anti-thrashing circuit breaker intervention injected"
                                    );
                                    tool_result.output.push_str("\n\n");
                                    tool_result.output.push_str(&reason);

                                    let trip_event = AgentEvent::AntiThrashTripped {
                                        turn_id,
                                        pattern: loop_type.pattern_name().to_string(),
                                        target: loop_type.target_file(),
                                        failures: loop_type.failures(),
                                        intervention: reason,
                                    };
                                    if let Err(e) = self
                                        .session_store
                                        .append_event(&self.session_id, &trip_event)
                                    {
                                        tracing::warn!(
                                            "Failed to persist AntiThrashTripped event: {}",
                                            e
                                        );
                                    }
                                    let _ = event_sender.send(trip_event);
                                    circuit_tripped = true;
                                }
                            }

                            // If LLM invoked activate_tools and succeeded, dynamically reload active schemas for subsequent steps
                            if tool_call.name == "activate_tools" && tool_result.success {
                                let cat_arg = crate::tools::param::get_str_with_aliases(
                                    &tool_call.arguments,
                                    &["category", "query", "search", "tools", "tool"],
                                );
                                let extra_query =
                                    crate::tools::param::opt_str(&tool_call.arguments, "query");
                                if let Some(cat_str) = cat_arg {
                                    if cat_str == "all" {
                                        active_categories
                                            .extend(crate::tools::category::ToolCategory::ALL);
                                        active_mcp_servers
                                            .extend(mcp_tools_by_server.keys().cloned());
                                    } else if cat_str == "mcp" {
                                        active_mcp_servers
                                            .extend(mcp_tools_by_server.keys().cloned());
                                    } else if extra_query.is_none()
                                        && cat_str
                                            .parse::<crate::tools::category::ToolCategory>()
                                            .is_ok()
                                    {
                                        if let Ok(cat) =
                                            cat_str.parse::<crate::tools::category::ToolCategory>()
                                        {
                                            active_categories.insert(cat);
                                        }
                                    } else if extra_query.is_none()
                                        && (cat_str.starts_with("mcp:")
                                            || mcp_tools_by_server.contains_key(cat_str))
                                    {
                                        let server =
                                            cat_str.strip_prefix("mcp:").unwrap_or(cat_str);
                                        active_mcp_servers.insert(server.to_string());
                                    } else {
                                        for part in cat_str
                                            .split(',')
                                            .map(|s| s.trim())
                                            .filter(|s| !s.is_empty())
                                        {
                                            if let Ok(cat) =
                                                part.parse::<crate::tools::category::ToolCategory>()
                                            {
                                                active_categories.insert(cat);
                                            }
                                        }
                                        let search_target = extra_query
                                            .filter(|q| !q.is_empty())
                                            .unwrap_or(cat_str);
                                        for matched in
                                            crate::tools::category::search_individual_tools(
                                                search_target,
                                                &mcp_tools_by_server,
                                            )
                                        {
                                            active_tool_names.insert(matched.name);
                                        }
                                    }
                                    tools = self.rebuild_active_tools(
                                        user_prompt,
                                        &active_categories,
                                        &mcp_tools_by_server,
                                        &active_mcp_servers,
                                        &active_tool_names,
                                    );
                                    tracing::info!(
                                        "Dynamically activated '{}'; active schemas count now {}",
                                        cat_str,
                                        tools.len()
                                    );
                                }
                            } else if tool_result.success
                                && !tools.iter().any(|t| t.name == tool_call.name)
                                && active_tool_names.insert(tool_call.name.clone())
                            {
                                tools = self.rebuild_active_tools(
                                    user_prompt,
                                    &active_categories,
                                    &mcp_tools_by_server,
                                    &active_mcp_servers,
                                    &active_tool_names,
                                );
                            } else if !tool_result.success
                                && tool_result.output.contains("Tool '")
                                && tool_result.output.contains("not found")
                            {
                                let mut added = false;
                                for matched in crate::tools::category::search_individual_tools(
                                    &tool_call.name,
                                    &mcp_tools_by_server,
                                ) {
                                    if active_tool_names.insert(matched.name) {
                                        added = true;
                                    }
                                }
                                if added {
                                    tools = self.rebuild_active_tools(
                                        user_prompt,
                                        &active_categories,
                                        &mcp_tools_by_server,
                                        &active_mcp_servers,
                                        &active_tool_names,
                                    );
                                }
                            }

                            let res_event = AgentEvent::ToolResult {
                                turn_id,
                                tool_id: tool_result.tool_id.clone(),
                                tool: tool_result.tool_name.clone(),
                                success: tool_result.success,
                                output: tool_result.display_output().to_string(),
                                duration_ms: tool_result.duration_ms,
                            };
                            if let Err(e) = self
                                .session_store
                                .append_event(&self.session_id, &res_event)
                            {
                                tracing::warn!("Failed to persist ToolResult event: {}", e);
                            }
                            event_sender.send(res_event)?;

                            let mut total_reconciled = 0;
                            let mut plan_emitted = false;
                            let is_noop_patch =
                                tool_result.output.starts_with("No changes needed in ");
                            if tool_result.success
                                && FILE_MODIFYING_TOOLS.contains(&tool_call.name.as_str())
                                && !is_noop_patch
                            {
                                let _ = std::fs::remove_file(
                                    self.workspace_root.join(".minicode/plan/last_verify.json"),
                                );
                                if let Some(path_str) =
                                    crate::tools::param::opt_path(&tool_call.arguments)
                                {
                                    let p_owned = path_str.to_string();
                                    if !turn_files_modified.contains(&p_owned) {
                                        turn_files_modified.push(p_owned.clone());
                                    }
                                    let (created, modified) = if file_existed_before {
                                        (Vec::new(), vec![p_owned.clone()])
                                    } else {
                                        (vec![p_owned.clone()], Vec::new())
                                    };
                                    let wm =
                                        crate::context::memory::working_memory::WorkingMemory::new(
                                            &self.workspace_root,
                                        );
                                    let reconciled = wm.reconcile_workspace_tasks(&[p_owned]);
                                    if reconciled > 0 {
                                        total_reconciled += reconciled;
                                        self.emit_current_plan(Some(turn_id), &event_sender);
                                        plan_emitted = true;
                                    }
                                    if let Some(ref mut ledger) = self.intent_ledger {
                                        let mut updated = ledger.update_from_file_activity(
                                            &created, &modified, turn_id,
                                        );
                                        if reconciled > 0 {
                                            updated +=
                                                ledger.sync_with_working_memory(&wm, turn_id);
                                        }
                                        if updated > 0 {
                                            items_progressed_this_turn = true;
                                        }
                                        let persistence_path = self
                                            .workspace_root
                                            .join(&self.config.agent.intent.persistence_file);
                                        if let Err(e) = ledger.save_to_disk(&persistence_path) {
                                            tracing::warn!(
                                                error = %e,
                                                path = %persistence_path.display(),
                                                "Failed to persist intent ledger after tool execution"
                                            );
                                        }
                                    }
                                } else if tool_call.name == "write_file" {
                                    self.emit_current_plan(Some(turn_id), &event_sender);
                                    plan_emitted = true;
                                }
                            } else if tool_result.success
                                && (tool_call.name == "update_progress"
                                    || tool_call.name == "archive_plan")
                            {
                                let wm = crate::context::memory::working_memory::WorkingMemory::new(
                                    &self.workspace_root,
                                );
                                if let Some(ref mut ledger) = self.intent_ledger {
                                    let synced = ledger.sync_with_working_memory(&wm, turn_id);
                                    if synced > 0 {
                                        items_progressed_this_turn = true;
                                        let persistence_path = self
                                            .workspace_root
                                            .join(&self.config.agent.intent.persistence_file);
                                        let _ = ledger.save_to_disk(&persistence_path);
                                    }
                                }
                            }

                            if tool_result.success
                                && (tool_call.name == "create_plan"
                                    || tool_call.name == "power_plan")
                            {
                                self.emit_current_plan(Some(turn_id), &event_sender);
                                plan_emitted = true;
                            } else if tool_result.success
                                && (tool_call.name == "kit_stack_add"
                                    || tool_call.name == "block_scaffold"
                                    || tool_call.name == "block_insert")
                            {
                                let _ = std::fs::remove_file(
                                    self.workspace_root.join(".minicode/plan/last_verify.json"),
                                );
                                let wm = crate::context::memory::working_memory::WorkingMemory::new(
                                    &self.workspace_root,
                                );
                                let auto_reconciled_extra = wm.reconcile_workspace_tasks(&[]);
                                if auto_reconciled_extra > 0 {
                                    total_reconciled += auto_reconciled_extra;
                                    self.emit_current_plan(Some(turn_id), &event_sender);
                                    plan_emitted = true;
                                }
                            }

                            let subagent_modified =
                                crate::agent::subagent::relay::get_global_relay_store()
                                    .drain_modified_files(&self.workspace_root);
                            if !subagent_modified.is_empty() {
                                let _ = std::fs::remove_file(
                                    self.workspace_root.join(".minicode/plan/last_verify.json"),
                                );
                                for p in &subagent_modified {
                                    if !turn_files_modified.contains(p) {
                                        turn_files_modified.push(p.clone());
                                    }
                                }
                                let wm = crate::context::memory::working_memory::WorkingMemory::new(
                                    &self.workspace_root,
                                );
                                let reconciled = wm.reconcile_workspace_tasks(&subagent_modified);
                                if reconciled > 0 {
                                    total_reconciled += reconciled;
                                    self.emit_current_plan(Some(turn_id), &event_sender);
                                    plan_emitted = true;
                                }
                                if let Some(ref mut ledger) = self.intent_ledger {
                                    let mut updated = ledger.update_from_file_activity(
                                        &[],
                                        &subagent_modified,
                                        turn_id,
                                    );
                                    if reconciled > 0 {
                                        updated += ledger.sync_with_working_memory(&wm, turn_id);
                                    }
                                    if updated > 0 {
                                        items_progressed_this_turn = true;
                                    }
                                    let persistence_path = self
                                        .workspace_root
                                        .join(&self.config.agent.intent.persistence_file);
                                    let _ = ledger.save_to_disk(&persistence_path);
                                }
                            }

                            if tool_result.success
                                && !crate::tools::concurrency::classify_tool(&tool_call.name)
                                    .is_read_only()
                            {
                                let mut category_added = false;
                                for cat in crate::agent::orchestrator::WorkflowRouter::state_driven_tool_categories(&self.workspace_root) {
                                    if active_categories.insert(cat) {
                                        category_added = true;
                                    }
                                }
                                for allowed in
                                    crate::vault::store::VaultStore::new(&self.workspace_root)
                                        .collect_active_skill_allowed_tools(
                                            Some(user_prompt),
                                            None,
                                            &turn_files_modified,
                                        )
                                {
                                    if let Ok(cat) =
                                        allowed.parse::<crate::tools::category::ToolCategory>()
                                    {
                                        if active_categories.insert(cat) {
                                            category_added = true;
                                        }
                                    }
                                    if active_tool_names.insert(allowed) {
                                        category_added = true;
                                    }
                                }
                                if category_added || tool_call.name == "kit_stack_add" {
                                    tools = self.rebuild_active_tools(
                                        user_prompt,
                                        &active_categories,
                                        &mcp_tools_by_server,
                                        &active_mcp_servers,
                                        &active_tool_names,
                                    );
                                    tracing::info!(
                                        tool = %tool_call.name,
                                        "Dynamic system instruction and state-driven tool categories refreshed following workspace state transition"
                                    );
                                }
                            }

                            // Append tool result message for LLM context with smart observation pruning (JSON + Logs)
                            let mut output_for_llm =
                                crate::context::budget::ObservationPruner::prune_for_llm(
                                    &tool_call.name,
                                    &tool_result.output,
                                );
                            if tool_result.success {
                                if crate::context::dedup::FetchCallTracker::should_invalidate_local_reads(
                                    &tool_call.name,
                                ) {
                                    self.fetch_tracker.invalidate_local_reads();
                                } else {
                                    output_for_llm = self.fetch_tracker.record_and_annotate(
                                        &tool_call.name,
                                        &tool_call.arguments,
                                        output_for_llm,
                                    );
                                }
                                self.fetch_tracker.record_preloaded_screenshot(
                                    &tool_call.name,
                                    &tool_call.arguments,
                                );
                            } else if tool_result.output.trim().len() >= 16 {
                                let store =
                                    crate::vault::store::VaultStore::new(&self.workspace_root);
                                let matched_gotchas =
                                    store.find_relevant_gotchas(&tool_result.output);
                                if !matched_gotchas.is_empty() {
                                    output_for_llm.push_str("\n\n<universal_learned_gotchas>\n");
                                    output_for_llm.push_str(
                                        "  Matching Pitfalls & Verified Fixes from Universal Vault:\n",
                                    );
                                    for g in matched_gotchas.iter().take(2) {
                                        output_for_llm.push_str(&format!(
                                            "  {}\n",
                                            g.format_prompt_block().replace('\n', "\n  ")
                                        ));
                                    }
                                    output_for_llm.push_str("</universal_learned_gotchas>");
                                }
                            }

                            // If kit_stack_add just scaffolded a UI workspace, immediately surface the framework-matched MiniBlocks catalog
                            if tool_result.success && tool_call.name == "kit_stack_add" {
                                if let Some(ui_enrichment) =
                                    crate::agent::orchestrator::WorkflowRouter::enrich_ui_design(
                                        &self.workspace_root,
                                        "",
                                    )
                                {
                                    output_for_llm.push_str("\n\n");
                                    output_for_llm.push_str(&ui_enrichment);
                                }
                            }

                            // If create_plan or power_plan just populated the active plan, immediately surface newly matched MiniVault skills
                            if tool_result.success
                                && (tool_call.name == "create_plan"
                                    || tool_call.name == "power_plan")
                            {
                                if let Some(post_plan_skills) =
                                    crate::agent::orchestrator::WorkflowRouter::enrich_post_plan_execution_context(
                                        &self.workspace_root,
                                        user_prompt,
                                    )
                                {
                                    output_for_llm.push_str("\n\n");
                                    output_for_llm.push_str(&post_plan_skills);
                                }
                            }

                            // Dynamic Step Grounding & Progress Invariant Injection:
                            // Remind the agent of its active step so it executes atomically and never batches at the end
                            if tool_call.name != "update_progress" {
                                let wm = crate::context::memory::working_memory::WorkingMemory::new(
                                    &self.workspace_root,
                                );
                                let has_ui_framework =
                                    crate::blocks::seed::ProjectStackInfo::detect(
                                        &self.workspace_root,
                                    )
                                    .framework
                                    .is_some();
                                let miniblocks_hint = if has_ui_framework {
                                    " Use `block_search`/`block_scaffold` for UI."
                                } else {
                                    ""
                                };
                                if total_reconciled > 0 {
                                    let rem_hint = wm.format_remaining_tasks_hint();
                                    if let Some(summary) = wm.read_active_plan_summary() {
                                        if let Some(ref next_active) = summary.active_task {
                                            output_for_llm.push_str(&format!(
                                                "\n\n{}\n[Plan Auto-Advancement: Steps 1..={} are ALREADY marked [x] ({}/{} completed — DO NOT call `update_progress` on them). Active step [>] is now #{}: \"{}\". Focus ONLY on this step.{}]",
                                                rem_hint,
                                                summary.completed_tasks,
                                                summary.completed_tasks,
                                                summary.total_tasks,
                                                summary.completed_tasks.saturating_add(1),
                                                next_active,
                                                miniblocks_hint
                                            ));
                                        } else if summary.completed_tasks == summary.total_tasks
                                            && summary.total_tasks > 0
                                        {
                                            output_for_llm.push_str(&format!(
                                                "\n\n{}\n[Plan Auto-Advancement: All {} tasks are ALREADY marked [x] (100% — DO NOT call `update_progress`). Next: run `power_review` + `power_verify`, then `archive_plan`.]",
                                                rem_hint,
                                                summary.total_tasks
                                            ));
                                        }
                                    }
                                } else if let Some(summary) = wm.read_active_plan_summary() {
                                    if let Some(ref active) = summary.active_task {
                                        output_for_llm.push_str(&format!(
                                            "\n\n[Active Step ({}/{}): [>] \"{}\". Continue implementing the remaining deliverables for this step (plan status auto-advances when all step deliverables exist on disk).{}]",
                                            summary.completed_tasks.saturating_add(1),
                                            summary.total_tasks,
                                            active,
                                            miniblocks_hint
                                        ));
                                    }
                                }
                            }

                            let is_fetch_blocked = output_for_llm.starts_with("<fetch-blocked");
                            let mut tool_msg = Message::tool_result(
                                tool_call.id,
                                tool_call.name.clone(),
                                output_for_llm,
                            );
                            if tool_result.success && !is_fetch_blocked {
                                let tool_images =
                                    crate::agent::subagent::worker::extract_tool_image_attachments(
                                        &self.workspace_root,
                                        &tool_call.name,
                                        &tool_call.arguments,
                                    );
                                if !tool_images.is_empty() {
                                    tool_msg = tool_msg.with_images(tool_images);
                                    // Keep at most the 6 most recent image-bearing tool messages to bound memory
                                    let mut seen_image_tools = 0usize;
                                    for prev in self.messages.iter_mut().rev() {
                                        if prev.role == crate::agent::types::Role::Tool
                                            && prev
                                                .images
                                                .as_ref()
                                                .is_some_and(|imgs| !imgs.is_empty())
                                        {
                                            seen_image_tools += 1;
                                            if seen_image_tools >= 6 {
                                                prev.images = None;
                                            }
                                        }
                                    }
                                }
                            }
                            self.messages.push(tool_msg);

                            if tool_result.success
                                && crate::context::budget::micro_compact::is_mutation_tool(
                                    &tool_call.name,
                                )
                            {
                                let micro_metrics =
                                    crate::context::budget::MicroCompactor::compact_messages(
                                        &mut self.messages,
                                        1,
                                    );
                                if micro_metrics.tokens_saved_estimate > 0 {
                                    tracing::info!(
                                        superseded_reads = micro_metrics.superseded_reads_compacted,
                                        duplicate_reads = micro_metrics.duplicate_reads_compacted,
                                        mutation_echoes = micro_metrics.mutation_echoes_compacted,
                                        search_results = micro_metrics.search_results_compacted,
                                        tokens_saved = micro_metrics.tokens_saved_estimate,
                                        "Applied semantic micro-compaction after file mutation"
                                    );
                                }
                            }

                            if !plan_emitted
                                && tool_result.success
                                && (tool_call.name.contains("plan")
                                    || tool_call.name == "update_progress"
                                    || tool_call.arguments.to_string().contains("todo.md"))
                            {
                                self.emit_current_plan(Some(turn_id), &event_sender);
                            }

                            turn_tool_results.push(tool_result);
                            if circuit_tripped {
                                break;
                            }
                        }
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

                // === 4-Gate Pre-Completion Verification Barrier (Phase 89 & Superpowers Protocol) ===
                let files_to_verify: Vec<String> = if !turn_files_modified.is_empty() {
                    turn_files_modified.clone()
                } else {
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

                let clean_text = crate::agent::prompt::strip_thought_blocks(&iteration_text);

                // === Boulder State Todo & TaskDAG Continuation Enforcer (Phase 175) ===
                // If an active plan (.minicode/plan/task_plan.md / todo.md) or TaskDAG (.minicode/task_dag.json)
                // still has unfinished tasks and the model stopped mid-execution (without asking the user),
                // inject a structured Boulder continuation reminder so it finishes all planned steps.
                let any_cancelled_tool = turn_tool_results.iter().any(|r| {
                    r.output.contains("cancelled") || r.output.contains("rejected by user")
                });
                let asked_user_recently = turn_tool_calls
                    .last()
                    .map(|t| t.name == "ask_user")
                    .unwrap_or(false);

                if !was_cancelled
                    && !circuit_tripped
                    && !any_cancelled_tool
                    && !asked_user_recently
                    && plan_continuation_nudges < max_boulder_nudges
                    && (!turn_tool_calls.is_empty() || !clean_text.trim().is_empty())
                {
                    if let Some(plan_summary) = wm.read_active_plan_summary() {
                        if plan_summary.total_tasks > 0
                            && plan_summary.completed_tasks < plan_summary.total_tasks
                        {
                            plan_continuation_nudges += 1;
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

                    // Also check active TaskDAG (.minicode/task_dag.json) for unblocked incomplete tasks
                    if let Ok(dag) = crate::agent::task_dag::TaskDag::load(&self.workspace_root) {
                        if !dag.tasks.is_empty() {
                            let unblocked = dag.next_executable_tasks();
                            if !unblocked.is_empty() {
                                plan_continuation_nudges += 1;
                                let completed_count = dag
                                    .tasks
                                    .values()
                                    .filter(|t| {
                                        t.status == crate::agent::task_dag::TaskStatus::Completed
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
        let mut max_turn_id: usize = 0;
        let mut total_tokens: usize = 0;
        let mut first_turn_prompt: Option<String> = None;

        for event in events {
            match event {
                AgentEvent::UserPrompt {
                    turn_id, prompt, ..
                } => {
                    // If there was an in-flight assistant response from previous turn, flush it
                    if !current_delta.is_empty() {
                        let clean = crate::agent::prompt::strip_thought_blocks(&current_delta);
                        self.messages.push(Message::assistant(clean));
                        current_delta.clear();
                    }
                    max_turn_id = max_turn_id.max(*turn_id);

                    if first_turn_prompt.is_none() {
                        first_turn_prompt = Some(prompt.clone());
                    }

                    self.messages.push(Message::user(prompt.clone()));
                }
                AgentEvent::StreamDelta { delta, .. } => {
                    current_delta.push_str(delta);
                }
                AgentEvent::ToolCall {
                    tool_id,
                    tool,
                    args,
                    ..
                } => {
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
                        self.messages.push(Message::assistant_with_tools(
                            clean,
                            std::mem::take(&mut pending_tool_calls),
                        ));
                        current_delta.clear();
                    }

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
        if !pending_tool_calls.is_empty() {
            let clean = crate::agent::prompt::strip_thought_blocks(&current_delta);
            self.messages
                .push(Message::assistant_with_tools(clean, pending_tool_calls));
        } else if !current_delta.is_empty() {
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
        // Established project so the one-time foundation checkpoint does not apply.
        let _ = std::fs::write(temp_dir.join("package.json"), "{}");
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
        // Established project so the foundation gate does not hold exec_cmd.
        let _ = std::fs::write(temp_dir.join("package.json"), "{}");
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
        let mut config = Config::default();
        config.provider.model = "dummy".to_string();
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

    #[tokio::test]
    async fn test_lazy_session_creation_deferred_until_prompt() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_lazy_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let mut config = Config::default();
        config.provider.model = "dummy".to_string();
        let provider = Box::new(DummyProvider);

        // 1. Initializing AgentLoop must NOT create any session file on disk!
        let agent = AgentLoop::new(&temp_dir, config, provider);
        let session_file = agent.session_store.session_file_path(agent.session_id());
        assert!(
            !session_file.exists(),
            "Session file must not exist before first prompt"
        );
        assert!(!agent.session_persisted);

        // 2. Dropping agent without prompt must leave zero files on disk
        drop(agent);
        assert!(
            !session_file.exists(),
            "Session file must not exist after drop without prompt"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

#[cfg(test)]
mod inquiry_tests {
    use super::*;
    use crate::agent::inquiry::*;
    use crate::agent::provider::{
        ChunkStream, CompletionOptions, Provider, StreamChunk, ToolSchema,
    };
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct TestToolCallProvider {
        call: ToolCall,
        spent: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl Provider for TestToolCallProvider {
        fn name(&self) -> &str {
            "test-inquiry-provider"
        }
        fn default_model(&self) -> &str {
            "test-model"
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
    async fn test_inquiry_registry_instantiation() {
        let registry: InquiryRegistry = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = tokio::sync::oneshot::channel::<InquiryResponse>();
        registry.lock().unwrap().insert("test-inq".to_string(), tx);

        let sender = registry.lock().unwrap().remove("test-inq");
        assert!(sender.is_some());
        let _ = sender.unwrap().send(InquiryResponse {
            inquiry_id: "test-inq".to_string(),
            answers: vec![],
            cancelled: false,
        });

        let resp = rx.await.unwrap();
        assert_eq!(resp.inquiry_id, "test-inq");
        assert!(!resp.cancelled);
    }

    #[tokio::test]
    async fn test_inquiry_suspension_interactive_resolution() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_inq_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(TestToolCallProvider {
            call: ToolCall {
                id: "call_inq_1".into(),
                name: "ask_user".into(),
                arguments: serde_json::json!({
                    "title": "Choose database",
                    "questions": [
                        {
                            "id": "db",
                            "question": "Which database?",
                            "options": [
                                { "id": "pg", "label": "PostgreSQL", "recommended": true },
                                { "id": "sqlite", "label": "SQLite" }
                            ]
                        }
                    ]
                }),
            },
            spent: std::sync::atomic::AtomicBool::new(false),
        });

        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        agent_loop.set_interactive_approvals(true);
        let inq_reg = agent_loop.inquiry_registry();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let turn_task =
            tokio::spawn(async move { agent_loop.execute_turn("setup db", tx, None).await });

        // Wait for AgentEvent::UserInquiry
        let mut captured_inquiry_id = None;
        for _ in 0..50 {
            match rx.recv().await {
                Some(AgentEvent::UserInquiry {
                    tool_id, request, ..
                }) => {
                    assert_eq!(tool_id, "call_inq_1");
                    assert_eq!(request.title, "Choose database");
                    captured_inquiry_id = Some(tool_id);
                    break;
                }
                Some(_) => {}
                None => panic!("event stream ended before UserInquiry"),
            }
        }
        let inquiry_id = captured_inquiry_id.expect("UserInquiry event must be emitted");

        // Verify turn is suspended
        assert!(
            !turn_task.is_finished(),
            "turn must suspend awaiting inquiry response"
        );

        // Respond via inquiry registry
        let sender = inq_reg
            .lock()
            .unwrap()
            .remove(&inquiry_id)
            .expect("inquiry must be in registry");

        let response = InquiryResponse {
            inquiry_id: inquiry_id.clone(),
            answers: vec![InquiryAnswer {
                question_id: "db".to_string(),
                selected_options: vec!["pg".to_string()],
                custom_text: None,
                masked: false,
            }],
            cancelled: false,
        };
        assert!(sender.send(response).is_ok());

        let turn = turn_task.await.expect("join ok").expect("turn completes");
        assert_eq!(turn.tool_results.len(), 1);
        assert!(turn.tool_results[0].success);
        assert!(turn.tool_results[0].output.contains("answered"));
        assert!(turn.tool_results[0].output.contains("pg"));

        // Registry should now be empty for this inquiry
        assert!(inq_reg.lock().unwrap().get(&inquiry_id).is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_inquiry_suspension_cancellation() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_inq_cancel_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(TestToolCallProvider {
            call: ToolCall {
                id: "call_inq_cancel".into(),
                name: "ask_user".into(),
                arguments: serde_json::json!({
                    "title": "Choose stack",
                    "questions": [
                        {
                            "id": "stack",
                            "question": "Which stack?",
                            "options": [
                                { "id": "react", "label": "React" }
                            ]
                        }
                    ]
                }),
            },
            spent: std::sync::atomic::AtomicBool::new(false),
        });

        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        agent_loop.set_interactive_approvals(true);
        let inq_reg = agent_loop.inquiry_registry();

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let turn_task = tokio::spawn(async move {
            agent_loop
                .execute_turn("setup stack", tx, Some(cancel_clone))
                .await
        });

        // Wait for AgentEvent::UserInquiry
        for _ in 0..50 {
            match rx.recv().await {
                Some(AgentEvent::UserInquiry { tool_id, .. }) => {
                    assert_eq!(tool_id, "call_inq_cancel");
                    break;
                }
                Some(_) => {}
                None => panic!("event stream ended before UserInquiry"),
            }
        }

        // Cancel while suspended
        cancel_token.cancel();

        let turn = turn_task.await.expect("join ok").expect("turn completes");
        assert_eq!(turn.tool_results.len(), 1);
        assert!(turn.tool_results[0].output.contains("cancelled"));

        // Registry must be cleaned up
        assert!(inq_reg.lock().unwrap().get("call_inq_cancel").is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_inquiry_non_interactive_auto_resolve() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_inq_noninter_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(TestToolCallProvider {
            call: ToolCall {
                id: "call_inq_auto".into(),
                name: "ask_user".into(),
                arguments: serde_json::json!({
                    "title": "Pick template",
                    "questions": [
                        {
                            "id": "tmpl",
                            "question": "Which template?",
                            "options": [
                                { "id": "default_tmpl", "label": "Default", "recommended": true },
                                { "id": "custom_tmpl", "label": "Custom" }
                            ]
                        }
                    ]
                }),
            },
            spent: std::sync::atomic::AtomicBool::new(false),
        });

        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        // Non-interactive / headless mode (e.g. -y)
        agent_loop.set_interactive_approvals(false);
        let inq_reg = agent_loop.inquiry_registry();

        let (tx, mut rx) = mpsc::unbounded_channel();
        // Should complete without hanging or suspending
        let turn = agent_loop
            .execute_turn("auto template", tx, None)
            .await
            .expect("turn completes immediately");

        assert_eq!(turn.tool_results.len(), 1);
        assert!(turn.tool_results[0].success);
        assert!(turn.tool_results[0].output.contains("answered"));
        assert!(turn.tool_results[0].output.contains("default_tmpl"));

        // No UserInquiry event should have been emitted in non-interactive mode
        let mut saw_user_inquiry = false;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AgentEvent::UserInquiry { .. }) {
                saw_user_inquiry = true;
            }
        }
        assert!(
            !saw_user_inquiry,
            "UserInquiry event must NOT be emitted in non-interactive mode"
        );

        // Registry should be empty
        assert!(inq_reg.lock().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_inquiry_empty_questions_fails_fast() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_inq_empty_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(TestToolCallProvider {
            call: ToolCall {
                id: "call_inq_empty".into(),
                name: "ask_user".into(),
                arguments: serde_json::json!({
                    "title": "Empty Questions Inquiry",
                    "questions": []
                }),
            },
            spent: std::sync::atomic::AtomicBool::new(false),
        });

        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        agent_loop.set_interactive_approvals(true);

        let (tx, _rx) = mpsc::unbounded_channel();
        let turn = agent_loop
            .execute_turn("test empty inquiry", tx, None)
            .await
            .expect("turn completes");

        assert_eq!(turn.tool_results.len(), 1);
        assert!(!turn.tool_results[0].success);
        assert!(turn.tool_results[0]
            .output
            .contains("requires at least one question"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    struct TrailingErrorToolCallProvider {
        spent: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl Provider for TrailingErrorToolCallProvider {
        fn name(&self) -> &str {
            "trailing-error-provider"
        }
        fn default_model(&self) -> &str {
            "test-model"
        }
        async fn stream_completion(
            &self,
            _messages: &[Message],
            _tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> Result<ChunkStream> {
            let was_spent = self.spent.swap(true, std::sync::atomic::Ordering::SeqCst);
            if was_spent {
                let s = async_stream::stream! {
                    yield Ok(StreamChunk::Done);
                };
                return Ok(Box::pin(s));
            }

            let s = async_stream::stream! {
                yield Ok(StreamChunk::Delta("I will find files.\n".to_string()));
                yield Ok(StreamChunk::ToolCallChunk(ToolCall {
                    id: "call_trailing_test".to_string(),
                    name: "file_search".to_string(),
                    arguments: serde_json::json!({ "pattern": "*" }),
                }));
                // Trailing network closure after tool call was emitted
                yield Err(crate::error::ProviderError::StreamDecode(
                    "SSE stream error: error decoding response body".to_string(),
                ).into());
            };
            Ok(Box::pin(s))
        }
    }

    #[tokio::test]
    async fn test_trailing_stream_closure_with_tool_calls_tolerated() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_trailing_err_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(TrailingErrorToolCallProvider {
            spent: std::sync::atomic::AtomicBool::new(false),
        });

        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        let (tx, _rx) = mpsc::unbounded_channel();
        let turn = agent_loop
            .execute_turn("list files", tx, None)
            .await
            .expect("turn should succeed by tolerating trailing stream closure");

        assert_eq!(turn.tool_results.len(), 1);
        assert_eq!(turn.tool_results[0].tool_name, "file_search");
        assert!(turn.tool_results[0].success);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    struct ThinkOnlyDropThenSuccessProvider {
        calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl Provider for ThinkOnlyDropThenSuccessProvider {
        fn name(&self) -> &str {
            "think-only-drop-provider"
        }
        fn default_model(&self) -> &str {
            "test-model"
        }
        async fn stream_completion(
            &self,
            _messages: &[Message],
            _tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> Result<ChunkStream> {
            let call_idx = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if call_idx == 0 {
                // Attempt 0: Stream drops right after <think> block before tool call arrives
                let s = async_stream::stream! {
                    yield Ok(StreamChunk::Delta("<think>Planning next component...</think>".to_string()));
                    yield Err(crate::error::ProviderError::StreamDecode(
                        "SSE stream transport error: error decoding response body".to_string(),
                    ).into());
                };
                return Ok(Box::pin(s));
            }
            if call_idx == 1 {
                // Attempt 1 (retry): Emits the tool call cleanly
                let s = async_stream::stream! {
                    yield Ok(StreamChunk::Delta("Searching workspace files.\n".to_string()));
                    yield Ok(StreamChunk::ToolCallChunk(ToolCall {
                        id: "call_retry_ok".to_string(),
                        name: "file_search".to_string(),
                        arguments: serde_json::json!({ "pattern": "*" }),
                    }));
                    yield Ok(StreamChunk::Done);
                };
                return Ok(Box::pin(s));
            }
            let s = async_stream::stream! {
                yield Ok(StreamChunk::Delta("Done.".to_string()));
                yield Ok(StreamChunk::Done);
            };
            Ok(Box::pin(s))
        }
    }

    #[tokio::test]
    async fn test_stream_closure_after_think_only_retries_instead_of_exiting() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_think_drop_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(ThinkOnlyDropThenSuccessProvider {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });

        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        let (tx, _rx) = mpsc::unbounded_channel();
        let turn = agent_loop
            .execute_turn("continue", tx, None)
            .await
            .expect("turn should retry after think-only stream drop and succeed");

        assert_eq!(
            turn.tool_results.len(),
            1,
            "Expected retry to execute the tool call instead of prematurely ending turn"
        );
        assert_eq!(turn.tool_results[0].tool_name, "file_search");
        assert!(turn.tool_results[0].success);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_classify_mandatory_approval_enforces_git_push_pull_and_critical_ops() {
        // Dedicated tools
        assert!(classify_mandatory_approval("git_push", &serde_json::json!({})).is_some());
        assert!(classify_mandatory_approval("git_pull", &serde_json::json!({})).is_some());
        assert!(
            classify_mandatory_approval("create_pr", &serde_json::json!({"title": "feat: x"}))
                .is_some()
        );

        // Shell commands inside exec_cmd
        assert!(classify_mandatory_approval(
            "exec_cmd",
            &serde_json::json!({"command": "git push origin main"})
        )
        .is_some());
        assert!(classify_mandatory_approval(
            "exec_cmd",
            &serde_json::json!({"command": "git status && git pull --rebase origin main"})
        )
        .is_some());
        assert!(classify_mandatory_approval(
            "exec_cmd",
            &serde_json::json!({"command": "git reset --hard HEAD~1"})
        )
        .is_some());
        assert!(classify_mandatory_approval(
            "exec_cmd",
            &serde_json::json!({"command": "git clean -fd"})
        )
        .is_some());
        assert!(classify_mandatory_approval(
            "exec_cmd",
            &serde_json::json!({"command": "gh pr merge 42 --squash"})
        )
        .is_some());
        assert!(classify_mandatory_approval(
            "exec_cmd",
            &serde_json::json!({"command": "npm publish"})
        )
        .is_some());

        // Safe local commands must NOT trigger mandatory approval when auto_approve is true
        assert!(classify_mandatory_approval(
            "exec_cmd",
            &serde_json::json!({"command": "git status && git diff"})
        )
        .is_none());
        assert!(classify_mandatory_approval(
            "exec_cmd",
            &serde_json::json!({"command": "bun run build && bun run lint"})
        )
        .is_none());
        assert!(classify_mandatory_approval(
            "exec_cmd",
            &serde_json::json!({"command": "rm -rf dist node_modules/.vite"})
        )
        .is_none());
    }
}
