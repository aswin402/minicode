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
        info_val.provider_override = config.provider.clone();
        info_val.model_override = config.model.clone();
        info_val.fallback_1_override = config.fallback_1.clone();
        info_val.fallback_2_override = config.fallback_2.clone();
        info_val.use_orchestrator_model = config.use_orchestrator_model;
        let info = Arc::new(RwLock::new(info_val));

        Self {
            info,
            config,
            workspace_root: workspace_root.to_path_buf(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            swarm_context: None,
        }
    }

    /// Resolves the active 4-slot `FallbackChainProvider` (`Primary -> 1st Fallback -> 2nd Fallback -> Global Last Fallback ★`)
    /// and primary model for this `MiniPub` worker, honoring per-role configuration, spawn overrides,
    /// and live runtime hot-swaps from the Orchestrator.
    fn resolve_worker_provider_chain(
        &self,
        cfg: &crate::config::Config,
        fallback_provider: &Arc<dyn Provider>,
        info: &SubagentInfo,
    ) -> (Arc<dyn Provider>, String, String) {
        let prov_name_lower = fallback_provider.name().to_lowercase();
        let is_test_mock = prov_name_lower.contains("mock") || prov_name_lower.contains("test");
        if is_test_mock && info.model_generation == 0 && info.provider_override.is_none() {
            let mdl = info
                .model_override
                .clone()
                .or_else(|| self.config.model.clone())
                .unwrap_or_else(|| fallback_provider.default_model().to_string());
            let lbl = format!("{}:{}", fallback_provider.name(), mdl);
            return (Arc::clone(fallback_provider), mdl, lbl);
        }

        let role_slug = info.role.badge().to_lowercase();

        let mut raw_slots = if info.use_orchestrator_model {
            cfg.get_four_slots_for_target("global", Some(&self.workspace_root))
        } else {
            cfg.get_four_slots_for_target(&role_slug, Some(&self.workspace_root))
        };

        if !info.use_orchestrator_model {
            let default_prov = raw_slots[0]
                .as_ref()
                .map(|s| s.provider.clone())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| {
                    if !cfg.provider.default.trim().is_empty() {
                        cfg.provider.default.trim().to_lowercase()
                    } else {
                        fallback_provider.name().to_lowercase()
                    }
                });

            match (&info.provider_override, &info.model_override) {
                (Some(p), Some(m)) => {
                    let slot = crate::config::ModelSlot::new(p, m);
                    if slot.is_valid() {
                        raw_slots[0] = Some(slot);
                    }
                }
                (None, Some(m)) => {
                    if let Some(slot) = crate::config::ModelSlot::parse_loose(m, &default_prov) {
                        raw_slots[0] = Some(slot);
                    }
                }
                (Some(p), None) => {
                    let mdl = cfg.get_default_model_for_provider(p);
                    let slot = crate::config::ModelSlot::new(p, mdl);
                    if slot.is_valid() {
                        raw_slots[0] = Some(slot);
                    }
                }
                (None, None) => {}
            }

            if let Some(ref f1) = info.fallback_1_override {
                if f1.is_valid() {
                    raw_slots[1] = Some(f1.clone());
                }
            }
            if let Some(ref f2) = info.fallback_2_override {
                if f2.is_valid() {
                    raw_slots[2] = Some(f2.clone());
                }
            }
        }

        let deduped = crate::config::Config::dedup_slots(raw_slots);

        let has_custom_chain = info.model_generation > 0
            || info.use_orchestrator_model
            || info.provider_override.is_some()
            || info.model_override.is_some()
            || info.fallback_1_override.is_some()
            || info.fallback_2_override.is_some()
            || cfg.has_custom_role_chain(&role_slug)
            || !cfg.agent.minipub.default_chain.is_empty()
            || deduped.len() > 1;

        if !deduped.is_empty() && has_custom_chain && !is_test_mock {
            let (chain_prov, _) =
                crate::agent::providers::create_chain_provider_for_slots(cfg, &deduped);
            let primary_model = deduped[0].model.clone();
            let label = if deduped.len() > 1 {
                format!(
                    "{}:{} (+{} fallback{})",
                    deduped[0].provider,
                    deduped[0].model,
                    deduped.len() - 1,
                    if deduped.len() == 2 { "" } else { "s" }
                )
            } else {
                format!("{}:{}", deduped[0].provider, deduped[0].model)
            };
            return (Arc::from(chain_prov), primary_model, label);
        }

        let mdl = deduped
            .first()
            .map(|s| s.model.clone())
            .or_else(|| self.config.model.clone())
            .unwrap_or_else(|| fallback_provider.default_model().to_string());
        let lbl = format!("{}:{}", fallback_provider.name(), mdl);
        (Arc::clone(fallback_provider), mdl, lbl)
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

    /// Resolves the canonical repository root even when this worker executes inside
    /// an isolated Git worktree (`.minicode/worktrees/<id>`).
    pub fn main_workspace_root(&self) -> PathBuf {
        crate::context::governance::dox::DoxEngine::canonical_workspace_root(&self.workspace_root)
    }

    /// Generates the specialized system prompt for the `minipub` worker's role, including
    /// any Orchestrator-assigned todo checklist and file boundaries.
    pub fn build_system_prompt(&self) -> String {
        let main_root = self.main_workspace_root();
        let app_cfg = crate::config::Config::load_for_workspace(&main_root);
        self.build_system_prompt_with_config(&app_cfg)
    }

    fn build_system_prompt_with_config(&self, app_cfg: &crate::config::Config) -> String {
        let main_root = self.main_workspace_root();
        let base = if let Some(custom) = &self.config.system_prompt_override {
            custom.clone()
        } else {
            self.config.role.load_role_prompt(Some(&main_root))
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

        // Inject semantic intent category directive if specified (e.g. ultrabrain, visual-engineering, quick, explore)
        if let Some(directive) = self.config.category_directive() {
            full_prompt.push_str("\n\n<minipub_intent_category>\n");
            full_prompt.push_str(directive);
            full_prompt.push_str("\n</minipub_intent_category>");
        }

        // Automatic + Explicit Domain Skill Injection from MiniVault (`load_skills` + task/file auto-match)
        let skills_block = self.resolve_minipub_skills_block();
        if !skills_block.is_empty() {
            full_prompt.push_str("\n\n");
            full_prompt.push_str(&skills_block);
        }

        // Inject Canonical Workspace Rules (.minicode/RULES.md) + Cumulative Session Wisdom Notepad (.minicode/notepads/session_wisdom.md)
        let wisdom_block = self.resolve_minipub_wisdom_block();
        if !wisdom_block.is_empty() {
            full_prompt.push_str("\n\n");
            full_prompt.push_str(&wisdom_block);
        }

        let flag_level = crate::agent::caveman::read_flag_safe(
            &crate::agent::caveman::default_flag_path(&main_root),
        );
        let caveman_enabled =
            app_cfg.agent.caveman_mode || flag_level.map(|l| l.is_enabled()).unwrap_or(false);
        if caveman_enabled {
            let level_str = flag_level
                .filter(|l| l.is_enabled())
                .map(|l| l.as_str())
                .unwrap_or(app_cfg.agent.caveman_level.as_str());
            full_prompt.push_str(&crate::agent::caveman::cavecrew_subagent_block(
                true,
                self.config.role.badge(),
                level_str,
            ));
        }

        full_prompt
    }

    /// Resolves both explicitly requested `load_skills` and auto-matched domain skills from `VaultStore`
    /// tailored to this `MiniPub` worker's prompt, role, and target file boundaries.
    fn resolve_minipub_skills_block(&self) -> String {
        let main_root = self.main_workspace_root();
        let vault = crate::vault::store::VaultStore::new(&main_root);
        let mut selected_skills = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        // 1. Explicitly requested skills (`load_skills`)
        for req_name in &self.config.load_skills {
            if let Ok(skill) = vault.get_skill(req_name) {
                if seen_names.insert(skill.name.to_lowercase()) {
                    selected_skills.push(skill);
                }
            }
        }

        // 2. Auto-matched skills based on MiniPub task prompt, role/category, and file boundaries
        let task_prompt = self
            .info
            .try_read()
            .ok()
            .map(|i| i.prompt.clone())
            .unwrap_or_default();
        let role_hint = self
            .config
            .category
            .as_deref()
            .unwrap_or_else(|| self.config.role.as_str());
        let auto_matched = vault.match_skills_for_context(
            Some(&task_prompt),
            Some(role_hint),
            &self.config.file_boundaries,
        );
        for skill in auto_matched {
            if selected_skills.len() >= 3 {
                break;
            }
            if skill.kind() == crate::vault::models::SkillKind::Core {
                continue;
            }
            if seen_names.insert(skill.name.to_lowercase()) {
                selected_skills.push(skill);
            }
        }

        if selected_skills.is_empty() {
            return String::new();
        }

        let mut out = String::from(
            "<minipub_domain_skills>\n## Specialized Domain Skills (Auto-Injected for MiniPub)\n",
        );
        let mut chars_used = 0usize;
        const MAX_SKILL_CHARS: usize = 3_200;

        for skill in selected_skills {
            let body = skill.instructions.trim();
            if body.is_empty() {
                continue;
            }
            let snippet = if body.len() > 1_200 {
                crate::utils::strings::truncate_ellipsis(body, 1_200)
            } else {
                body.to_string()
            };
            if chars_used + snippet.len() > MAX_SKILL_CHARS && chars_used > 0 {
                break;
            }
            out.push_str(&format!("### Skill: `{}`\n{}\n\n", skill.name, snippet));
            chars_used += snippet.len();
        }
        out.push_str("</minipub_domain_skills>");
        out
    }

    /// Reads canonical `.minicode/RULES.md` (scoped to file boundaries) and the cumulative
    /// `.minicode/notepads/session_wisdom.md` notepad so every `MiniPub` worker inherits
    /// conventions, gotchas, and verified findings discovered by prior `MiniPub` workers.
    fn resolve_minipub_wisdom_block(&self) -> String {
        Self::resolve_minipub_wisdom_block_for_workspace(
            &self.workspace_root,
            &self.config.file_boundaries,
        )
    }

    pub fn resolve_minipub_wisdom_block_for_workspace(
        workspace_root: &std::path::Path,
        file_boundaries: &[String],
    ) -> String {
        let main_root =
            crate::context::governance::dox::DoxEngine::canonical_workspace_root(workspace_root);
        let mut sections = Vec::new();

        // 1. Scoped canonical rules from .minicode/RULES.md
        let rules = crate::context::governance::dox::DoxEngine::resolve_scoped_rules_with_budget(
            workspace_root,
            file_boundaries,
            1_600,
        );
        if !rules.trim().is_empty() {
            sections.push(format!(
                "### Canonical Workspace Rules (`.minicode/RULES.md`)\n{}",
                rules.trim()
            ));
        }

        // 2. Cumulative Session Wisdom Notepad (.minicode/notepads/session_wisdom.md)
        let wisdom_path = main_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::SESSION_WISDOM_FILE);
        if let Ok(wisdom_content) = std::fs::read_to_string(&wisdom_path) {
            let trimmed = wisdom_content.trim();
            if !trimmed.is_empty() {
                let tail = if trimmed.len() > 2_400 {
                    let start = trimmed.floor_char_boundary(trimmed.len() - 2_400);
                    format!("...\n{}", &trimmed[start..])
                } else {
                    trimmed.to_string()
                };
                sections.push(format!(
                    "### Inherited Session Wisdom (from Prior MiniPub Workers)\n{}",
                    tail
                ));
            }
        }

        if sections.is_empty() {
            return String::new();
        }

        format!(
            "<minipub_accumulated_wisdom>\n{}\n</minipub_accumulated_wisdom>",
            sections.join("\n\n")
        )
    }

    /// Executes the `minipub` subagent loop and returns the final boomerang result
    pub fn run<'a>(
        &'a self,
        provider: Arc<dyn Provider>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<SubagentResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let (prompt, role, id, initial_info_snapshot) = {
                let info = self.info.read().await;
                (
                    info.prompt.clone(),
                    info.role,
                    info.id.clone(),
                    info.clone(),
                )
            };

            let main_root = self.main_workspace_root();
            let app_cfg = crate::config::Config::load_for_workspace(&main_root);
            let (mut active_provider, initial_model, initial_label) =
                self.resolve_worker_provider_chain(&app_cfg, &provider, &initial_info_snapshot);
            let mut active_generation = initial_info_snapshot.model_generation;
            let mut retried_with_orchestrator_chain = initial_info_snapshot.use_orchestrator_model;

            {
                let mut info = self.info.write().await;
                info.active_model_label = Some(initial_label.clone());
            }

            tracing::info!(
                minipub_id = %id,
                role = ?role,
                model_chain = %initial_label,
                "Starting minipub subagent worker"
            );

            let system_prompt = self.build_system_prompt_with_config(&app_cfg);
            let initial_images = if role == SubagentRole::Vision {
                extract_prompt_image_attachments(&self.workspace_root, &prompt)
            } else {
                Vec::new()
            };
            let mut messages = vec![Message::user_with_images(prompt.clone(), initial_images)];

            // Filter tools to role's capability whitelist using cached global schema registry
            let filtered_tools: Vec<_> = get_cached_tool_schemas()
                .iter()
                .filter(|schema| self.config.tool_whitelist.contains(&schema.name))
                .cloned()
                .collect();

            let configured_max_tokens = app_cfg.provider.max_tokens.clamp(4096, 16384);

            let mut options = CompletionOptions {
                model: initial_model,
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
            let mut thought_only_nudges: usize = 0;
            let mut last_seen_swarm_msg_id: Option<String> = None;
            let mut seen_global_msg_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut transcript = crate::agent::subagent::transcript::SubagentTranscript::new(
                &id,
                role,
                prompt.clone(),
            );

            loop {
                let (
                    live_max_turns,
                    live_token_budget,
                    live_timeout_secs,
                    drained_steers,
                    maybe_new_info,
                ) = {
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
                    let snap = if info.model_generation != active_generation {
                        Some(info.clone())
                    } else {
                        None
                    };
                    (mt, tb, to, steers, snap)
                };

                if let Some(updated_info) = maybe_new_info {
                    let (new_prov, new_mdl, new_lbl) =
                        self.resolve_worker_provider_chain(&app_cfg, &provider, &updated_info);
                    active_provider = new_prov;
                    options.model = new_mdl;
                    active_generation = updated_info.model_generation;
                    let mut info = self.info.write().await;
                    info.active_model_label = Some(new_lbl.clone());
                    tracing::info!(
                        minipub_id = %id,
                        new_model_chain = %new_lbl,
                        "MiniPub dynamically hot-swapped provider/model in-flight"
                    );
                }

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

                // Stream completions from active_provider guarded by activity-heartbeat timeout
                let stream_res = tokio::time::timeout(
                    inactivity_timeout,
                    active_provider.stream_completion(&messages, &filtered_tools, &options),
                )
                .await;

                let mut stream = match stream_res {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        if !retried_with_orchestrator_chain {
                            retried_with_orchestrator_chain = true;
                            let orch_info = {
                                let mut info = self.info.write().await;
                                info.use_orchestrator_model = true;
                                info.provider_override = None;
                                info.model_override = None;
                                info.model_generation = info.model_generation.saturating_add(1);
                                info.status_message = Some(
                                    "MiniPub model error -> falling back to Orchestrator model chain"
                                        .to_string(),
                                );
                                info.clone()
                            };
                            let (new_prov, new_mdl, new_lbl) =
                                self.resolve_worker_provider_chain(&app_cfg, &provider, &orch_info);
                            active_provider = new_prov;
                            options.model = new_mdl;
                            active_generation = orch_info.model_generation;
                            {
                                let mut info = self.info.write().await;
                                info.active_model_label = Some(new_lbl.clone());
                            }
                            tracing::warn!(
                                minipub_id = %id,
                                error = %e,
                                fallback_chain = %new_lbl,
                                "MiniPub custom model failed; dynamically retrying with Orchestrator model chain"
                            );
                            turns_executed = turns_executed.saturating_sub(1);
                            continue;
                        }
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

                if self.cancel_flag.load(Ordering::SeqCst) {
                    success = false;
                    break;
                }

                let clean_iteration_text =
                    crate::agent::prompt::strip_thought_blocks(&iteration_text);
                if !clean_iteration_text.is_empty() || final_summary.is_empty() {
                    final_summary = clean_iteration_text.clone();
                }
                {
                    let mut info = self.info.write().await;
                    sync_todos_from_activity(
                        &mut info.todos,
                        &clean_iteration_text,
                        &files_modified,
                    );
                }

                if pending_tool_calls.is_empty() {
                    // If the model emitted only internal reasoning (<thought>/<think>) with no tool calls
                    // and no visible report on any turn, nudge it (up to 2 times) to invoke its tools or emit a visible report.
                    if clean_iteration_text.trim().is_empty()
                        && thought_only_nudges < 2
                        && turns_executed < live_max_turns
                    {
                        thought_only_nudges += 1;
                        messages.push(Message::assistant(iteration_text));
                        messages.push(Message::user(
                            "[System: Your previous response contained only internal reasoning (<thought>/<think>) with no tool calls or visible output. Invoke the required tools now or provide your final visible analysis.]".to_string(),
                        ));
                        continue;
                    }
                    // Agent concluded with final response
                    break;
                }
                thought_only_nudges = 0;

                // Free base64 image payloads from already-consumed turns so multi-turn
                // Vision/QA MiniPubs do not retain or re-transmit stale screenshots in RAM.
                for msg in &mut messages {
                    msg.images = None;
                }

                // Record assistant tool calls
                messages.push(Message::assistant_with_tools(
                    clean_iteration_text,
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
                                if !files_modified.iter().any(|existing| existing == p) {
                                    files_modified.push(p.to_string());
                                }
                            } else if !files_inspected.iter().any(|existing| existing == p) {
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

                    let tool_images = if tool_res.success {
                        extract_tool_image_attachments(&self.workspace_root, &tool_name, &args)
                    } else {
                        Vec::new()
                    };
                    let pruned_output = crate::context::ObservationPruner::prune_for_llm(
                        &tool_name,
                        &tool_res.output,
                    );
                    // Mask older tool observations (> 6 messages behind current tail) to keep MiniPub RAM minimal
                    if messages.len() > 8 {
                        let cutoff = messages.len().saturating_sub(6);
                        for old_msg in messages.iter_mut().take(cutoff) {
                            if old_msg.role == crate::agent::types::Role::Tool
                                && old_msg.content.len() > 1200
                            {
                                old_msg.content = format!(
                                    "{}\n...[observation condensed after turn]...",
                                    crate::utils::strings::truncate_ellipsis(&old_msg.content, 600)
                                );
                            }
                        }
                    }
                    messages.push(
                        Message::tool_result(call_id, tool_name, pruned_output)
                            .with_images(tool_images),
                    );
                }
            }

            let now = now_unix_secs();

            transcript.total_tokens = tokens_used;
            transcript.turns_executed = turns_executed;
            transcript.finished_at_secs = Some(now);
            crate::agent::subagent::transcript::get_global_transcript_store()
                .store(transcript.clone(), Some(&main_root));

            let isolate_worktree = {
                let info = self.info.read().await;
                info.isolate_worktree
            };
            let worktree_branch = if isolate_worktree {
                Some(format!("subagent/{}", id))
            } else {
                None
            };

            let was_cancelled = self.cancel_flag.load(Ordering::SeqCst) || {
                let info = self.info.read().await;
                matches!(info.state, SubagentState::Terminated)
            };
            if was_cancelled {
                success = false;
                if final_summary.trim().is_empty() {
                    final_summary =
                        "Subagent execution was canceled before completion.".to_string();
                }
            }
            let status_str = if was_cancelled {
                "canceled"
            } else if success {
                "completed"
            } else {
                "failed"
            };

            let report = crate::agent::subagent::reducer::SubagentSynthesisReducer::reduce(
                crate::agent::subagent::reducer::SubagentSynthesisContext {
                    subagent_id: &id,
                    role: &role,
                    final_summary: &final_summary,
                    success,
                    status: status_str,
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

            // Auto-deposit synthesized report into shared scratchpad under canonical key
            // (`read_entry` resolves `subagent_<id>` and `minipub/<id>` aliases without duplicating strings in RAM)
            let scratchpad = crate::agent::subagent::get_global_scratchpad();
            let report_title = format!("MiniPub {} ({}) Report", id, role.badge());
            scratchpad.write_entry(
                &format!("subagent/{}", id),
                &report_title,
                &markdown_report,
                &id,
            );
            let _ = scratchpad.save_to_disk(&main_root);

            if !files_inspected.is_empty() || !files_modified.is_empty() {
                crate::context::governance::dox::DoxEngine::sync_and_harvest_rules(
                    &main_root,
                    &files_inspected,
                );
                crate::context::governance::dox::DoxEngine::sync_and_harvest_rules(
                    &main_root,
                    &files_modified,
                );
            }

            if !was_cancelled {
                append_session_wisdom_entry(
                    &main_root,
                    &id,
                    role,
                    &prompt,
                    &files_modified,
                    &files_inspected,
                    &final_summary,
                    success,
                );
            }

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
    messages
        .iter()
        .map(|m| crate::context::compressor::ContextCompressor::count_tokens_fast(&m.content))
        .sum()
}

/// Scans a `Vision` MiniPub's initial prompt for existing image file paths (`.png`, `.jpg`, `.jpeg`, `.webp`, `.gif`)
/// in the workspace and attaches them as `ImageAttachment`s so the Vision LLM can inspect them immediately.
fn extract_prompt_image_attachments(
    workspace_root: &std::path::Path,
    prompt: &str,
) -> Vec<crate::agent::types::ImageAttachment> {
    let mut attachments = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw_token in prompt.split_whitespace() {
        let token = raw_token.trim_matches(|c: char| {
            matches!(
                c,
                '`' | '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';' | ':'
            )
        });
        if token.is_empty() {
            continue;
        }
        let lower = token.to_ascii_lowercase();
        if !(lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".webp")
            || lower.ends_with(".gif"))
        {
            continue;
        }
        let candidate = {
            let p = std::path::Path::new(token);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                workspace_root.join(p)
            }
        };
        if let Ok(canon) = candidate.canonicalize() {
            if seen.insert(canon.clone()) {
                if let Some(att) = crate::agent::types::ImageAttachment::from_path(&canon) {
                    attachments.push(att);
                }
            }
        }
    }
    attachments
}

/// Extracts `ImageAttachment`s when a MiniPub worker invokes `read_file` or `browser_screenshot` on an image asset.
fn extract_tool_image_attachments(
    workspace_root: &std::path::Path,
    tool_name: &str,
    args: &serde_json::Value,
) -> Vec<crate::agent::types::ImageAttachment> {
    match tool_name {
        "read_file" | "browser_screenshot" => {
            if let Some(rel_path) = args
                .get("path")
                .or_else(|| args.get("file_path"))
                .and_then(|v| v.as_str())
            {
                let p = std::path::Path::new(rel_path);
                let full_path = if p.is_absolute() {
                    p.to_path_buf()
                } else if tool_name == "browser_screenshot"
                    && !rel_path.starts_with(".minicode/screenshots")
                {
                    let direct = workspace_root.join(p);
                    if direct.exists() {
                        direct
                    } else {
                        workspace_root.join(".minicode").join("screenshots").join(p)
                    }
                } else {
                    workspace_root.join(p)
                };
                if let Some(att) = crate::agent::types::ImageAttachment::from_path(&full_path) {
                    return vec![att];
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
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

/// Appends a distilled learning entry from a finished `MiniPub` worker into
/// `.minicode/notepads/session_wisdom.md` so subsequent `MiniPub` workers inherit
/// discovered conventions, modified file boundaries, and gotchas.
#[allow(clippy::too_many_arguments)]
fn append_session_wisdom_entry(
    workspace_root: &std::path::Path,
    minipub_id: &str,
    role: super::types::SubagentRole,
    prompt: &str,
    files_modified: &[String],
    files_inspected: &[String],
    final_summary: &str,
    success: bool,
) {
    let wisdom_path = workspace_root
        .join(crate::constants::WORKSPACE_DIR_NAME)
        .join(crate::constants::SESSION_WISDOM_FILE);
    if let Some(parent) = wisdom_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let task_brief = crate::utils::strings::truncate_ellipsis(
        prompt.lines().next().unwrap_or(prompt).trim(),
        120,
    );
    let summary_brief = crate::utils::strings::truncate_ellipsis(final_summary.trim(), 360);
    let mod_str = if files_modified.is_empty() {
        "none (read-only)".to_string()
    } else {
        files_modified
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    let inspected_str = if files_inspected.is_empty() {
        "none".to_string()
    } else {
        files_inspected
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };

    let entry = format!(
        "#### MiniPub `{}` ({}) — {}\n- **Task**: {}\n- **Modified**: `{}` | **Inspected**: `{}`\n- **Key Takeaway**: {}\n",
        minipub_id,
        role.badge(),
        if success { "Verified" } else { "Failed/Gotcha" },
        task_brief,
        mod_str,
        inspected_str,
        summary_brief.replace('\n', " ")
    );

    let mut existing = std::fs::read_to_string(&wisdom_path).unwrap_or_else(|_| {
        "# MiniPub Session Wisdom Notepad\n\n> Automatically accumulated across MiniPub subagent runs in this workspace.\n\n".to_string()
    });

    existing.push('\n');
    existing.push_str(&entry);

    // Keep notepad bounded (~6,000 bytes max) so it never bloats disk or prompt reads
    if existing.len() > 6_000 {
        let keep_from = existing.floor_char_boundary(existing.len() - 4_800);
        existing = format!(
            "# MiniPub Session Wisdom Notepad (Recent Entries)\n\n...\n{}",
            &existing[keep_from..]
        );
    }

    let _ = std::fs::write(&wisdom_path, existing);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::subagent::types::{SubagentConfig, SubagentRole};

    #[test]
    fn test_minipub_semantic_category_routing_and_directives() {
        assert_eq!(
            SubagentRole::from_str_loose("ultrabrain"),
            SubagentRole::Architect
        );
        assert_eq!(
            SubagentRole::from_str_loose("visual-engineering"),
            SubagentRole::Frontend
        );
        assert_eq!(SubagentRole::from_str_loose("explore"), SubagentRole::Scout);
        assert_eq!(
            SubagentRole::from_str_loose("security-audit"),
            SubagentRole::Security
        );
        assert_eq!(
            SubagentRole::from_str_loose("test-verification"),
            SubagentRole::Tester
        );

        let quick_cfg =
            SubagentConfig::for_role(SubagentRole::Coder).with_category(Some("quick".to_string()));
        assert_eq!(quick_cfg.max_turns, 8);
        assert_eq!(quick_cfg.token_budget, 24_000);
        assert!(quick_cfg
            .category_directive()
            .expect("directive")
            .contains("Fast Surgical Execution"));

        let ultra_cfg = SubagentConfig::for_role(SubagentRole::Planner)
            .with_category(Some("ultrabrain".to_string()))
            .with_load_skills(vec!["rust-pro".to_string()]);
        assert!(ultra_cfg.max_turns >= 18);
        assert!(ultra_cfg.token_budget >= 56_000);
        assert_eq!(ultra_cfg.load_skills, vec!["rust-pro".to_string()]);
        assert!(ultra_cfg
            .category_directive()
            .expect("directive")
            .contains("Deep Reasoning"));
    }

    #[test]
    fn test_minipub_session_wisdom_and_canonical_rules_injection() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".minicode")).expect("mkdir .minicode");
        std::fs::write(
            dir.path().join(".minicode/RULES.md"),
            "# Canonical Rules\n- Use `nice -n 19 cargo check -j 1`\n",
        )
        .expect("write RULES.md");

        append_session_wisdom_entry(
            dir.path(),
            "mp_1",
            SubagentRole::Scout,
            "Investigate DoxEngine rule harvesting",
            &[],
            &["src/context/governance/dox.rs".to_string()],
            "DoxEngine harvests foreign rules into .minicode/RULES.md cleanly.",
            true,
        );

        let wisdom_block =
            SubagentWorker::resolve_minipub_wisdom_block_for_workspace(dir.path(), &[]);
        assert!(wisdom_block.contains("<minipub_accumulated_wisdom>"));
        assert!(wisdom_block.contains("nice -n 19 cargo check -j 1"));
        assert!(wisdom_block.contains("DoxEngine harvests foreign rules"));
    }
}
