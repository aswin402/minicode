use super::types::{
    MiniPubTodoItem, MiniPubUpdate, SubagentConfig, SubagentInfo, SubagentResult, SubagentRole,
    SubagentState,
};
use super::worker::SubagentWorker;
use crate::agent::provider::Provider;
use crate::error::{MinicodeError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handle to a registered subagent worker
struct WorkerHandle {
    info: Arc<RwLock<SubagentInfo>>,
    cancel_flag: Arc<AtomicBool>,
}

/// Thread-safe supervisor pool managing concurrent `minipub` (subagent) workers
#[derive(Clone)]
pub struct SubagentPool {
    workers: Arc<RwLock<HashMap<String, WorkerHandle>>>,
    counter: Arc<RwLock<usize>>,
    workspace_root: PathBuf,
    provider: Arc<RwLock<Option<Arc<dyn Provider>>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    max_parallel: Arc<AtomicUsize>,
    completion_notify: Arc<tokio::sync::Notify>,
    reported_completions: Arc<RwLock<std::collections::HashSet<String>>>,
}

/// First-class alias for `SubagentPool`
#[allow(dead_code)]
pub type MiniPubPool = SubagentPool;

#[allow(dead_code)]
impl SubagentPool {
    pub fn new(workspace_root: &Path) -> Self {
        let cfg = crate::config::Config::load(Some(workspace_root), None).unwrap_or_default();
        let max_parallel = cfg.agent.minipub.max_parallel.clamp(
            crate::constants::MIN_PARALLEL_MINIPUBS,
            crate::constants::MAX_PARALLEL_MINIPUBS_CAP,
        );
        // Clear any stale file boundary leases left behind by a previous crashed session
        crate::sandbox::worktree::FileLeaseManager::clear_all_leases(workspace_root);
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            counter: Arc::new(RwLock::new(1)),
            workspace_root: workspace_root.to_path_buf(),
            provider: Arc::new(RwLock::new(None)),
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_parallel)),
            max_parallel: Arc::new(AtomicUsize::new(max_parallel)),
            completion_notify: Arc::new(tokio::sync::Notify::new()),
            reported_completions: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    /// Returns the configured maximum parallel `minipub` workers for this pool.
    pub fn max_parallel(&self) -> usize {
        self.max_parallel.load(Ordering::Relaxed)
    }

    /// Dynamically updates the maximum parallel `minipub` workers at runtime (`1..=16`).
    pub fn set_max_parallel(&self, new_max: usize) -> usize {
        let clamped = new_max.clamp(
            crate::constants::MIN_PARALLEL_MINIPUBS,
            crate::constants::MAX_PARALLEL_MINIPUBS_CAP,
        );
        let old = self.max_parallel.swap(clamped, Ordering::SeqCst);
        if clamped > old {
            self.semaphore.add_permits(clamped - old);
        }
        clamped
    }

    /// Waits until any worker state changes or `timeout` elapses (zero busy-polling).
    pub async fn wait_for_change(&self, timeout: std::time::Duration) {
        let _ = tokio::time::timeout(timeout, self.completion_notify.notified()).await;
    }

    /// Sets the active LLM provider for the swarm pool
    pub fn set_provider(&self, provider: Arc<dyn Provider>) {
        if let Ok(mut lock) = self.provider.try_write() {
            *lock = Some(provider);
        }
    }

    /// Retrieves or lazily creates an LLM provider for background subagents
    pub async fn get_or_create_provider(&self) -> Arc<dyn Provider> {
        {
            let lock = self.provider.read().await;
            if let Some(ref p) = *lock {
                return Arc::clone(p);
            }
        }

        let cfg = crate::config::Config::load(Some(&self.workspace_root), None).unwrap_or_default();
        let api_key_res = cfg.get_api_key(&cfg.provider.default);
        let custom_url = cfg.get_provider_base_url(&cfg.provider.default);
        let (prov, _) = crate::agent::providers::create_provider_or_fallback(
            &cfg.provider.default,
            api_key_res,
            custom_url.as_deref(),
        );
        let arc_prov: Arc<dyn Provider> = Arc::from(prov);
        {
            let mut lock = self.provider.write().await;
            *lock = Some(Arc::clone(&arc_prov));
        }
        arc_prov
    }

    /// Returns the number of registered workers in the pool
    pub fn worker_count(&self) -> usize {
        self.workers.try_read().map(|w| w.len()).unwrap_or(0)
    }

    /// Returns the number of currently active/running workers in the pool
    pub fn active_worker_count(&self) -> usize {
        if let Ok(workers) = self.workers.try_read() {
            let mut count = 0;
            for handle in workers.values() {
                if let Ok(info) = handle.info.try_read() {
                    if info.state.is_active() {
                        count += 1;
                    }
                }
            }
            count
        } else {
            0
        }
    }

    /// Returns a synchronous, non-blocking telemetry snapshot of all workers for TUI rendering
    pub fn snapshot_subagents(&self) -> Vec<SubagentInfo> {
        if let Ok(workers) = self.workers.try_read() {
            let mut list = Vec::new();
            for handle in workers.values() {
                if let Ok(info) = handle.info.try_read() {
                    list.push(info.clone());
                }
            }
            list.sort_by_key(|info| std::cmp::Reverse(info.started_at_secs));
            list
        } else {
            Vec::new()
        }
    }

    /// Generates a human-friendly subagent ID
    pub async fn next_id(&self, role: &SubagentRole) -> String {
        let mut count = self.counter.write().await;
        let id = format!("{}-{}", role.badge().to_lowercase(), *count);
        *count += 1;
        id
    }

    /// Resolves workspace isolation and cooperative file-boundary leases for a `minipub` worker.
    async fn resolve_worker_isolation(
        &self,
        id: &str,
        role: SubagentRole,
        requested_worktree: bool,
        file_boundaries: &[String],
    ) -> (PathBuf, bool) {
        let cfg = crate::config::Config::load(Some(&self.workspace_root), None).unwrap_or_default();
        let policy = cfg.agent.minipub.worktree_isolation.trim().to_lowercase();

        // Read-only roles (Scout, Architect, Planner, Vision, Reviewer, Security) never need a git worktree unless forced
        let is_mutating = role.is_mutating();
        let mut use_wt = match policy.as_str() {
            "never" => false,
            "always" => is_mutating || requested_worktree,
            "file_leases" => false,
            _ => requested_worktree,
        };

        // If mutating in shared workspace with file_boundaries, try acquiring cooperative lease;
        // if another active minipub holds an overlapping boundary, escalate to an isolated worktree!
        if !use_wt && is_mutating && !file_boundaries.is_empty() {
            if let Err(conflict) = crate::sandbox::worktree::FileLeaseManager::acquire_leases(
                &self.workspace_root,
                id,
                file_boundaries,
            ) {
                tracing::info!(
                    minipub_id = %id,
                    conflict = %conflict,
                    "File boundary lease conflict detected; escalating minipub to isolated git worktree"
                );
                use_wt = true;
            }
        }

        if use_wt {
            let worktree_mgr = crate::git::worktree::WorktreeManager::new(&self.workspace_root);
            match worktree_mgr.create_worktree(id).await {
                Ok(path) => (path, true),
                Err(e) => {
                    tracing::warn!(
                        minipub_id = %id,
                        error = %e,
                        "Failed to create worktree, falling back to main workspace"
                    );
                    (self.workspace_root.clone(), false)
                }
            }
        } else {
            (self.workspace_root.clone(), false)
        }
    }

    /// Spawns a `minipub` worker and executes it to completion synchronously with options
    pub async fn run_subagent_with_options(
        &self,
        role: SubagentRole,
        prompt: &str,
        custom_config: Option<SubagentConfig>,
        provider: Arc<dyn Provider>,
        isolate_worktree: bool,
    ) -> Result<SubagentResult> {
        let id = self.next_id(&role).await;
        let config = custom_config.unwrap_or_else(|| {
            let cfg =
                crate::config::Config::load(Some(&self.workspace_root), None).unwrap_or_default();
            SubagentConfig::for_role_with_minipub_config(role, &cfg.agent.minipub)
        });

        let (effective_root, actual_worktree) = self
            .resolve_worker_isolation(&id, role, isolate_worktree, &config.file_boundaries)
            .await;

        let worker = SubagentWorker::new(id.clone(), prompt.to_string(), config, &effective_root);
        {
            let mut info_guard = worker.info.write().await;
            info_guard.isolate_worktree = actual_worktree;
            info_guard.status_message = Some(format!(
                "Queued (waiting for minipub slot, max {})",
                self.max_parallel()
            ));
        }

        let info = Arc::clone(&worker.info);
        let cancel_flag = Arc::clone(&worker.cancel_flag);

        {
            let mut workers = self.workers.write().await;
            workers.insert(id.clone(), WorkerHandle { info, cancel_flag });
        }

        let cancel_flag_clone = Arc::clone(&worker.cancel_flag);
        let dev_registry = crate::dev::registry::get_global_dev_registry();
        let dev_id = crate::dev::models::DevProcessId::from(format!("worker-{}", id));
        let worker_name = format!(
            "MiniPub ({}) - {}",
            role.badge(),
            prompt.chars().take(40).collect::<String>()
        );
        let dev_handle = dev_registry
            .register_worker(
                dev_id,
                worker_name,
                format!("minipub::{} - {}", role.badge(), prompt),
                effective_root.clone(),
                std::process::id(),
                0,
                Some(Arc::new(move || {
                    cancel_flag_clone.store(true, Ordering::SeqCst);
                })),
            )
            .await;

        // Acquire concurrency permit from the pool's Semaphore (default: 4 parallel minipubs)
        let _permit = self.semaphore.clone().acquire_owned().await;
        {
            let mut info_guard = worker.info.write().await;
            info_guard.status_message = Some("Running".to_string());
        }

        dev_handle
            .append_log(format!("[{}] MiniPub worker running: {}", id, prompt))
            .await;
        let run_res = worker.run(provider).await;

        // Release any cooperative file-boundary leases
        crate::sandbox::worktree::FileLeaseManager::release_leases(&self.workspace_root, &id);
        self.completion_notify.notify_waiters();

        match &run_res {
            Ok(sub_res) => {
                dev_handle
                    .append_log(format!("[{}] Completed: {}", id, sub_res.final_summary))
                    .await;
                dev_handle
                    .update_status(crate::dev::models::DevProcessStatus::Exited(Some(0)))
                    .await;
                if !sub_res.success && actual_worktree {
                    let wt_mgr = crate::git::worktree::WorktreeManager::new(&self.workspace_root);
                    let _ = wt_mgr.remove_worktree(&id).await;
                }
            }
            Err(e) => {
                dev_handle
                    .append_log(format!("[{}] Execution error: {}", id, e))
                    .await;
                dev_handle
                    .update_status(crate::dev::models::DevProcessStatus::Degraded(
                        e.to_string(),
                    ))
                    .await;
                if actual_worktree {
                    let wt_mgr = crate::git::worktree::WorktreeManager::new(&self.workspace_root);
                    let _ = wt_mgr.remove_worktree(&id).await;
                }
            }
        }

        let mut res = run_res?;
        if actual_worktree && res.success {
            let agent_id = crate::agent::subagent::types::AgentId(id.clone());
            let _ = crate::sandbox::worktree::GitWorktreeManager::commit_uncommitted_changes(
                &effective_root,
                &agent_id,
            );
            res.worktree_branch = Some(
                crate::sandbox::worktree::GitWorktreeManager::resolve_branch_for(
                    &self.workspace_root,
                    &agent_id,
                ),
            );
        }
        Ok(res)
    }

    /// Registers an external `SubagentWorker` (e.g. from `FanoutOrchestrator`) into the pool
    /// so it is visible in `/tasks`, `minitask(action="workers")`, and supports live CRUD steering.
    pub async fn register_worker_handle(
        &self,
        id: String,
        info: Arc<RwLock<SubagentInfo>>,
        cancel_flag: Arc<AtomicBool>,
    ) {
        let mut workers = self.workers.write().await;
        workers.insert(id, WorkerHandle { info, cancel_flag });
    }

    /// Notifies any callers awaiting worker completion (`manage_subagents(action="await")`).
    pub fn notify_completion(&self) {
        self.completion_notify.notify_waiters();
    }

    /// Spawns a subagent worker and executes it to completion synchronously
    pub async fn run_subagent(
        &self,
        role: SubagentRole,
        prompt: &str,
        custom_config: Option<SubagentConfig>,
        provider: Arc<dyn Provider>,
    ) -> Result<SubagentResult> {
        self.run_subagent_with_options(role, prompt, custom_config, provider, false)
            .await
    }

    /// Spawns an autonomous background `minipub` worker without blocking caller,
    /// gated by the pool's `max_parallel` Semaphore.
    pub async fn spawn_background_worker(
        &self,
        role: SubagentRole,
        prompt: &str,
        custom_config: Option<SubagentConfig>,
        provider: Arc<dyn Provider>,
        isolate_worktree: bool,
    ) -> Result<String> {
        let id = self.next_id(&role).await;
        let config = custom_config.unwrap_or_else(|| {
            let cfg =
                crate::config::Config::load(Some(&self.workspace_root), None).unwrap_or_default();
            SubagentConfig::for_role_with_minipub_config(role, &cfg.agent.minipub)
        });

        let (effective_root, actual_worktree) = self
            .resolve_worker_isolation(&id, role, isolate_worktree, &config.file_boundaries)
            .await;

        let worker = SubagentWorker::new(id.clone(), prompt.to_string(), config, &effective_root);
        {
            let mut info_guard = worker.info.write().await;
            info_guard.isolate_worktree = actual_worktree;
            info_guard.status_message = Some("Spawned in background".to_string());
        }

        let info = Arc::clone(&worker.info);
        let cancel_flag = Arc::clone(&worker.cancel_flag);
        let info_for_merge = Arc::clone(&worker.info);

        {
            let mut workers = self.workers.write().await;
            workers.insert(id.clone(), WorkerHandle { info, cancel_flag });
        }

        let cancel_flag_clone = Arc::clone(&worker.cancel_flag);
        let dev_registry = crate::dev::registry::get_global_dev_registry();
        let dev_id = crate::dev::models::DevProcessId::from(format!("worker-{}", id));
        let worker_name = format!(
            "MiniPub ({}) - {}",
            role.badge(),
            prompt.chars().take(40).collect::<String>()
        );
        let dev_handle = dev_registry
            .register_worker(
                dev_id,
                worker_name,
                format!("minipub::{} - {}", role.badge(), prompt),
                effective_root.clone(),
                std::process::id(),
                0,
                Some(Arc::new(move || {
                    cancel_flag_clone.store(true, Ordering::SeqCst);
                })),
            )
            .await;

        let worker_id = id.clone();
        let worker_role = role;
        let prompt_log = prompt.to_string();
        let ws_root = self.workspace_root.clone();
        let sem = Arc::clone(&self.semaphore);
        let notify = Arc::clone(&self.completion_notify);

        tokio::spawn(async move {
            let _permit = sem.acquire_owned().await;
            dev_handle
                .append_log(format!(
                    "[{}] Background minipub started (role: {}): {}",
                    worker_id,
                    worker_role.badge(),
                    prompt_log
                ))
                .await;
            let res = worker.run(provider).await;
            crate::sandbox::worktree::FileLeaseManager::release_leases(&ws_root, &worker_id);

            let scratchpad = crate::agent::subagent::get_global_scratchpad();
            match res {
                Ok(mut sub_res) => {
                    if sub_res.success && actual_worktree {
                        let agent_id = crate::agent::subagent::types::AgentId(worker_id.clone());
                        match crate::sandbox::worktree::GitWorktreeManager::auto_commit_and_merge(
                            &ws_root, &agent_id,
                        ) {
                            Ok(Some(commit_hash)) => {
                                let merge_note = format!(
                                    "\n\n[Worktree Auto-Merged into workspace HEAD ({})]",
                                    commit_hash
                                );
                                sub_res.final_summary.push_str(&merge_note);
                                let mut guard = info_for_merge.write().await;
                                if let Some(ref mut summary) = guard.final_summary {
                                    summary.push_str(&merge_note);
                                }
                            }
                            Ok(None) => {}
                            Err(conflict_err) => {
                                let conflict_note =
                                    format!("\n\n[Worktree Auto-Merge Notice: {}]", conflict_err);
                                sub_res.final_summary.push_str(&conflict_note);
                                let mut guard = info_for_merge.write().await;
                                if let Some(ref mut summary) = guard.final_summary {
                                    summary.push_str(&conflict_note);
                                }
                            }
                        }
                    } else if !sub_res.success && actual_worktree {
                        let wt_mgr = crate::git::worktree::WorktreeManager::new(&ws_root);
                        let _ = wt_mgr.remove_worktree(&worker_id).await;
                    }

                    let title = format!("MiniPub {} ({}) Report", worker_id, worker_role.badge());
                    scratchpad.write_entry(
                        &format!("subagent_{}", worker_id),
                        &title,
                        &sub_res.final_summary,
                        &worker_id,
                    );
                    scratchpad.write_entry(
                        &format!("subagent/{}", worker_id),
                        &title,
                        &sub_res.final_summary,
                        &worker_id,
                    );
                    scratchpad.write_entry(
                        &format!("minipub/{}", worker_id),
                        &title,
                        &sub_res.final_summary,
                        &worker_id,
                    );
                    let _ = scratchpad.save_to_disk(&ws_root);
                    tracing::info!(minipub_id = %worker_id, "Background minipub completed");

                    dev_handle
                        .append_log(format!(
                            "[{}] MiniPub completed (tokens: {}, turns: {})",
                            worker_id, sub_res.tokens_used, sub_res.turns_executed
                        ))
                        .await;
                    dev_handle
                        .append_log(format!(
                            "[{}] Summary: {}",
                            worker_id, sub_res.final_summary
                        ))
                        .await;
                    dev_handle
                        .update_status(crate::dev::models::DevProcessStatus::Exited(Some(0)))
                        .await;
                }
                Err(e) => {
                    tracing::error!(minipub_id = %worker_id, error = %e, "Background minipub failed");
                    let title = format!("MiniPub {} ({}) Failure", worker_id, worker_role.badge());
                    scratchpad.write_entry(
                        &format!("subagent_{}", worker_id),
                        &title,
                        &format!("Execution error: {}", e),
                        &worker_id,
                    );
                    let _ = scratchpad.save_to_disk(&ws_root);

                    dev_handle
                        .append_log(format!("[{}] MiniPub execution error: {}", worker_id, e))
                        .await;
                    dev_handle
                        .update_status(crate::dev::models::DevProcessStatus::Degraded(
                            e.to_string(),
                        ))
                        .await;
                    if actual_worktree {
                        let wt_mgr = crate::git::worktree::WorktreeManager::new(&ws_root);
                        let _ = wt_mgr.remove_worktree(&worker_id).await;
                    }
                }
            }
            notify.notify_waiters();
        });

        Ok(id)
    }

    /// Lists live telemetry info across all registered subagents
    pub async fn list_subagents(&self) -> Vec<SubagentInfo> {
        let workers = self.workers.read().await;
        let mut list = Vec::new();
        for handle in workers.values() {
            let info = handle.info.read().await;
            list.push(info.clone());
        }
        list.sort_by_key(|info| std::cmp::Reverse(info.started_at_secs));
        list
    }

    /// Retrieves status for a single subagent by ID
    pub async fn get_subagent(&self, id: &str) -> Option<SubagentInfo> {
        let workers = self.workers.read().await;
        if let Some(handle) = workers.get(id) {
            let info = handle.info.read().await;
            Some(info.clone())
        } else {
            None
        }
    }

    /// Dynamically modifies a running or queued `minipub` worker in-flight (Orchestrator CRUD Update):
    /// updates timeouts, token/turn budgets, todos, cooperative file boundaries, and injects live steering prompts.
    pub async fn update_subagent(&self, id: &str, update: MiniPubUpdate) -> Result<SubagentInfo> {
        let updated_info = {
            let workers = self.workers.read().await;
            let Some(handle) = workers.get(id) else {
                return Err(MinicodeError::Channel(format!(
                    "MiniPub subagent '{}' not found",
                    id
                )));
            };

            let mut info = handle.info.write().await;

            if let Some(to) = update.inactivity_timeout_secs {
                info.inactivity_timeout_secs = to.clamp(15, 7200);
            }
            if let Some(tb) = update.token_budget {
                info.token_budget = tb.max(4096);
            }
            if let Some(mt) = update.max_turns {
                info.max_turns = mt.clamp(1, 200);
            }
            if let Some(new_todos) = update.todos {
                info.todos = MiniPubTodoItem::from_strings(&new_todos);
            }
            if !update.append_todos.is_empty() {
                let mut next_id = info.todos.len() + 1;
                for item in update.append_todos {
                    let trimmed = item.trim();
                    if !trimmed.is_empty() {
                        info.todos.push(MiniPubTodoItem {
                            id: next_id,
                            task: trimmed.to_string(),
                            completed: false,
                        });
                        next_id += 1;
                    }
                }
            }
            for done_id in update.complete_todo_ids {
                if let Some(t) = info.todos.iter_mut().find(|t| t.id == done_id) {
                    t.completed = true;
                }
            }
            if let Some(new_boundaries) = update.file_boundaries {
                let cleaned: Vec<String> = new_boundaries
                    .into_iter()
                    .map(|b| b.trim().to_string())
                    .filter(|b| !b.is_empty())
                    .collect();
                crate::sandbox::worktree::FileLeaseManager::release_leases(
                    &self.workspace_root,
                    id,
                );
                if !info.isolate_worktree && !cleaned.is_empty() {
                    let _ = crate::sandbox::worktree::FileLeaseManager::acquire_leases(
                        &self.workspace_root,
                        id,
                        &cleaned,
                    );
                }
                info.file_boundaries = cleaned;
            }
            if let Some(steer) = update.steer_prompt {
                let trimmed = steer.trim();
                if !trimmed.is_empty() {
                    info.pending_steer_messages.push(trimmed.to_string());
                    info.status_message = Some(format!(
                        "Updated by Orchestrator: {}",
                        crate::utils::truncate_ellipsis(trimmed, 42)
                    ));
                }
            }
            let mut model_changed = false;
            if let Some(use_orch) = update.use_orchestrator_model {
                info.use_orchestrator_model = use_orch;
                if use_orch {
                    info.provider_override = None;
                    info.model_override = None;
                    info.fallback_1_override = None;
                    info.fallback_2_override = None;
                    info.active_model_label = Some("orchestrator-chain".to_string());
                    info.status_message =
                        Some("Switched to Orchestrator model & fallback chain".to_string());
                }
                model_changed = true;
            }
            if let Some(prov) = update.provider {
                let trimmed = prov.trim();
                if !trimmed.is_empty() {
                    info.provider_override = Some(trimmed.to_lowercase());
                    info.use_orchestrator_model = false;
                    model_changed = true;
                }
            }
            if let Some(mdl) = update.model {
                let trimmed = mdl.trim();
                if !trimmed.is_empty() {
                    if let Some((p, m)) = trimmed.split_once(':') {
                        info.provider_override = Some(p.trim().to_lowercase());
                        info.model_override = Some(m.trim().to_string());
                    } else {
                        info.model_override = Some(trimmed.to_string());
                    }
                    info.use_orchestrator_model = false;
                    model_changed = true;
                }
            }
            if let Some(f1) = update.fallback_1 {
                info.fallback_1_override = f1.is_valid().then_some(f1);
                model_changed = true;
            }
            if let Some(f2) = update.fallback_2 {
                info.fallback_2_override = f2.is_valid().then_some(f2);
                model_changed = true;
            }
            if model_changed {
                info.model_generation = info.model_generation.saturating_add(1);
                if !info.use_orchestrator_model {
                    let lbl = match (&info.provider_override, &info.model_override) {
                        (Some(p), Some(m)) => format!("{}:{}", p, m),
                        (None, Some(m)) => m.clone(),
                        (Some(p), None) => format!("{}:default", p),
                        (None, None) => "default-chain".to_string(),
                    };
                    info.active_model_label = Some(lbl.clone());
                    info.status_message = Some(format!("Hot-swapped model -> {}", lbl));
                }
            }

            info.clone()
        };

        self.completion_notify.notify_waiters();
        Ok(updated_info)
    }

    /// Cancels a running `minipub` worker by ID and cleans up any leases or ephemeral worktrees
    pub async fn kill_subagent(&self, id: &str) -> Result<()> {
        let isolate_wt = {
            let workers = self.workers.read().await;
            if let Some(handle) = workers.get(id) {
                handle.cancel_flag.store(true, Ordering::SeqCst);
                let mut info = handle.info.write().await;
                info.state = SubagentState::Terminated;
                info.current_tool = None;
                info.status_message = Some("Canceled by user".to_string());
                info.isolate_worktree
            } else {
                return Err(MinicodeError::Channel(format!(
                    "Subagent '{}' not found",
                    id
                )));
            }
        };

        crate::sandbox::worktree::FileLeaseManager::release_leases(&self.workspace_root, id);
        if isolate_wt {
            let wt_mgr = crate::git::worktree::WorktreeManager::new(&self.workspace_root);
            let _ = wt_mgr.remove_worktree(id).await;
        }
        self.completion_notify.notify_waiters();
        Ok(())
    }

    /// Cancels all currently running `minipub` subagents and cleans up their leases
    pub async fn kill_all(&self) {
        let mut wt_ids = Vec::new();
        {
            let workers = self.workers.read().await;
            for (id, handle) in workers.iter() {
                handle.cancel_flag.store(true, Ordering::SeqCst);
                crate::sandbox::worktree::FileLeaseManager::release_leases(
                    &self.workspace_root,
                    id,
                );
                if let Ok(mut info) = handle.info.try_write() {
                    info.state = SubagentState::Terminated;
                    info.current_tool = None;
                    info.status_message = Some("Canceled by user".to_string());
                    if info.isolate_worktree {
                        wt_ids.push(id.clone());
                    }
                }
            }
        }
        for id in wt_ids {
            let wt_mgr = crate::git::worktree::WorktreeManager::new(&self.workspace_root);
            let _ = wt_mgr.remove_worktree(&id).await;
        }
        self.completion_notify.notify_waiters();
    }

    /// Formats a clean Markdown summary report of all `minipub` subagents in the pool
    pub async fn format_swarm_summary(&self) -> String {
        let list = self.list_subagents().await;
        if list.is_empty() {
            return "No minipub subagent workers have been spawned in this session.".to_string();
        }

        let mut out = format!(
            "### MiniPub Subagent Swarm Status (Max Parallel: {})\n\n",
            self.max_parallel()
        );
        out.push_str(
            "| ID | Role | Status | Active Tool / Detail | Todos | Turns | Tokens Used | Task Prompt |\n",
        );
        out.push_str("| :--- | :--- | :--- | :--- | :---: | :---: | :---: | :--- |\n");

        for item in list {
            let status_badge = match &item.state {
                SubagentState::Starting => "○ Starting",
                SubagentState::Running => "◉ Running",
                SubagentState::WaitingForInput => "◷ Waiting",
                SubagentState::Completed => "✔ Done",
                SubagentState::Failed(_) => "✗ Failed",
                SubagentState::Terminated => "⊘ Terminated",
            };

            let detail = if let Some(ref t) = item.current_tool {
                format!("⚡ `{}`", t)
            } else if let Some(ref s) = item.status_message {
                s.clone()
            } else {
                "-".to_string()
            };
            let todo_progress = if item.todos.is_empty() {
                "-".to_string()
            } else {
                let done = item.todos.iter().filter(|t| t.completed).count();
                format!("{}/{}", done, item.todos.len())
            };
            let short_prompt = crate::utils::truncate_ellipsis(&item.prompt, 40);
            out.push_str(&format!(
                "| `{}` | **{}** | {} | {} | {} | {} | {} | {} |\n",
                item.id,
                item.role.badge(),
                status_badge,
                detail,
                todo_progress,
                item.turns_executed,
                item.tokens_used,
                short_prompt
            ));
        }

        out
    }

    /// Builds a reactive `<active_minipub_workers>` context block for the Orchestrator:
    /// includes live running `MiniPub` workers AND newly completed/failed workers (pushed once via `reported_completions`).
    pub async fn format_orchestrator_context_block(&self) -> Option<String> {
        let list = self.list_subagents().await;
        if list.is_empty() {
            return None;
        }

        let mut active_items = Vec::new();
        let mut newly_finished = Vec::new();
        {
            let mut reported = self.reported_completions.write().await;
            for item in list {
                if item.state.is_active() {
                    active_items.push(item);
                } else if matches!(
                    item.state,
                    SubagentState::Completed | SubagentState::Failed(_)
                ) && !reported.contains(&item.id)
                {
                    reported.insert(item.id.clone());
                    newly_finished.push(item);
                }
            }
        }

        if active_items.is_empty() && newly_finished.is_empty() {
            return None;
        }

        let mut out = format!(
            "<active_minipub_workers max_parallel=\"{}\" stream_idle_timeout_secs=\"{}\">\n",
            self.max_parallel(),
            crate::config::get_active_stream_idle_timeout_secs()
        );

        if !active_items.is_empty() {
            out.push_str("  Active MiniPub Workers in Flight:\n");
            for w in &active_items {
                let todo_str = if w.todos.is_empty() {
                    String::new()
                } else {
                    let done = w.todos.iter().filter(|t| t.completed).count();
                    format!(" | Todos: {}/{}", done, w.todos.len())
                };
                let bounds_str = if w.file_boundaries.is_empty() {
                    String::new()
                } else {
                    format!(" | Boundaries: {}", w.file_boundaries.join(", "))
                };
                out.push_str(&format!(
                    "  • `[{}]` ({}) — {:?} | Turns: {}/{} | Tokens: {}/{} | Timeout: {}s (Idle: {}s){}{}\n",
                    w.id,
                    w.role.badge(),
                    w.state,
                    w.turns_executed,
                    w.max_turns,
                    w.tokens_used,
                    w.token_budget,
                    w.inactivity_timeout_secs,
                    w.last_activity_secs_ago(),
                    todo_str,
                    bounds_str
                ));
            }
            out.push_str("  Orchestrator Control: Use `minitask(action=\"update_minipub\", id=\"...\", timeout_secs=..., steer_prompt=\"...\", append_todos=[...])` to steer/adjust in-flight, `manage_subagents(action=\"await\", subagent_id=\"...\")` to wait, or `minitask(action=\"kill\", id=\"...\")` to terminate.\n");
        }

        if !newly_finished.is_empty() {
            out.push_str("  Reactive Boomerang Completion Notifications:\n");
            for w in &newly_finished {
                let status_label = match &w.state {
                    SubagentState::Completed => "COMPLETED ✔",
                    SubagentState::Failed(_) => "FAILED ✖",
                    _ => "FINISHED",
                };
                let todo_str = if w.todos.is_empty() {
                    String::new()
                } else {
                    let done = w.todos.iter().filter(|t| t.completed).count();
                    format!(" | Todos Completed: {}/{}", done, w.todos.len())
                };
                out.push_str(&format!(
                    "  • `[{}]` ({}) — {} (Turns: {}, Tokens: {}{})\n",
                    w.id,
                    w.role.badge(),
                    status_label,
                    w.turns_executed,
                    w.tokens_used,
                    todo_str
                ));
                if let Some(ref summary) = w.final_summary {
                    let excerpt = crate::utils::strings::truncate_chars(summary.trim(), 600);
                    for line in excerpt.lines().take(12) {
                        out.push_str(&format!("    │ {}\n", line));
                    }
                }
            }
        }

        out.push_str("</active_minipub_workers>");
        Some(out)
    }

    /// Waits for one or more `MiniPub` workers to complete.
    /// - `target_ids`: Specific worker IDs to watch. If empty, watches all currently active workers (or all pool workers if already finished).
    /// - `wait_mode`: `"any"` / `"first"` / `"race"` wakes as soon as the first watched worker reaches a terminal state; `"all"` (default) waits until all watched workers finish.
    pub async fn await_workers(
        &self,
        target_ids: &[String],
        wait_mode: &str,
        timeout_secs: u64,
    ) -> String {
        let normalized_ids: Vec<String> = if target_ids.is_empty() {
            let all = self.list_subagents().await;
            let active: Vec<String> = all
                .iter()
                .filter(|w| w.state.is_active())
                .map(|w| w.id.clone())
                .collect();
            if active.is_empty() {
                all.into_iter().map(|w| w.id).collect()
            } else {
                active
            }
        } else {
            target_ids
                .iter()
                .map(|s| {
                    s.trim()
                        .strip_prefix("worker-")
                        .unwrap_or(s.trim())
                        .to_string()
                })
                .filter(|s| !s.is_empty())
                .collect()
        };

        if normalized_ids.is_empty() {
            return "ℹ No active or completed MiniPub subagents found in the pool.".to_string();
        }

        // Single-worker fast path preserving existing output format
        if normalized_ids.len() == 1 {
            let id = &normalized_ids[0];
            if self.get_subagent(id).await.is_none() {
                return format!("ℹ No subagent found with ID '{}'", id);
            }
        }

        let is_any_mode = matches!(
            wait_mode.trim().to_ascii_lowercase().as_str(),
            "any" | "first" | "race"
        );
        let start = std::time::Instant::now();
        let max_dur = std::time::Duration::from_secs(timeout_secs.max(1));

        loop {
            let mut watched_infos = Vec::new();
            for id in &normalized_ids {
                if let Some(info) = self.get_subagent(id).await {
                    watched_infos.push(info);
                }
            }

            if watched_infos.is_empty() {
                return format!(
                    "ℹ No MiniPub subagents found matching IDs: {}",
                    normalized_ids.join(", ")
                );
            }

            let finished_count = watched_infos
                .iter()
                .filter(|w| !w.state.is_active())
                .count();
            let all_finished = finished_count == watched_infos.len();

            if all_finished || (is_any_mode && finished_count > 0) || start.elapsed() >= max_dur {
                break;
            }

            self.wait_for_change(std::time::Duration::from_millis(200))
                .await;
        }

        let mut watched_infos = Vec::new();
        for id in &normalized_ids {
            if let Some(info) = self.get_subagent(id).await {
                watched_infos.push(info);
            }
        }

        let scratchpad = crate::agent::subagent::get_global_scratchpad();

        if watched_infos.len() == 1 {
            let info = &watched_infos[0];
            if !info.state.is_active() {
                let scratchpad_content = scratchpad
                    .read_entry(&format!("subagent_{}", info.id))
                    .or_else(|| scratchpad.read_entry(&format!("subagent/{}", info.id)))
                    .or_else(|| scratchpad.read_entry(&format!("minipub/{}", info.id)))
                    .map(|e| e.content)
                    .or_else(|| info.final_summary.clone())
                    .unwrap_or_else(|| "No output report recorded.".to_string());

                if scratchpad_content.starts_with("### ") {
                    return scratchpad_content;
                }
                return format!(
                    "✔ MiniPub Subagent `{}` has finished execution (State: {:?})!\n\
                     • **Role**: {}\n\
                     • **Turns Executed**: {}\n\
                     • **Tokens Used**: {}\n\
                     • **Isolation**: {}\n\n\
                     ### Findings & Outcome Summary\n\
                     {}",
                    info.id,
                    info.state,
                    info.role.badge(),
                    info.turns_executed,
                    info.tokens_used,
                    if info.isolate_worktree {
                        "Git Worktree"
                    } else {
                        "Read-Only / Main"
                    },
                    scratchpad_content
                );
            } else {
                let detail = info
                    .status_message
                    .clone()
                    .unwrap_or_else(|| "Running".to_string());
                return format!(
                    "⏳ MiniPub Subagent `{}` is still executing after {}s wait window.\n• Current Status: {}\n• Use `minitask(action=\"await\", id=\"{}\")` or `manage_subagents(action=\"await\", subagent_id=\"{}\")` to continue waiting.",
                    info.id, timeout_secs, detail, info.id, info.id
                );
            }
        }

        // Multi-worker summary (wait_mode="any" or "all")
        let finished: Vec<&SubagentInfo> = watched_infos
            .iter()
            .filter(|w| !w.state.is_active())
            .collect();
        let active: Vec<&SubagentInfo> = watched_infos
            .iter()
            .filter(|w| w.state.is_active())
            .collect();

        let mut out = format!(
            "### MiniPub Await Summary (`wait_mode={}`, Completed: {}/{}, Still Active: {})\n\n",
            if is_any_mode { "any" } else { "all" },
            finished.len(),
            watched_infos.len(),
            active.len()
        );

        for info in &finished {
            let done_todos = info.todos.iter().filter(|t| t.completed).count();
            let todo_str = if info.todos.is_empty() {
                String::new()
            } else {
                format!(" | Todos: {}/{}", done_todos, info.todos.len())
            };
            out.push_str(&format!(
                "#### ✔ `{}` ({}) — {:?} (Turns: {}, Tokens: {}{})\n",
                info.id,
                info.role.badge(),
                info.state,
                info.turns_executed,
                info.tokens_used,
                todo_str
            ));
            let report = scratchpad
                .read_entry(&format!("subagent_{}", info.id))
                .or_else(|| scratchpad.read_entry(&format!("subagent/{}", info.id)))
                .or_else(|| scratchpad.read_entry(&format!("minipub/{}", info.id)))
                .map(|e| e.content)
                .or_else(|| info.final_summary.clone())
                .unwrap_or_else(|| "No output report recorded.".to_string());
            out.push_str(&report);
            out.push_str("\n\n");
        }

        if !active.is_empty() {
            out.push_str("#### ⏳ Still Running MiniPub Workers:\n");
            for info in &active {
                let done_todos = info.todos.iter().filter(|t| t.completed).count();
                let todo_str = if info.todos.is_empty() {
                    String::new()
                } else {
                    format!(" | Todos: {}/{}", done_todos, info.todos.len())
                };
                out.push_str(&format!(
                    "• `{}` ({}) — {:?} | Turns: {}/{} | Tokens: {}/{}{}\n",
                    info.id,
                    info.role.badge(),
                    info.state,
                    info.turns_executed,
                    info.max_turns,
                    info.tokens_used,
                    info.token_budget,
                    todo_str
                ));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::mock_provider::MockProvider;

    #[tokio::test]
    async fn test_subagent_pool_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pool = SubagentPool::new(temp_dir.path());

        assert_eq!(pool.worker_count(), 0);
        assert!(pool.snapshot_subagents().is_empty());

        let id1 = pool.next_id(&SubagentRole::Scout).await;
        assert_eq!(id1, "scout-1");

        let id2 = pool.next_id(&SubagentRole::Tester).await;
        assert_eq!(id2, "tester-2");
    }

    #[tokio::test]
    async fn test_spawn_background_worker_and_snapshot() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pool = SubagentPool::new(temp_dir.path());

        let mock = Arc::new(MockProvider::new("mock", "mock-model"));
        mock.push_response(&["Subagent analysis completed."], vec![]);
        pool.set_provider(mock.clone());

        let id = pool
            .spawn_background_worker(
                SubagentRole::Scout,
                "Analyze codebase architecture",
                None,
                mock,
                false,
            )
            .await
            .unwrap();

        assert_eq!(id, "scout-1");
        assert_eq!(pool.worker_count(), 1);

        let snapshots = pool.snapshot_subagents();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, "scout-1");
        assert_eq!(snapshots[0].role, SubagentRole::Scout);

        // Wait briefly for background execution
        for _ in 0..20 {
            if let Some(info) = pool.get_subagent(&id).await {
                if matches!(info.state, SubagentState::Completed) {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let summary = pool.format_swarm_summary().await;
        assert!(summary.contains("scout-1"));
        assert!(summary.contains("Scout"));
    }

    #[tokio::test]
    async fn test_kill_subagent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pool = SubagentPool::new(temp_dir.path());

        let mock = Arc::new(MockProvider::new("mock", "mock-model"));
        // No responses queued so it would wait or finish
        let id = pool
            .spawn_background_worker(
                SubagentRole::Reviewer,
                "Review code changes",
                None,
                mock,
                false,
            )
            .await
            .unwrap();

        let kill_res = pool.kill_subagent(&id).await;
        assert!(kill_res.is_ok());

        let info = pool.get_subagent(&id).await.unwrap();
        assert_eq!(info.state, SubagentState::Terminated);
        assert_eq!(info.status_message.as_deref(), Some("Canceled by user"));
    }

    #[test]
    fn test_minipub_todos_file_boundaries_and_leases() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pool = SubagentPool::new(temp_dir.path());
        assert_eq!(
            pool.max_parallel(),
            crate::constants::DEFAULT_MAX_PARALLEL_MINIPUBS
        );
        assert_eq!(pool.set_max_parallel(8), 8);
        assert_eq!(pool.max_parallel(), 8);

        let mp_cfg = crate::config::MiniPubConfig::default();
        assert_eq!(mp_cfg.max_parallel, 4);
        assert_eq!(mp_cfg.inactivity_timeout_secs, 600);
        assert_eq!(mp_cfg.default_token_budget, 50_000);
        assert_eq!(mp_cfg.default_max_turns, 15);

        let sub_cfg = SubagentConfig::for_role_with_minipub_config(SubagentRole::Coder, &mp_cfg)
            .with_todos(vec![
                "Create src/auth.rs".to_string(),
                "Run cargo check".to_string(),
            ])
            .with_file_boundaries(vec!["src/auth.rs".to_string(), "src/types/".to_string()]);

        assert_eq!(sub_cfg.todos.len(), 2);
        assert_eq!(sub_cfg.file_boundaries.len(), 2);
        assert!(sub_cfg.tool_whitelist.contains("kit_skill_show"));
        assert!(sub_cfg.tool_whitelist.contains("block_search"));

        let worker = SubagentWorker::new(
            "coder-1".to_string(),
            "Implement auth module".to_string(),
            sub_cfg.clone(),
            temp_dir.path(),
        );
        let sys_prompt = worker.build_system_prompt();
        assert!(sys_prompt.contains("<assigned_minipub_todos>"));
        assert!(sys_prompt.contains("1. Create src/auth.rs"));
        assert!(sys_prompt.contains("<assigned_file_boundaries>"));
        assert!(sys_prompt.contains("`src/auth.rs`"));

        // Verify FileLeaseManager lease acquisition, conflict detection, and boundary checks
        assert!(crate::sandbox::worktree::FileLeaseManager::acquire_leases(
            temp_dir.path(),
            "coder-1",
            &sub_cfg.file_boundaries
        )
        .is_ok());

        // Overlapping boundary from another minipub must be rejected
        assert!(crate::sandbox::worktree::FileLeaseManager::acquire_leases(
            temp_dir.path(),
            "coder-2",
            &["src/types/user.rs".to_string()]
        )
        .is_err());

        // Disjoint boundary from another minipub succeeds
        assert!(crate::sandbox::worktree::FileLeaseManager::acquire_leases(
            temp_dir.path(),
            "coder-2",
            &["src/ui/".to_string()]
        )
        .is_ok());

        // Verify write boundary check
        assert!(
            crate::sandbox::worktree::FileLeaseManager::verify_write_allowed(
                &sub_cfg.file_boundaries,
                temp_dir.path(),
                "src/auth.rs"
            )
            .is_ok()
        );
        assert!(
            crate::sandbox::worktree::FileLeaseManager::verify_write_allowed(
                &sub_cfg.file_boundaries,
                temp_dir.path(),
                "src/main.rs"
            )
            .is_err()
        );

        crate::sandbox::worktree::FileLeaseManager::release_leases(temp_dir.path(), "coder-1");
        crate::sandbox::worktree::FileLeaseManager::release_leases(temp_dir.path(), "coder-2");
    }

    #[tokio::test]
    async fn test_update_minipub_in_flight_crud() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pool = SubagentPool::new(temp_dir.path());
        let mock = Arc::new(MockProvider::new("mock", "mock-model"));

        let cfg = SubagentConfig::for_role(SubagentRole::Coder)
            .with_todos(vec!["Step 1".to_string(), "Step 2".to_string()]);
        let id = pool
            .spawn_background_worker(
                SubagentRole::Coder,
                "Implement feature",
                Some(cfg),
                mock,
                false,
            )
            .await
            .unwrap();

        let updated = pool
            .update_subagent(
                &id,
                crate::agent::subagent::MiniPubUpdate {
                    inactivity_timeout_secs: Some(1200),
                    token_budget: Some(90_000),
                    max_turns: Some(30),
                    todos: None,
                    append_todos: vec!["Step 3".to_string()],
                    complete_todo_ids: vec![1],
                    file_boundaries: Some(vec!["src/feature.rs".to_string()]),
                    steer_prompt: Some("Use pure functions in src/feature.rs".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.inactivity_timeout_secs, 1200);
        assert_eq!(updated.token_budget, 90_000);
        assert_eq!(updated.max_turns, 30);
        assert_eq!(updated.todos.len(), 3);
        assert!(updated.todos[0].completed);
        assert!(!updated.todos[1].completed);
        assert_eq!(updated.todos[2].task, "Step 3");
        assert_eq!(updated.file_boundaries, vec!["src/feature.rs".to_string()]);

        let _ = pool.kill_subagent(&id).await;
    }

    #[tokio::test]
    async fn test_await_workers_any_and_all() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pool = SubagentPool::new(temp_dir.path());
        let mock = Arc::new(MockProvider::new("mock", "mock-model"));
        mock.push_response(&["Checked off:\n- [x] 1. First task done"], vec![]);
        mock.push_response(&["Checked off:\n- [x] 1. Second task done"], vec![]);

        let cfg1 = SubagentConfig::for_role(SubagentRole::Scout)
            .with_todos(vec!["First task done".to_string()]);
        let cfg2 = SubagentConfig::for_role(SubagentRole::Scout)
            .with_todos(vec!["Second task done".to_string()]);

        let id1 = pool
            .spawn_background_worker(
                SubagentRole::Scout,
                "First scout",
                Some(cfg1),
                mock.clone(),
                false,
            )
            .await
            .unwrap();
        let id2 = pool
            .spawn_background_worker(SubagentRole::Scout, "Second scout", Some(cfg2), mock, false)
            .await
            .unwrap();

        let any_summary = pool
            .await_workers(&[id1.clone(), id2.clone()], "any", 5)
            .await;
        assert!(any_summary.contains("MiniPub Await Summary (`wait_mode=any`"));

        let all_summary = pool.await_workers(&[], "all", 5).await;
        assert!(all_summary.contains("MiniPub Await Summary (`wait_mode=all`, Completed: 2/2"));
        assert!(all_summary.contains(&id1));
        assert!(all_summary.contains(&id2));
    }

    #[test]
    fn test_all_14_omc_minipub_roles_and_markdown_overrides() {
        let all_roles = SubagentRole::all_roles();
        assert_eq!(all_roles.len(), 14);

        let temp_dir = tempfile::tempdir().unwrap();

        for role in all_roles {
            let slug = role.as_str();
            assert!(!slug.is_empty());
            assert_eq!(SubagentRole::from_str_loose(slug), *role);

            let spec = role.default_markdown_spec();
            assert!(
                spec.contains("<Agent_Prompt>"),
                "Role {:?} markdown spec must contain <Agent_Prompt>",
                role
            );
            assert!(
                spec.contains("<Role>"),
                "Role {:?} markdown spec must contain <Role>",
                role
            );

            let loaded = role.load_role_prompt(Some(temp_dir.path()));
            assert!(
                !loaded.starts_with("---"),
                "YAML frontmatter should be stripped for {:?}",
                role
            );
            assert!(loaded.contains("<Agent_Prompt>"));

            let cfg = SubagentConfig::for_role(*role);
            assert!(!cfg.tool_whitelist.is_empty());
        }

        // Verify key aliases from oh-my-claudecode (OMC)
        assert_eq!(SubagentRole::from_str_loose("explore"), SubagentRole::Scout);
        assert_eq!(
            SubagentRole::from_str_loose("oracle"),
            SubagentRole::Architect
        );
        assert_eq!(
            SubagentRole::from_str_loose("prometheus"),
            SubagentRole::Planner
        );
        assert_eq!(
            SubagentRole::from_str_loose("ui_ux"),
            SubagentRole::Designer
        );
        assert_eq!(
            SubagentRole::from_str_loose("frontend_specialist"),
            SubagentRole::Frontend
        );
        assert_eq!(
            SubagentRole::from_str_loose("backend_specialist"),
            SubagentRole::Backend
        );
        assert_eq!(
            SubagentRole::from_str_loose("executor"),
            SubagentRole::Coder
        );
        assert_eq!(
            SubagentRole::from_str_loose("multimodal"),
            SubagentRole::Vision
        );
        assert_eq!(
            SubagentRole::from_str_loose("writer"),
            SubagentRole::ContentWriter
        );
        assert_eq!(
            SubagentRole::from_str_loose("code_simplifier"),
            SubagentRole::Simplifier
        );

        // Verify read-only vs mutating classification
        assert!(!SubagentRole::Scout.is_mutating());
        assert!(!SubagentRole::Architect.is_mutating());
        assert!(!SubagentRole::Planner.is_mutating());
        assert!(!SubagentRole::Vision.is_mutating());
        assert!(!SubagentRole::Reviewer.is_mutating());
        assert!(!SubagentRole::Security.is_mutating());

        assert!(SubagentRole::Designer.is_mutating());
        assert!(SubagentRole::Frontend.is_mutating());
        assert!(SubagentRole::Backend.is_mutating());
        assert!(SubagentRole::Coder.is_mutating());
        assert!(SubagentRole::ContentWriter.is_mutating());
        assert!(SubagentRole::Debugger.is_mutating());
        assert!(SubagentRole::Tester.is_mutating());
        assert!(SubagentRole::Simplifier.is_mutating());

        // Verify .minicode/agents/<role>.md custom workspace override
        let custom_agents_dir = temp_dir.path().join(".minicode").join("agents");
        std::fs::create_dir_all(&custom_agents_dir).unwrap();
        std::fs::write(
            custom_agents_dir.join("vision.md"),
            "---\nname: vision\n---\n<Agent_Prompt>\nCustom workspace Vision MiniPub rules.\n</Agent_Prompt>",
        )
        .unwrap();

        let overridden = SubagentRole::Vision.load_role_prompt(Some(temp_dir.path()));
        assert!(overridden.contains("Custom workspace Vision MiniPub rules."));
    }

    #[test]
    fn test_vision_image_attachment_loading() {
        let temp_dir = tempfile::tempdir().unwrap();
        let img_path = temp_dir.path().join("mockup.png");
        // Valid 1x1 PNG header + bytes
        let png_bytes: [u8; 32] = [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x05, 0x00, // width = 1280
            0x00, 0x00, 0x02, 0xD0, // height = 720
            0x08, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        std::fs::write(&img_path, png_bytes).unwrap();

        let att = crate::agent::types::ImageAttachment::from_path(&img_path).unwrap();
        assert_eq!(att.media_type, "image/png");
        assert!(!att.data_base64.is_empty());
        assert!(att.data_url().starts_with("data:image/png;base64,"));
    }
}
