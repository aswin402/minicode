//! Global thread-safe registry managing development processes, servers, Docker stacks, and browsers.
//!
//! Modularized into focused domain submodules under `src/dev/registry_impl/`:
//! - `tasks_and_logs.rs` — background task await barriers, completion harvesting, and multi-source log retrieval
//! - `lifecycle_and_telemetry.rs` — process lifecycle control (`stop`, `restart`, `kill_all`, `schedule`), worker/swarm registration, telemetry synthesis, and `kill_all_sync`
//! - `tests.rs` — unit tests

#[path = "registry_impl/lifecycle_and_telemetry.rs"]
mod lifecycle_and_telemetry;
#[path = "registry_impl/tasks_and_logs.rs"]
mod tasks_and_logs;
#[cfg(test)]
#[path = "registry_impl/tests.rs"]
mod tests;

pub use self::lifecycle_and_telemetry::kill_all_sync;

use crate::dev::metrics::sample_process_metrics;
use crate::dev::models::{
    DevProcessId, DevProcessStatus, DevProcessSummary, DevProcessType, ScheduleInfo,
    SpawnDevRequest,
};
use crate::dev::process::DevProcessHandle;
use crate::error::{DevError, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

static GLOBAL_DEV_REGISTRY: OnceLock<Arc<MiniDevRegistry>> = OnceLock::new();

#[inline]
fn decrement_atomic_saturating(atomic: &AtomicU32) {
    let mut current = atomic.load(Ordering::Relaxed);
    while current > 0 {
        match atomic.compare_exchange_weak(
            current,
            current - 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}

/// Returns the global MiniDevRegistry singleton.
pub fn get_global_dev_registry() -> &'static Arc<MiniDevRegistry> {
    GLOBAL_DEV_REGISTRY.get_or_init(|| Arc::new(MiniDevRegistry::new()))
}

/// Thread-safe registry coordinating all running daemons and background tasks.
pub struct MiniDevRegistry {
    processes: Arc<RwLock<HashMap<DevProcessId, Arc<DevProcessHandle>>>>,
    docker_containers: Arc<RwLock<HashSet<String>>>,
    browser_pids: Arc<RwLock<HashSet<u32>>>,
    active_pgids: Arc<std::sync::Mutex<HashSet<u32>>>,
    active_pids: Arc<std::sync::Mutex<HashSet<u32>>>,
    cached_active_count: Arc<AtomicU32>,
    cached_total_memory_mb: Arc<AtomicU32>,
    cached_total_cpu_pct: Arc<AtomicU32>,
    task_completion_notify: Arc<tokio::sync::Notify>,
    reported_completed_tasks: std::sync::Mutex<HashSet<DevProcessId>>,
    steering_prompts: std::sync::Mutex<Vec<String>>,
    continuation_armed: AtomicBool,
    async_idle_wakeup_enabled: AtomicBool,
    orchestrator_turn_active: AtomicBool,
    current_turn_epoch: AtomicU64,
    spawned_epochs: std::sync::Mutex<HashMap<DevProcessId, u64>>,
    active_detached_commands: std::sync::Mutex<HashMap<DevProcessId, String>>,
    latest_user_prompt: std::sync::Mutex<Option<String>>,
    prompt_during_active_background: AtomicBool,
    telemetry_refresh_in_flight: AtomicBool,
}

impl Default for MiniDevRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MiniDevRegistry {
    pub fn new() -> Self {
        let registry = Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            docker_containers: Arc::new(RwLock::new(HashSet::new())),
            browser_pids: Arc::new(RwLock::new(HashSet::new())),
            active_pgids: Arc::new(std::sync::Mutex::new(HashSet::new())),
            active_pids: Arc::new(std::sync::Mutex::new(HashSet::new())),
            cached_active_count: Arc::new(AtomicU32::new(0)),
            cached_total_memory_mb: Arc::new(AtomicU32::new(0)),
            cached_total_cpu_pct: Arc::new(AtomicU32::new(0)),
            task_completion_notify: Arc::new(tokio::sync::Notify::new()),
            reported_completed_tasks: std::sync::Mutex::new(HashSet::new()),
            steering_prompts: std::sync::Mutex::new(Vec::new()),
            continuation_armed: AtomicBool::new(false),
            async_idle_wakeup_enabled: AtomicBool::new(false),
            orchestrator_turn_active: AtomicBool::new(false),
            current_turn_epoch: AtomicU64::new(1),
            spawned_epochs: std::sync::Mutex::new(HashMap::new()),
            active_detached_commands: std::sync::Mutex::new(HashMap::new()),
            latest_user_prompt: std::sync::Mutex::new(None),
            prompt_during_active_background: AtomicBool::new(false),
            telemetry_refresh_in_flight: AtomicBool::new(false),
        };
        registry.spawn_resource_watchdog();
        registry
    }

    /// Marks whether an `AgentLoop::execute_turn` is currently executing on the Orchestrator.
    pub fn set_orchestrator_turn_active(&self, active: bool) {
        self.orchestrator_turn_active
            .store(active, Ordering::Relaxed);
    }

    /// Advances the monotonic Orchestrator turn epoch so process/worker ownership across turns is
    /// tracked via exact epoch comparison rather than text heuristics.
    pub fn advance_turn_epoch(&self) -> u64 {
        self.current_turn_epoch.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Returns the current Orchestrator turn epoch.
    pub fn current_turn_epoch(&self) -> u64 {
        self.current_turn_epoch.load(Ordering::Relaxed)
    }

    /// Records that `id` was spawned during the current Orchestrator turn epoch.
    pub fn record_spawn_epoch(&self, id: &DevProcessId) {
        let epoch = self.current_turn_epoch.load(Ordering::Relaxed);
        if let Ok(mut map) = self.spawned_epochs.lock() {
            map.insert(id.clone(), epoch);
        }
    }

    /// Returns `true` if `id` is either untracked or was spawned inside the currently executing
    /// Orchestrator turn epoch (allowing safe intra-turn ephemeral QA server teardown).
    pub fn spawned_in_current_turn_epoch(&self, id: &DevProcessId) -> bool {
        let cur = self.current_turn_epoch.load(Ordering::Relaxed);
        let Ok(map) = self.spawned_epochs.lock() else {
            return true;
        };
        let lookup = map
            .get(id)
            .or_else(|| {
                let alt = DevProcessId::from(format!("worker-{}", id.as_str()));
                map.get(&alt)
            })
            .or_else(|| {
                id.as_str()
                    .strip_prefix("worker-")
                    .and_then(|s| map.get(&DevProcessId::from(s)))
            });
        lookup.map(|&ep| ep == cur).unwrap_or(true)
    }

    /// Records that a background command detached asynchronously into `MiniDevRegistry` and has
    /// not yet finished executing (used by verification gates instead of string-prefix sniffing).
    pub fn mark_command_detached(&self, id: &DevProcessId, command: &str) {
        let norm = command.trim().to_string();
        if !norm.is_empty() {
            if let Ok(mut map) = self.active_detached_commands.lock() {
                map.insert(id.clone(), norm);
            }
        }
    }

    /// Clears detached command tracking for `id` when the process finishes or is terminated.
    pub fn clear_detached_command(&self, id: &DevProcessId) {
        if let Ok(mut map) = self.active_detached_commands.lock() {
            map.remove(id);
        }
    }

    /// Returns `true` if `command` is currently running as an uncompleted detached `minitask` job.
    pub fn is_command_currently_detached(&self, command: &str) -> bool {
        let norm = command.trim();
        if norm.is_empty() {
            return false;
        }
        self.active_detached_commands
            .lock()
            .ok()
            .map(|map| map.values().any(|c| c == norm))
            .unwrap_or(false)
    }

    /// Prunes a short-lived synchronous probe/command that finished within the foreground deadline
    /// so `/tasks` and `minitask(list)` never accumulate transient `<2.5s` commands.
    pub async fn remove_completed_ephemeral(&self, id: &DevProcessId) {
        self.clear_detached_command(id);
        if let Ok(mut epochs) = self.spawned_epochs.lock() {
            epochs.remove(id);
        }
        if let Ok(mut reported) = self.reported_completed_tasks.lock() {
            reported.remove(id);
        }
        let removed = {
            let mut lock = self.processes.write().await;
            lock.remove(id)
        };
        if let Some(handle) = removed {
            self.untrack_process_ids(handle.pid(), handle.pgid());
            decrement_atomic_saturating(&self.cached_active_count);
            if let Some(ref log_path) = handle.log_path {
                let _ = tokio::fs::remove_file(log_path).await;
            }
        }
    }

    /// Enables or disables non-blocking TUI async turn yield when background `minitask` / `MiniPub`
    /// jobs are running (enabled in Interactive TUI mode, disabled in one-shot headless CLI mode).
    pub fn set_async_idle_wakeup_enabled(&self, enabled: bool) {
        self.async_idle_wakeup_enabled
            .store(enabled, Ordering::Relaxed);
    }

    /// Returns `true` when Interactive TUI reactive idle wakeup is active so background jobs
    /// detach promptly instead of blocking the Orchestrator turn.
    pub fn is_async_idle_wakeup_enabled(&self) -> bool {
        self.async_idle_wakeup_enabled.load(Ordering::Relaxed)
    }

    /// Records the latest explicit human prompt (ignoring synthetic `[SYSTEM ...]` wakeups) and notes
    /// whether background `minitask` or `MiniPub` jobs were already active when the user spoke.
    pub fn record_user_prompt(&self, prompt: &str, had_active_background: bool) {
        let trimmed = prompt.trim();
        if trimmed.is_empty() || trimmed.starts_with("[SYSTEM") {
            return;
        }
        if let Ok(mut guard) = self.latest_user_prompt.lock() {
            *guard = Some(trimmed.to_string());
        }
        self.prompt_during_active_background
            .store(had_active_background, Ordering::Relaxed);
    }

    /// Clears the recorded user prompt (used on turn exit and isolated unit test teardown).
    pub fn clear_recorded_user_prompt(&self) {
        if let Ok(mut guard) = self.latest_user_prompt.lock() {
            *guard = None;
        }
        self.prompt_during_active_background
            .store(false, Ordering::Relaxed);
    }

    /// Negation-aware, clause-scoped, target-conditioned polarity scorer for termination intent.
    ///
    /// - Positive score (`> 0`): explicit un-negated imperative directive to stop/kill/cancel the target.
    /// - Zero (`0`): neutral, status-inquiry, or interrogative prompt (`"what's happening"`, `"why did it stop?"`).
    /// - Negative score (`< 0`): explicit preservation or negated termination (`"don't stop the build"`, `"keep vite running"`).
    pub fn analyze_termination_polarity(prompt: &str, target_label: Option<&str>) -> i32 {
        let lower = prompt.to_ascii_lowercase();
        if lower.trim().is_empty() {
            return 0;
        }

        let target_stems: Vec<&str> = target_label
            .into_iter()
            .flat_map(|lbl| lbl.split(|c: char| !c.is_ascii_alphanumeric()))
            .filter(|t| {
                t.len() >= 3
                    && !matches!(
                        *t,
                        "dev"
                            | "worker"
                            | "subagent"
                            | "minipub"
                            | "task"
                            | "all"
                            | "active"
                            | "background"
                            | "jobs"
                            | "daemons"
                    )
                    && !t.chars().all(|c| c.is_ascii_digit())
            })
            .collect();

        // Normalize contrastive discourse markers into clause separators (`\x1f`)
        let mut normalized = lower;
        for marker in [
            " but ",
            " however ",
            " though ",
            " instead ",
            " whereas ",
            " while ",
        ] {
            normalized = normalized.replace(marker, "\x1f");
        }

        let is_termination_stem = |w: &str| -> bool {
            let bare = w.trim_matches('\'');
            bare.starts_with("stop")
                || bare.starts_with("kill")
                || bare.starts_with("cancel")
                || bare.starts_with("abort")
                || bare.starts_with("terminat")
                || bare.starts_with("halt")
                || bare == "shutdown"
                || bare.starts_with("quit")
                || bare.starts_with("restart")
        };

        let mut total_score = 0i32;

        for raw_clause in normalized.split(['\n', '.', ';', '!', '?', ',', '\x1f']) {
            let tokens: Vec<&str> = raw_clause
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '\'')
                .map(|t| t.trim_matches('\''))
                .filter(|t| !t.is_empty())
                .collect();
            if tokens.is_empty() {
                continue;
            }

            // Interrogative clauses ("why did it stop", "did reviewer stop") ask a question rather than issuing a command
            if matches!(
                tokens[0],
                "what"
                    | "whats"
                    | "what's"
                    | "why"
                    | "how"
                    | "when"
                    | "where"
                    | "who"
                    | "which"
                    | "did"
                    | "does"
                    | "is"
                    | "are"
                    | "was"
                    | "were"
                    | "has"
                    | "have"
            ) {
                continue;
            }

            let mut clause_score = 0i32;
            let mut neg_window = 0usize;
            let mut i = 0usize;
            while i < tokens.len() {
                let w = tokens[i];
                let is_negator = matches!(
                    w,
                    "not"
                        | "no"
                        | "never"
                        | "neither"
                        | "nor"
                        | "without"
                        | "avoid"
                        | "dont"
                        | "don't"
                        | "doesnt"
                        | "doesn't"
                        | "didnt"
                        | "didn't"
                        | "cant"
                        | "can't"
                        | "cannot"
                        | "wont"
                        | "won't"
                        | "shouldnt"
                        | "shouldn't"
                        | "wouldnt"
                        | "wouldn't"
                ) || w.ends_with("n't");

                if is_negator {
                    neg_window = 4;
                    i += 1;
                    continue;
                }

                if matches!(w, "keep" | "leave" | "continue" | "preserve" | "maintain") {
                    clause_score -= 2;
                    neg_window = 3;
                    i += 1;
                    continue;
                }

                let is_shut_down = w == "shut"
                    && tokens
                        .get(i + 1)
                        .is_some_and(|&next| next == "down" || next == "it");
                if is_termination_stem(w) || is_shut_down {
                    if neg_window > 0 {
                        clause_score -= 3;
                    } else {
                        clause_score += 2;
                    }
                    if is_shut_down {
                        i += 2;
                        continue;
                    }
                } else if neg_window > 0
                    && !matches!(
                        w,
                        "please"
                            | "the"
                            | "a"
                            | "an"
                            | "any"
                            | "all"
                            | "my"
                            | "our"
                            | "this"
                            | "that"
                            | "running"
                            | "active"
                            | "current"
                    )
                {
                    neg_window -= 1;
                }
                i += 1;
            }

            if clause_score == 0 {
                continue;
            }

            if target_stems.is_empty() {
                total_score += clause_score;
            } else {
                let matches_target = tokens.iter().any(|cw| {
                    target_stems
                        .iter()
                        .any(|ts| *cw == *ts || cw.starts_with(ts) || ts.starts_with(cw))
                });
                let mentions_universal = tokens
                    .iter()
                    .any(|&cw| matches!(cw, "all" | "everything" | "it" | "them" | "both"));
                if matches_target {
                    total_score += clause_score * 2;
                } else if mentions_universal || normalized.matches('\x1f').count() == 0 {
                    // Apply general clause polarity only when the prompt isn't a contrastive multi-target split
                    total_score += clause_score;
                }
            }
        }

        total_score
    }

    /// Returns `true` only when `prompt` carries positive un-negated termination polarity.
    pub fn user_prompt_authorizes_task_termination(prompt: &str) -> bool {
        Self::analyze_termination_polarity(prompt, None) > 0
    }

    /// Guards active background `minitask` jobs and `MiniPub` workers using `TurnEpoch` ownership
    /// and target-conditioned negation-aware clause polarity.
    /// Returns `Some(guidance)` when termination should be blocked, or `None` when allowed.
    pub fn check_background_kill_guard(
        &self,
        target_label: &str,
        is_daemon: bool,
    ) -> Option<String> {
        let candidate_id = DevProcessId::from(target_label);
        self.check_background_kill_guard_for_id(Some(&candidate_id), target_label, is_daemon)
    }

    /// Epoch- and polarity-aware kill guard for a specific `DevProcessId` (if known).
    pub fn check_background_kill_guard_for_id(
        &self,
        id: Option<&DevProcessId>,
        target_label: &str,
        is_daemon: bool,
    ) -> Option<String> {
        let prompt = self
            .latest_user_prompt
            .lock()
            .ok()
            .and_then(|g| g.clone())?;
        let polarity = Self::analyze_termination_polarity(&prompt, Some(target_label));
        if polarity > 0 {
            return None;
        }
        // Intra-turn self-owned daemon lifecycle: if an ephemeral daemon (`bun run dev`) was spawned
        // within the CURRENT turn epoch during a normal build turn (`!prompt_during_active_background`)
        // and the user did NOT explicitly forbid stopping (`polarity == 0`), allow clean QA teardown.
        if polarity == 0
            && is_daemon
            && !self.prompt_during_active_background.load(Ordering::Relaxed)
            && id
                .map(|pid| self.spawned_in_current_turn_epoch(pid))
                .unwrap_or(true)
        {
            return None;
        }
        Some(format!(
            "ℹ Active background job '{}' is still running asynchronously and the user's message ({:?}) did NOT authorize stopping it. \
             Leave '{}' running uninterrupted in `minitask` — answer the user's question or perform parallel non-destructive work directly, \
             and `minicode` will automatically wake you when background jobs finish (or call `ask_user` first if you genuinely need permission to stop it).",
            target_label,
            crate::utils::truncate_chars(&prompt, 80),
            target_label
        ))
    }

    /// Wakes any task or turn loop awaiting background task completion and clears detached tracking.
    pub fn notify_task_completed(&self, id: &DevProcessId) {
        self.clear_detached_command(id);
        self.task_completion_notify.notify_waiters();
    }

    /// Returns a future that resolves when a background task completes or a live user steering prompt arrives.
    pub async fn notified_task_or_steering(&self) {
        self.task_completion_notify.notified().await;
    }

    /// Enqueues a live user prompt submitted while the orchestrator or a background `minitask` / `MiniPub` is active,
    /// immediately waking any `await_tasks`, `await_workers`, or `wait_until_active_workers_settle` barrier without killing
    /// the running background task or subagent.
    pub fn push_steering_prompt(&self, prompt: String) {
        if self.orchestrator_turn_active.load(Ordering::Relaxed) {
            self.record_user_prompt(&prompt, true);
        }
        if let Ok(mut queue) = self.steering_prompts.lock() {
            queue.push(prompt);
        }
        self.continuation_armed.store(true, Ordering::Relaxed);
        self.task_completion_notify.notify_waiters();
        if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
            pool.notify_completion();
        }
    }

    /// Returns `true` if one or more live user steering prompts are waiting to be processed.
    pub fn has_steering_prompts(&self) -> bool {
        self.steering_prompts
            .lock()
            .map(|q| !q.is_empty())
            .unwrap_or(false)
    }

    /// Drains all queued live user steering prompts in FIFO order.
    pub fn drain_steering_prompts(&self) -> Vec<String> {
        if let Ok(mut queue) = self.steering_prompts.lock() {
            std::mem::take(&mut *queue)
        } else {
            Vec::new()
        }
    }

    /// Arms or disarms automatic idle-state continuation when background `minitask` processes complete.
    pub fn set_continuation_armed(&self, armed: bool) {
        self.continuation_armed.store(armed, Ordering::Relaxed);
    }

    /// Returns `true` if automatic continuation is armed for background `minitask` completions.
    pub fn is_continuation_armed(&self) -> bool {
        self.continuation_armed.load(Ordering::Relaxed)
    }

    /// Marks a background task as already reported to the Orchestrator (e.g., when it completed
    /// synchronously within `wait_ms_before_async` or via explicit `minitask(action="await")`).
    pub fn mark_task_reported(&self, id: &DevProcessId) {
        if let Ok(mut set) = self.reported_completed_tasks.lock() {
            set.insert(id.clone());
        }
    }

    /// Spawns an autonomous background resource watchdog loop.
    /// Every 2 seconds, monitors CPU and RSS memory consumption across all active process trees
    /// and browser instances. If an OOM spike or runaway memory allocation exceeds the ceiling,
    /// cleanly terminates the offending process tree to safeguard the host system.
    fn spawn_resource_watchdog(&self) {
        let processes = Arc::clone(&self.processes);
        let cached_active_count = Arc::clone(&self.cached_active_count);
        let cached_total_memory_mb = Arc::clone(&self.cached_total_memory_mb);
        let cached_total_cpu_pct = Arc::clone(&self.cached_total_cpu_pct);

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(2000));

                loop {
                    interval.tick().await;

                    // Dynamically derive default runaway ceiling as 25% of physical host RAM from /proc/meminfo
                    let host_total_mb =
                        crate::dev::metrics::sample_host_memory_total_mb().unwrap_or(0.0);
                    let default_ceiling_mb: f32 = (host_total_mb * 0.25).max(512.0);

                    let mut total_active: u32 = 0;
                    let mut total_memory_mb: f32 = 0.0;
                    let mut total_cpu_pct: f32 = 0.0;

                    // 1. Check managed processes
                    let handles: Vec<Arc<DevProcessHandle>> = {
                        let lock = processes.read().await;
                        lock.values().cloned().collect()
                    };

                    let my_pid = std::process::id();
                    let mut seen_pids = HashSet::new();
                    for handle in handles {
                        let status = handle.status.read().await.clone();
                        if !status.is_alive() {
                            continue;
                        }

                        let pid = handle.pid();

                        // Virtual in-process workers (subagents, swarm tasks, timers) have pid == 0 or share minicode's PID.
                        // Estimate their in-process memory footprint for TUI telemetry, but never subject minicode's own PID
                        // to OS descendant process tree memory summation or OOM termination.
                        if pid == 0 || pid == my_pid {
                            let usage = Self::estimate_in_process_usage(&handle).await;
                            total_active += 1;
                            total_memory_mb += usage.memory_rss_mb;
                            total_cpu_pct += usage.cpu_percent;
                            continue;
                        }

                        // Only monitor alive OS processes once per unique root PID
                        if !seen_pids.insert(pid) || !crate::dev::ports::is_process_running(pid) {
                            continue;
                        }

                        // Sample tree metrics (root + all descendants)
                        let usage = crate::dev::metrics::sample_process_metrics(pid);

                        let limit_mb = handle
                            .max_memory_mb
                            .map(|m| m as f32)
                            .unwrap_or(default_ceiling_mb);

                        // Check for RAM spike exceeding ceiling
                        if usage.memory_rss_mb > limit_mb {
                            let alert_msg = format!(
                                "[RESOURCE_WATCHDOG] 🚨 MEMORY SPIKE DETECTED: Process '{}' (PID {}) consumed {:.1} MB, exceeding ceiling of {:.1} MB! Terminating runaway process to prevent system freeze.",
                                handle.id.as_str(),
                                pid,
                                usage.memory_rss_mb,
                                limit_mb
                            );
                            tracing::warn!("{}", alert_msg);
                            handle.append_log(&alert_msg).await;
                            handle
                                .update_status(crate::dev::models::DevProcessStatus::Degraded(
                                    format!(
                                        "Killed by OOM Watchdog: {:.1}MB exceeded {:.1}MB ceiling",
                                        usage.memory_rss_mb, limit_mb
                                    ),
                                ))
                                .await;

                            // Gracefully terminate then kill runaway process group
                            let _ = handle.terminate().await;
                            continue;
                        }

                        total_active += 1;
                        total_memory_mb += usage.memory_rss_mb;
                        total_cpu_pct += usage.cpu_percent;
                    }

                    // 2. Check live browser engine
                    if crate::tools::browser::BrowserManager::is_live_engine_running().await {
                        if let Some((_engine, browser_pid, _port)) =
                            crate::tools::browser::BrowserManager::get_live_engine_info().await
                        {
                            if browser_pid > 0
                                && seen_pids.insert(browser_pid)
                                && crate::dev::ports::is_process_running(browser_pid)
                            {
                                let browser_usage =
                                    crate::dev::metrics::sample_process_metrics(browser_pid);

                                if browser_usage.memory_rss_mb > default_ceiling_mb {
                                    tracing::warn!(
                                        "[RESOURCE_WATCHDOG] 🚨 Browser engine PID {} consumed {:.1} MB, exceeding {:.1} MB ceiling! Shutting down browser.",
                                        browser_pid,
                                        browser_usage.memory_rss_mb,
                                        default_ceiling_mb
                                    );
                                    let _ =
                                        crate::tools::browser::BrowserManager::shutdown_live_engine(
                                        )
                                        .await;
                                } else {
                                    total_active += 1;
                                    total_memory_mb += browser_usage.memory_rss_mb;
                                    total_cpu_pct += browser_usage.cpu_percent;
                                }
                            }
                        }
                    }

                    cached_active_count.store(total_active, Ordering::Relaxed);
                    cached_total_memory_mb.store(
                        (total_memory_mb * 10.0).round() as u32,
                        Ordering::Relaxed,
                    );
                    cached_total_cpu_pct.store(
                        (total_cpu_pct * 10.0).round() as u32,
                        Ordering::Relaxed,
                    );
                }
            });
        }
    }

    /// Returns an instant, lock-free telemetry snapshot of active background tasks:
    /// `(active_count, total_memory_rss_mb, total_cpu_percent)`.
    /// Zero-cost reading suitable for per-frame 60 FPS TUI rendering.
    pub fn get_telemetry_snapshot(&self) -> (usize, f32, f32) {
        let count = self.cached_active_count.load(Ordering::Relaxed) as usize;
        let mem_mb = self.cached_total_memory_mb.load(Ordering::Relaxed) as f32 / 10.0;
        let cpu_pct = self.cached_total_cpu_pct.load(Ordering::Relaxed) as f32 / 10.0;
        (count, mem_mb, cpu_pct)
    }

    /// Spawns and registers a new development daemon or process.
    pub async fn spawn(
        &self,
        workspace_root: &Path,
        req: SpawnDevRequest,
    ) -> Result<DevProcessSummary> {
        let (summary, _) = self.spawn_with_options(workspace_root, req, true).await?;
        Ok(summary)
    }

    /// Spawns and registers a managed process or background task with explicit daemon vs non-daemon semantics,
    /// returning both the initial `DevProcessSummary` and the live `Arc<DevProcessHandle>`.
    pub async fn spawn_with_options(
        &self,
        workspace_root: &Path,
        req: SpawnDevRequest,
        is_daemon: bool,
    ) -> Result<(DevProcessSummary, Arc<DevProcessHandle>)> {
        // Singleton Frontend Dev Server Invariant:
        // A single workspace directory should only run one active frontend development server at a time.
        // If an active frontend server already exists in the same target directory, terminate the previous
        // server first to prevent orphan zombie cascades and port collisions.
        if is_daemon && req.process_type == DevProcessType::Frontend {
            let target_dir = req
                .working_dir
                .as_ref()
                .map(|p| workspace_root.join(p))
                .unwrap_or_else(|| workspace_root.to_path_buf());

            let mut to_terminate = Vec::new();
            {
                let lock = self.processes.read().await;
                for (id, handle) in lock.iter() {
                    if handle.process_type == DevProcessType::Frontend
                        && handle.working_dir == target_dir
                        && handle.status.read().await.is_alive()
                    {
                        to_terminate.push((id.clone(), Arc::clone(handle)));
                    }
                }
            }

            for (old_id, old_handle) in to_terminate {
                tracing::info!(
                    id = %old_id,
                    dir = %target_dir.display(),
                    "Replacing existing frontend dev server with new instance"
                );
                let _ = old_handle.terminate().await;
                self.untrack_process_ids(old_handle.pid(), old_handle.pgid());
                decrement_atomic_saturating(&self.cached_active_count);
            }
        }

        let is_docker =
            req.process_type == DevProcessType::Docker || req.command.contains("docker run");
        let handle = crate::dev::process::spawn_process_group_with_options(
            workspace_root,
            req,
            None,
            is_daemon,
        )
        .await?;
        let handle_arc = Arc::new(handle);
        let id = handle_arc.id.clone();
        self.record_spawn_epoch(&id);
        self.track_process_ids(handle_arc.pid(), handle_arc.pgid());

        if is_docker {
            let mut dock = self.docker_containers.write().await;
            dock.insert(id.as_str().to_string());
        }

        let summary = self.create_summary(&handle_arc).await;

        {
            let mut lock = self.processes.write().await;
            lock.insert(id, Arc::clone(&handle_arc));
        }
        self.cached_active_count.fetch_add(1, Ordering::Relaxed);
        self.trigger_background_telemetry_refresh();

        Ok((summary, handle_arc))
    }

    pub(super) fn track_process_ids(&self, pid: u32, pgid: u32) {
        let my_pid = std::process::id();
        if pgid > 0 && pgid != my_pid {
            if let Ok(mut set) = self.active_pgids.lock() {
                set.insert(pgid);
            }
        }
        if pid > 0 && pid != my_pid {
            if let Ok(mut set) = self.active_pids.lock() {
                set.insert(pid);
            }
        }
    }

    pub(crate) fn untrack_process_ids(&self, pid: u32, pgid: u32) {
        let my_pid = std::process::id();
        if pgid > 0 && pgid != my_pid {
            if let Ok(mut set) = self.active_pgids.lock() {
                set.remove(&pgid);
            }
        }
        if pid > 0 && pid != my_pid {
            if let Ok(mut set) = self.active_pids.lock() {
                set.remove(&pid);
            }
        }
    }

    pub(super) async fn build_live_browser_summary() -> Option<DevProcessSummary> {
        if !crate::tools::browser::BrowserManager::is_live_engine_running().await {
            return None;
        }
        let (engine, pid, port, uptime_secs) =
            crate::tools::browser::BrowserManager::get_live_engine_details().await?;
        let usage = sample_process_metrics(pid);
        Some(DevProcessSummary {
            id: DevProcessId::from("browser"),
            name: format!("browser-{}", engine.to_lowercase()),
            process_type: DevProcessType::Chrome,
            status: DevProcessStatus::Running,
            pid: Some(pid),
            ports: vec![port],
            url: Some(format!("http://127.0.0.1:{}", port)),
            cpu_percent: usage.cpu_percent,
            memory_rss_mb: usage.memory_rss_mb,
            uptime_secs,
            restart_count: 0,
            restart_policy: crate::dev::models::RestartPolicy::Never,
            port_resolution: None,
            schedule_info: None,
        })
    }

    pub(super) fn lookup_handle<'a>(
        lock: &'a HashMap<DevProcessId, Arc<DevProcessHandle>>,
        id: &DevProcessId,
    ) -> Option<&'a Arc<DevProcessHandle>> {
        lock.get(id)
            .or_else(|| {
                let alt = DevProcessId::from(format!("worker-{}", id.as_str()));
                lock.get(&alt)
            })
            .or_else(|| {
                id.as_str()
                    .strip_prefix("worker-")
                    .and_then(|s| lock.get(&DevProcessId::from(s)))
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_virtual_handle(
        id: DevProcessId,
        name: String,
        process_type: DevProcessType,
        command: String,
        working_dir: PathBuf,
        pid: u32,
        pgid: u32,
        cancel_hook: Option<Arc<dyn Fn() + Send + Sync>>,
        schedule_info: Option<ScheduleInfo>,
    ) -> Arc<DevProcessHandle> {
        let working_dir =
            crate::agent::subagent::transcript::resolve_host_workspace_root(&working_dir);
        let is_daemon = matches!(process_type, DevProcessType::Cron);
        let log_path = if working_dir.exists() && working_dir != Path::new("/tmp") {
            let log_dir = working_dir.join(".minicode").join("logs").join("tasks");
            if std::fs::create_dir_all(&log_dir).is_ok() {
                Some(log_dir.join(format!("{}.log", id.as_str())))
            } else {
                None
            }
        } else {
            None
        };
        Arc::new(DevProcessHandle {
            id,
            name,
            process_type,
            command,
            working_dir,
            extra_env: HashMap::new(),
            port_policy: None,
            max_memory_mb: None,
            pid: Arc::new(std::sync::atomic::AtomicU32::new(pid)),
            pgid: Arc::new(std::sync::atomic::AtomicU32::new(pgid)),
            started_at: std::time::Instant::now(),
            finished_elapsed_secs: Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX)),
            status: Arc::new(RwLock::new(DevProcessStatus::Running)),
            ports: Arc::new(RwLock::new(Vec::new())),
            logs: Arc::new(RwLock::new(std::collections::VecDeque::with_capacity(
                crate::dev::process::MAX_RING_BUFFER_LINES,
            ))),
            is_shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cancel_hook: cancel_hook.map(crate::dev::process::CancelHook::new),
            restart_policy: crate::dev::models::RestartPolicy::Never,
            restart_stats: Arc::new(RwLock::new(crate::dev::models::RestartStats::default())),
            port_resolution: Arc::new(RwLock::new(None)),
            schedule_info: Arc::new(RwLock::new(schedule_info)),
            is_daemon,
            log_path,
            stdin_tx: None,
            completion_notify: Arc::new(tokio::sync::Notify::new()),
        })
    }

    /// Appends a live log line to a registered worker or task handle (matching `id` or `worker-<id>`).
    pub async fn append_worker_log(&self, id_raw: &str, line: impl Into<String>) {
        let dev_id = DevProcessId::from(id_raw);
        let handle_opt = {
            let lock = self.processes.read().await;
            Self::lookup_handle(&lock, &dev_id).cloned()
        };
        if let Some(handle) = handle_opt {
            handle.append_log(line).await;
        }
    }

    /// Lists summaries of all managed processes, optionally filtered by process category.
    pub async fn list_filtered(
        &self,
        filter_type: Option<DevProcessType>,
    ) -> Vec<DevProcessSummary> {
        let lock = self.processes.read().await;
        let mut summaries = Vec::with_capacity(lock.len() + 1);
        for handle in lock.values() {
            if let Some(target) = filter_type {
                if handle.process_type != target {
                    continue;
                }
            }
            let status = handle.status.read().await.clone();
            let pid = handle.pid();
            // For Worker processes, keep completed (`Exited(Some(0))`) entries visible unless explicitly stopped/killed
            let keep_completed_worker = handle.process_type == DevProcessType::Worker
                && matches!(status, DevProcessStatus::Exited(Some(0)));
            if !status.is_alive() && !keep_completed_worker {
                continue;
            }
            if pid > 0
                && !crate::dev::ports::is_process_running(pid)
                && !matches!(status, DevProcessStatus::Degraded(_))
                && !keep_completed_worker
            {
                continue;
            }
            summaries.push(self.create_summary(handle).await);
        }

        // Include live browser engine if active
        if filter_type.is_none() || filter_type == Some(DevProcessType::Chrome) {
            if let Some(browser_summary) = Self::build_live_browser_summary().await {
                summaries.push(browser_summary);
            }
        }

        summaries.sort_by(|a, b| {
            b.status
                .is_alive()
                .cmp(&a.status.is_alive())
                .then_with(|| a.name.cmp(&b.name))
        });
        summaries
    }

    /// Lists summaries of all managed processes including recently completed workers, swarms, and tasks
    /// for interactive UI dashboards (`F7` Task Menu and `SwarmDeckData`).
    pub async fn list_all_with_completed(
        &self,
        filter_type: Option<DevProcessType>,
        workspace_root: Option<&Path>,
    ) -> Vec<DevProcessSummary> {
        let normalized_ws =
            workspace_root.map(crate::agent::subagent::transcript::resolve_host_workspace_root);
        let workspace_root = normalized_ws.as_deref();

        let lock = self.processes.read().await;
        let mut summaries = Vec::with_capacity(lock.len() + 8);
        let mut seen_ids = std::collections::HashSet::new();

        for handle in lock.values() {
            if let Some(target) = filter_type {
                if handle.process_type != target {
                    continue;
                }
            }
            if let Some(ws) = workspace_root {
                let handle_ws = crate::agent::subagent::transcript::resolve_host_workspace_root(
                    &handle.working_dir,
                );
                let in_ws = handle_ws.starts_with(ws)
                    || ws.starts_with(&handle_ws)
                    || (handle.process_type == DevProcessType::Swarm
                        && handle.working_dir == Path::new("/tmp"));
                if !in_ws && ws != Path::new("/tmp") && !ws.as_os_str().is_empty() {
                    if handle.working_dir.exists()
                        && ws.exists()
                        && handle.working_dir != Path::new("/tmp")
                    {
                        continue;
                    }
                }
            }
            let summary = self.create_summary(handle).await;
            seen_ids.insert(summary.id.as_str().to_string());
            if let Some(stripped) = summary.id.as_str().strip_prefix("worker-") {
                seen_ids.insert(stripped.to_string());
            }
            summaries.push(summary);
        }
        drop(lock);

        // Bridge SubagentPool workers (initializing the pool for `workspace_root` if needed)
        // and any persisted `.minicode/subagents/*/transcript.json` records on disk.
        if filter_type.is_none() || filter_type == Some(DevProcessType::Worker) {
            let pool_opt = if let Some(ws) = workspace_root {
                if ws.exists() && ws != Path::new("/tmp") && !ws.as_os_str().is_empty() {
                    Some(crate::agent::subagent::get_global_subagent_pool(ws))
                } else {
                    crate::agent::subagent::try_get_global_subagent_pool()
                }
            } else {
                crate::agent::subagent::try_get_global_subagent_pool()
            };

            if let Some(pool) = pool_opt {
                let pool_matches_ws = workspace_root
                    .map(|ws| {
                        pool.workspace_root() == ws
                            || pool.workspace_root().starts_with(ws)
                            || ws.starts_with(pool.workspace_root())
                    })
                    .unwrap_or(true);
                if pool_matches_ws {
                    for sub in pool.snapshot_subagents() {
                        let dev_id_str = if sub.id.starts_with("worker-") {
                            sub.id.clone()
                        } else {
                            format!("worker-{}", sub.id)
                        };
                        if seen_ids.contains(&dev_id_str) || seen_ids.contains(&sub.id) {
                            continue;
                        }
                        seen_ids.insert(dev_id_str.clone());
                        seen_ids.insert(sub.id.clone());
                        let status = match sub.state {
                            crate::agent::subagent::SubagentState::Starting
                            | crate::agent::subagent::SubagentState::Running
                            | crate::agent::subagent::SubagentState::WaitingForInput => {
                                DevProcessStatus::Running
                            }
                            crate::agent::subagent::SubagentState::Completed => {
                                DevProcessStatus::Exited(Some(0))
                            }
                            crate::agent::subagent::SubagentState::Failed(ref msg) => {
                                DevProcessStatus::Degraded(msg.clone())
                            }
                            crate::agent::subagent::SubagentState::Terminated => {
                                DevProcessStatus::Stopped
                            }
                        };
                        let uptime_secs = sub
                            .finished_at_secs
                            .unwrap_or_else(|| {
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(sub.started_at_secs)
                            })
                            .saturating_sub(sub.started_at_secs);
                        summaries.push(DevProcessSummary {
                            id: DevProcessId::from(dev_id_str),
                            name: format!(
                                "MiniPub ({}) - {}",
                                sub.role.badge(),
                                crate::utils::truncate_ellipsis(&sub.prompt, 40)
                            ),
                            process_type: DevProcessType::Worker,
                            status,
                            pid: Some(std::process::id()),
                            ports: Vec::new(),
                            url: None,
                            uptime_secs,
                            cpu_percent: 0.0,
                            memory_rss_mb: 0.0,
                            restart_count: 0,
                            restart_policy: crate::dev::models::RestartPolicy::Never,
                            port_resolution: None,
                            schedule_info: None,
                        });
                    }
                }
            }

            if let Some(ws) = workspace_root {
                let disk_transcripts =
                    crate::agent::subagent::transcript::get_global_transcript_store()
                        .load_all_from_disk(ws);
                for t in disk_transcripts {
                    let raw_id = t
                        .subagent_id
                        .strip_prefix("worker-")
                        .unwrap_or(&t.subagent_id)
                        .to_string();
                    let dev_id_str = format!("worker-{}", raw_id);
                    if seen_ids.contains(&dev_id_str) || seen_ids.contains(&raw_id) {
                        continue;
                    }
                    seen_ids.insert(dev_id_str.clone());
                    seen_ids.insert(raw_id);
                    let uptime_secs = t
                        .finished_at_secs
                        .unwrap_or(t.started_at_secs)
                        .saturating_sub(t.started_at_secs);
                    let disk_status = match t.final_report.as_deref().map(str::trim) {
                        Some(rep) if rep.starts_with('✗') => {
                            DevProcessStatus::Degraded(crate::utils::truncate_ellipsis(rep, 60))
                        }
                        _ => DevProcessStatus::Exited(Some(0)),
                    };
                    summaries.push(DevProcessSummary {
                        id: DevProcessId::from(dev_id_str),
                        name: format!(
                            "MiniPub ({}) - {}",
                            t.role.badge(),
                            crate::utils::truncate_ellipsis(&t.prompt, 40)
                        ),
                        process_type: DevProcessType::Worker,
                        status: disk_status,
                        pid: None,
                        ports: Vec::new(),
                        url: None,
                        uptime_secs,
                        cpu_percent: 0.0,
                        memory_rss_mb: 0.0,
                        restart_count: 0,
                        restart_policy: crate::dev::models::RestartPolicy::Never,
                        port_resolution: None,
                        schedule_info: None,
                    });
                }
            }
        }

        // Hydrate persisted swarms and swarm workers from `.minicode/swarms/` on disk
        if filter_type.is_none() || filter_type == Some(DevProcessType::Swarm) {
            if let Some(ws) = workspace_root {
                let swarms_dir = ws.join(".minicode").join("swarms");
                if swarms_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&swarms_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if !path.is_dir() {
                                continue;
                            }
                            let plan_file = path.join("plan.json");
                            if !plan_file.exists() {
                                continue;
                            }
                            let Ok(plan_raw) = std::fs::read_to_string(&plan_file) else {
                                continue;
                            };
                            let Ok(plan) = serde_json::from_str::<
                                crate::agent::swarm::models::SwarmPlan,
                            >(&plan_raw) else {
                                continue;
                            };
                            let state_opt = std::fs::read_to_string(path.join("state.json"))
                                .ok()
                                .and_then(|s| {
                                    serde_json::from_str::<
                                        crate::agent::swarm::models::SwarmExecutionState,
                                    >(&s)
                                    .ok()
                                });
                            let swarm_id = format!("swarm-{}", plan.id);
                            if !seen_ids.contains(&swarm_id) {
                                seen_ids.insert(swarm_id.clone());
                                let is_complete = state_opt
                                    .as_ref()
                                    .map(|st| {
                                        st.task_statuses
                                            .values()
                                            .filter(|&&s| {
                                                s == crate::agent::swarm::models::SwarmTaskStatus::Completed
                                            })
                                            .count()
                                            >= plan.tasks.len()
                                    })
                                    .unwrap_or(true);
                                let status = if is_complete {
                                    DevProcessStatus::Exited(Some(0))
                                } else {
                                    DevProcessStatus::Stopped
                                };
                                summaries.push(DevProcessSummary {
                                    id: DevProcessId::from(swarm_id),
                                    name: format!(
                                        "Swarm ({} tasks) - {}",
                                        plan.tasks.len(),
                                        crate::utils::truncate_ellipsis(&plan.title, 36)
                                    ),
                                    process_type: DevProcessType::Swarm,
                                    status,
                                    pid: None,
                                    ports: Vec::new(),
                                    url: None,
                                    uptime_secs: 0,
                                    cpu_percent: 0.0,
                                    memory_rss_mb: 0.0,
                                    restart_count: 0,
                                    restart_policy: crate::dev::models::RestartPolicy::Never,
                                    port_resolution: None,
                                    schedule_info: None,
                                });
                            }
                            for task in &plan.tasks {
                                let worker_id = format!("swarm-worker-{}-{}", plan.id, task.id);
                                if seen_ids.contains(&worker_id) {
                                    continue;
                                }
                                seen_ids.insert(worker_id.clone());
                                let status = if let Some(ref st) = state_opt {
                                    match st.task_statuses.get(&task.id) {
                                        Some(
                                            crate::agent::swarm::models::SwarmTaskStatus::Failed,
                                        ) => DevProcessStatus::Degraded("Task failed".to_string()),
                                        Some(
                                            crate::agent::swarm::models::SwarmTaskStatus::Completed,
                                        ) => DevProcessStatus::Exited(Some(0)),
                                        Some(
                                            crate::agent::swarm::models::SwarmTaskStatus::Running,
                                        ) => DevProcessStatus::Running,
                                        _ => DevProcessStatus::Stopped,
                                    }
                                } else {
                                    DevProcessStatus::Exited(Some(0))
                                };
                                summaries.push(DevProcessSummary {
                                    id: DevProcessId::from(worker_id),
                                    name: format!(
                                        "Swarm Worker [{}] - {}",
                                        task.id,
                                        crate::utils::truncate_ellipsis(&task.title, 36)
                                    ),
                                    process_type: DevProcessType::Swarm,
                                    status,
                                    pid: None,
                                    ports: Vec::new(),
                                    url: None,
                                    uptime_secs: 0,
                                    cpu_percent: 0.0,
                                    memory_rss_mb: 0.0,
                                    restart_count: 0,
                                    restart_policy: crate::dev::models::RestartPolicy::Never,
                                    port_resolution: None,
                                    schedule_info: None,
                                });
                            }
                        }
                    }
                }
            }

            // Synthesize a `swarm-minipub-wave` summary when MiniPub workers exist for this workspace
            // so `[4] ◉ Swarm` and `minitask(action="swarms")` always reflect MiniPub / delegate_task / fanout waves.
            let has_swarm_head = summaries.iter().any(|s| {
                s.process_type == DevProcessType::Swarm
                    && !s.id.as_str().starts_with("swarm-worker-")
            });
            if !has_swarm_head && !seen_ids.contains("swarm-minipub-wave") {
                let mut worker_ids_for_wave = std::collections::HashSet::new();
                let mut any_alive = false;
                let mut max_uptime = 0u64;

                let pool_opt = if let Some(ws) = workspace_root {
                    if ws.exists() && ws != Path::new("/tmp") && !ws.as_os_str().is_empty() {
                        Some(crate::agent::subagent::get_global_subagent_pool(ws))
                    } else {
                        crate::agent::subagent::try_get_global_subagent_pool()
                    }
                } else {
                    crate::agent::subagent::try_get_global_subagent_pool()
                };

                if let Some(pool) = pool_opt {
                    let pool_matches_ws = workspace_root
                        .map(|ws| {
                            pool.workspace_root() == ws
                                || pool.workspace_root().starts_with(ws)
                                || ws.starts_with(pool.workspace_root())
                        })
                        .unwrap_or(true);
                    if pool_matches_ws {
                        for sub in pool.snapshot_subagents() {
                            worker_ids_for_wave.insert(sub.id.clone());
                            if matches!(
                                sub.state,
                                crate::agent::subagent::SubagentState::Starting
                                    | crate::agent::subagent::SubagentState::Running
                                    | crate::agent::subagent::SubagentState::WaitingForInput
                            ) {
                                any_alive = true;
                            }
                            max_uptime = max_uptime.max(sub.elapsed_secs());
                        }
                    }
                }

                if let Some(ws) = workspace_root {
                    for t in crate::agent::subagent::transcript::get_global_transcript_store()
                        .load_all_from_disk(ws)
                    {
                        let raw_id = t
                            .subagent_id
                            .strip_prefix("worker-")
                            .unwrap_or(&t.subagent_id)
                            .to_string();
                        worker_ids_for_wave.insert(raw_id);
                        let dur = t
                            .finished_at_secs
                            .unwrap_or(t.started_at_secs)
                            .saturating_sub(t.started_at_secs);
                        max_uptime = max_uptime.max(dur);
                    }
                }

                if !worker_ids_for_wave.is_empty() {
                    seen_ids.insert("swarm-minipub-wave".to_string());
                    summaries.push(DevProcessSummary {
                        id: DevProcessId::from("swarm-minipub-wave"),
                        name: format!("MiniPub Swarm Wave ({} workers)", worker_ids_for_wave.len()),
                        process_type: DevProcessType::Swarm,
                        status: if any_alive {
                            DevProcessStatus::Running
                        } else {
                            DevProcessStatus::Exited(Some(0))
                        },
                        pid: Some(std::process::id()),
                        ports: Vec::new(),
                        url: None,
                        uptime_secs: max_uptime,
                        cpu_percent: 0.0,
                        memory_rss_mb: 0.0,
                        restart_count: 0,
                        restart_policy: crate::dev::models::RestartPolicy::Never,
                        port_resolution: None,
                        schedule_info: None,
                    });
                }
            }
        }

        // Hydrate persisted task/worker logs from `.minicode/logs/tasks/*.log` on disk
        if let Some(ws) = workspace_root {
            let log_dir = ws.join(".minicode").join("logs").join("tasks");
            if log_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&log_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) != Some("log") {
                            continue;
                        }
                        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                            continue;
                        };
                        let stripped = stem.strip_prefix("worker-").unwrap_or(stem);
                        if seen_ids.contains(stem) || seen_ids.contains(stripped) {
                            continue;
                        }
                        let proc_type = DevProcessType::from_str_loose(stem);
                        if let Some(target) = filter_type {
                            if proc_type != target {
                                continue;
                            }
                        }
                        seen_ids.insert(stem.to_string());
                        seen_ids.insert(stripped.to_string());
                        let name = format!("{} ({})", proc_type, stripped);
                        summaries.push(DevProcessSummary {
                            id: DevProcessId::from(stem),
                            name,
                            process_type: proc_type,
                            status: DevProcessStatus::Exited(Some(0)),
                            pid: None,
                            ports: Vec::new(),
                            url: None,
                            uptime_secs: 0,
                            cpu_percent: 0.0,
                            memory_rss_mb: 0.0,
                            restart_count: 0,
                            restart_policy: crate::dev::models::RestartPolicy::Never,
                            port_resolution: None,
                            schedule_info: None,
                        });
                    }
                }
            }
        }

        // Include live browser engine if active
        if filter_type.is_none() || filter_type == Some(DevProcessType::Chrome) {
            if let Some(browser_summary) = Self::build_live_browser_summary().await {
                summaries.push(browser_summary);
            }
        }

        summaries.sort_by(|a, b| {
            b.status
                .is_alive()
                .cmp(&a.status.is_alive())
                .then_with(|| {
                    a.process_type
                        .display_priority()
                        .cmp(&b.process_type.display_priority())
                })
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        summaries
    }

    /// Lists summaries of all managed processes.
    pub async fn list(&self) -> Vec<DevProcessSummary> {
        self.list_filtered(None).await
    }

    /// Lists summaries of active worker processes (autonomous subagents & tasks).
    pub async fn list_workers(&self) -> Vec<DevProcessSummary> {
        self.list_filtered(Some(DevProcessType::Worker)).await
    }

    /// Lists summaries of active swarm processes (orchestrator DAGs & workers).
    pub async fn list_swarms(&self) -> Vec<DevProcessSummary> {
        self.list_filtered(Some(DevProcessType::Swarm)).await
    }

    /// Retrieves detailed summary for a specific process ID with flexible prefix matching.
    pub async fn get(&self, id: &DevProcessId) -> Option<DevProcessSummary> {
        if id.as_str() == "browser" || id.as_str() == "chrome" {
            if let Some(browser_summary) = Self::build_live_browser_summary().await {
                return Some(browser_summary);
            }
        }

        let lock = self.processes.read().await;
        if let Some(handle) = Self::lookup_handle(&lock, id) {
            Some(self.create_summary(handle).await)
        } else {
            None
        }
    }

    /// Retrieves the raw `Arc<DevProcessHandle>` for a specific process or task ID with flexible matching.
    pub async fn get_handle(&self, id: &DevProcessId) -> Option<Arc<DevProcessHandle>> {
        let lock = self.processes.read().await;
        Self::lookup_handle(&lock, id).cloned()
    }

    /// Delivers interactive `stdin` input to a running background task.
    pub async fn send_input(&self, id: &DevProcessId, input: &str) -> Result<()> {
        let handle = self
            .get_handle(id)
            .await
            .ok_or_else(|| DevError::NotFound(id.to_string()))?;
        handle.send_input(input).await
    }
}
