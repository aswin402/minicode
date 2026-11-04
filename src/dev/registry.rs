//! Global thread-safe registry managing development processes, servers, Docker stacks, and browsers.

use crate::dev::metrics::{sample_aggregate_metrics, sample_process_metrics};
use crate::dev::models::{
    DevProcessId, DevProcessStatus, DevProcessSummary, DevProcessType, RuntimeResourceSummary,
    ScheduleInfo, ScheduleRequest, SpawnDevRequest,
};
use crate::dev::process::DevProcessHandle;
use crate::error::{DevError, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;
use uuid::Uuid;

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
            telemetry_refresh_in_flight: AtomicBool::new(false),
        };
        registry.spawn_resource_watchdog();
        registry
    }

    /// Wakes any task or turn loop awaiting background task completion.
    pub fn notify_task_completed(&self, _id: &DevProcessId) {
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

    fn track_process_ids(&self, pid: u32, pgid: u32) {
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

    async fn build_live_browser_summary() -> Option<DevProcessSummary> {
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

    fn lookup_handle<'a>(
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
    fn build_virtual_handle(
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
                    summaries.push(DevProcessSummary {
                        id: DevProcessId::from(dev_id_str),
                        name: format!(
                            "MiniPub ({}) - {}",
                            t.role.badge(),
                            crate::utils::truncate_ellipsis(&t.prompt, 40)
                        ),
                        process_type: DevProcessType::Worker,
                        status: DevProcessStatus::Exited(Some(0)),
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
                        let proc_type = match DevProcessType::from_str_loose(stem) {
                            DevProcessType::Script => DevProcessType::Worker,
                            other => other,
                        };
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

    /// Lists recently completed non-daemon background tasks (builds, tests, compiles, scripts)
    /// alongside their log file path and command string.
    pub async fn list_recent_completed_tasks(
        &self,
        limit: usize,
    ) -> Vec<(DevProcessSummary, Option<PathBuf>, String)> {
        if limit == 0 {
            return Vec::new();
        }
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };
        let mut completed = Vec::new();
        for handle in handles {
            if handle.is_daemon
                || matches!(
                    handle.process_type,
                    DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                )
            {
                continue;
            }
            let status = handle.status.read().await.clone();
            if !status.is_alive() {
                let summary = self.create_summary(&handle).await;
                completed.push((
                    handle.started_at,
                    summary,
                    handle.log_path.clone(),
                    handle.command.clone(),
                ));
            }
        }
        completed.sort_by(|a, b| b.0.cmp(&a.0));
        completed
            .into_iter()
            .take(limit)
            .map(|(_, s, log_path, cmd)| (s, log_path, cmd))
            .collect()
    }

    /// Returns the number of currently running non-daemon background OS tasks (builds, tests, compiles, scripts, timers).
    pub async fn active_non_daemon_task_count(&self) -> usize {
        let lock = self.processes.read().await;
        let mut count = 0usize;
        for handle in lock.values() {
            if handle.is_daemon
                || matches!(
                    handle.process_type,
                    DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                )
            {
                continue;
            }
            if handle.status.read().await.is_alive() {
                count += 1;
            }
        }
        count
    }

    /// Event-driven wait barrier until all active non-daemon background tasks complete,
    /// a live user steering prompt arrives, or `timeout` elapses.
    pub async fn wait_until_active_tasks_settle(&self, timeout: std::time::Duration) {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.task_completion_notify.notified();
            if self.has_steering_prompts() || self.active_non_daemon_task_count().await == 0 {
                break;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            if tokio::time::timeout(deadline - now, notified)
                .await
                .is_err()
            {
                break;
            }
        }
    }

    /// Awaits specific background task IDs (or all active non-daemon background tasks if `ids` is empty)
    /// using `"all"` or `"any"` / `"first"` / `"await_next"` barrier semantics.
    /// Yields early without killing background tasks if a live user steering prompt arrives.
    pub async fn await_tasks(
        &self,
        ids: &[String],
        wait_mode: &str,
        timeout: std::time::Duration,
    ) -> Result<String> {
        let wait_any = matches!(
            wait_mode.trim().to_ascii_lowercase().as_str(),
            "any" | "first" | "next" | "await_next" | "wait_any"
        );
        let target_ids: Vec<DevProcessId> = if ids.is_empty() {
            let lock = self.processes.read().await;
            let mut active = Vec::new();
            for (id, handle) in lock.iter() {
                if !handle.is_daemon
                    && !matches!(
                        handle.process_type,
                        DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                    )
                    && handle.status.read().await.is_alive()
                {
                    active.push(id.clone());
                }
            }
            active
        } else {
            ids.iter().map(|s| DevProcessId::from(s.as_str())).collect()
        };

        if target_ids.is_empty() {
            let recent = self.list_recent_completed_tasks(5).await;
            if recent.is_empty() {
                return Ok("No active or recently completed non-daemon background tasks found in `minitask`.".to_string());
            }
            let mut out =
                String::from("All background tasks in `minitask` have already completed:\n");
            for (summary, log_path, cmd) in recent {
                out.push_str(&format!(
                    "• `{}` ({}) — Status: **{}** | Command: `{}`{}\n",
                    summary.id,
                    summary.name,
                    summary.status,
                    cmd,
                    log_path
                        .as_ref()
                        .map(|p| format!(" | Log: `{}`", p.display()))
                        .unwrap_or_default()
                ));
            }
            return Ok(out);
        }

        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.task_completion_notify.notified();
            if self.has_steering_prompts() {
                break;
            }
            let mut alive_count = 0usize;
            let mut finished_count = 0usize;

            for tid in &target_ids {
                if let Some(handle) = self.get_handle(tid).await {
                    if handle.status.read().await.is_alive() {
                        alive_count += 1;
                    } else {
                        finished_count += 1;
                    }
                }
            }

            if alive_count == 0 || (wait_any && finished_count > 0) {
                break;
            }

            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            if tokio::time::timeout(deadline - now, notified)
                .await
                .is_err()
            {
                break;
            }
        }

        let mut out = String::new();
        let mut any_still_alive = false;
        for tid in &target_ids {
            if let Some(handle) = self.get_handle(tid).await {
                let status = handle.status.read().await.clone();
                if status.is_alive() {
                    any_still_alive = true;
                } else {
                    self.mark_task_reported(&handle.id);
                }
                let logs = handle.get_logs(40, None).await;
                let log_hint = handle
                    .log_path
                    .as_ref()
                    .map(|p| format!(" | Log: `{}`", p.display()))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "### Task `{}` ({})\n- **Status:** `{}` ({}s){}\n- **Command:** `{}`\n",
                    handle.id,
                    handle.name,
                    status,
                    handle.uptime_secs(),
                    log_hint,
                    handle.command
                ));
                if !logs.is_empty() {
                    out.push_str("- **Output (last 40 lines):**\n```\n");
                    out.push_str(&logs.join("\n"));
                    out.push_str("\n```\n");
                }
            } else {
                out.push_str(&format!(
                    "### Task `{}`\n- **Status:** Not found in `minitask`\n",
                    tid
                ));
            }
        }
        if any_still_alive && self.has_steering_prompts() {
            out.push_str(
                "\n⚡ **Live User Prompt Received:** The background task(s) above are still running uninterrupted in `minitask`, and the user sent a live prompt. Handle the user's prompt now; `minitask` will automatically notify you when the background task(s) finish.\n",
            );
        }
        Ok(out)
    }

    /// Returns `true` if there is at least one completed non-daemon background task in `minitask`
    /// that has not yet been reported to the Orchestrator.
    pub async fn has_unreported_completed_tasks(&self) -> bool {
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };
        for handle in handles {
            if handle.is_daemon
                || matches!(
                    handle.process_type,
                    DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                )
            {
                continue;
            }
            if handle.status.read().await.is_alive() {
                continue;
            }
            let already_reported = self
                .reported_completed_tasks
                .lock()
                .map(|set| set.contains(&handle.id))
                .unwrap_or(false);
            if !already_reported {
                return true;
            }
        }
        false
    }

    /// Formats a live telemetry snapshot of all currently running `minitask` processes
    /// (`<active_minitask_processes>...</active_minitask_processes>`) so the Orchestrator can
    /// make context-aware decisions when a live user prompt arrives during background execution.
    pub async fn format_active_tasks_context(&self) -> String {
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };
        let mut active_lines = Vec::new();
        for handle in handles {
            let status = handle.status.read().await.clone();
            if !status.is_alive() {
                continue;
            }
            let pid = handle.pid();
            let usage = if pid > 0 && pid != std::process::id() {
                crate::dev::metrics::sample_process_metrics(pid)
            } else {
                Self::estimate_in_process_usage(&handle).await
            };
            let port_str = handle
                .ports
                .read()
                .await
                .first()
                .copied()
                .map(|p| format!(" port=\"{}\"", p))
                .unwrap_or_default();
            active_lines.push(format!(
                "  <process id=\"{}\" name=\"{}\" type=\"{}\" is_daemon=\"{}\" pid=\"{}\" uptime_secs=\"{}\" cpu_pct=\"{:.1}\" mem_mb=\"{:.1}\"{} working_dir=\"{}\" command={:?} />",
                handle.id,
                handle.name,
                handle.process_type,
                handle.is_daemon,
                handle.pid(),
                handle.uptime_secs(),
                usage.cpu_percent,
                usage.memory_rss_mb,
                port_str,
                handle.working_dir.display(),
                handle.command
            ));
        }
        if active_lines.is_empty() {
            String::new()
        } else {
            format!(
                "<active_minitask_processes>\n{}\n</active_minitask_processes>",
                active_lines.join("\n")
            )
        }
    }

    /// Harvests any newly completed non-daemon background tasks (builds, tests, compiles, scripts, timers)
    /// that have not yet been reported to the Orchestrator, formatting them into a structured
    /// `<completed_background_tasks>...</completed_background_tasks>` notification block.
    pub async fn harvest_newly_completed_tasks_block(
        &self,
        workspace_root: &Path,
    ) -> Option<(Vec<String>, String)> {
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };

        let mut newly_completed = Vec::new();
        for handle in handles {
            if handle.is_daemon
                || matches!(
                    handle.process_type,
                    DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                )
            {
                continue;
            }
            let status = handle.status.read().await.clone();
            if status.is_alive() {
                continue;
            }
            let is_new = if let Ok(mut set) = self.reported_completed_tasks.lock() {
                set.insert(handle.id.clone())
            } else {
                false
            };
            if is_new {
                newly_completed.push((handle, status));
            }
        }

        if newly_completed.is_empty() {
            return None;
        }

        let context_window = crate::tools::exec::resolve_context_window(workspace_root);
        let mut ids = Vec::with_capacity(newly_completed.len());
        let mut block = String::from("<completed_background_tasks>\n");
        for (handle, status) in newly_completed {
            ids.push(handle.id.as_str().to_string());
            let raw_logs = handle.get_logs(120, None).await.join("\n");
            let clean_logs = crate::security::sanitize_text(&raw_logs);
            let exit_code = match status {
                DevProcessStatus::Exited(code) => code,
                DevProcessStatus::Stopped => Some(0),
                _ => Some(1),
            };
            let rtk_res = crate::tools::rtk_filter::RtkFilter::filter_for_context(
                &handle.command,
                &clean_logs,
                exit_code,
                context_window,
            );
            let compacted = crate::tools::compactor::compact_tool_output_for_context(
                &handle.command,
                &rtk_res.content,
                exit_code,
                context_window,
            );
            let fault_hint = match status {
                DevProcessStatus::Exited(Some(code)) if code != 0 => {
                    let frames = crate::context::search::fault_localizer::FaultLocalizer::extract_trace_frames(&clean_logs);
                    if frames.is_empty() {
                        String::new()
                    } else {
                        let mut hint = String::from("\n[Where] Probable Fault Sites:\n");
                        for frame in frames.iter().take(4) {
                            hint.push_str(&format!(
                                "  • `{}:{}`\n",
                                frame.file_path, frame.line_number
                            ));
                        }
                        hint
                    }
                }
                _ => String::new(),
            };
            let log_attr = handle
                .log_path
                .as_ref()
                .map(|p| format!(" log_path=\"{}\"", p.display()))
                .unwrap_or_default();
            block.push_str(&format!(
                "  <task id=\"{}\" name=\"{}\" status=\"{}\" duration_secs=\"{}\" command={:?}{}>\n{}{}\n  </task>\n",
                handle.id,
                handle.name,
                status,
                handle.uptime_secs(),
                handle.command,
                log_attr,
                compacted.trim(),
                fault_hint
            ));
        }
        block.push_str("</completed_background_tasks>");
        Some((ids, block))
    }

    /// Reads recent logs from the process output ring buffer with flexible ID matching.
    pub async fn logs(
        &self,
        id: &DevProcessId,
        tail: usize,
        filter: Option<&str>,
    ) -> Result<Vec<String>> {
        let handle_opt = {
            let lock = self.processes.read().await;
            Self::lookup_handle(&lock, id).cloned()
        };
        if let Some(ref handle) = handle_opt {
            let lines = handle.get_logs(tail, filter).await;
            if !lines.is_empty() {
                return Ok(lines);
            }
        }
        let raw_id = id.as_str().strip_prefix("worker-").unwrap_or(id.as_str());
        let ws_hint_buf = handle_opt
            .as_ref()
            .map(|h| h.working_dir.clone())
            .or_else(|| {
                crate::agent::subagent::try_get_global_subagent_pool()
                    .map(|p| p.workspace_root().to_path_buf())
            })
            .or_else(|| std::env::current_dir().ok());
        let ws_hint = ws_hint_buf.as_deref();
        if let Some(transcript) =
            crate::agent::subagent::transcript::get_global_transcript_store().get(raw_id, ws_hint)
        {
            let mut synth_lines = Vec::new();
            synth_lines.push(format!(
                "[{}] Role: {} | Task: {}",
                transcript.subagent_id,
                transcript.role.badge(),
                crate::utils::truncate_ellipsis(&transcript.prompt, 100)
            ));
            for step in &transcript.steps {
                let status_icon = if step.success { "✔" } else { "✗" };
                synth_lines.push(format!(
                    "[Turn {} • Step {}] {} {} ({}ms): {}",
                    step.turn,
                    step.step_index,
                    status_icon,
                    step.tool_name,
                    step.duration_ms,
                    step.output_snippet.lines().next().unwrap_or("")
                ));
            }
            if let Some(q) = filter {
                let q_low = q.to_lowercase();
                synth_lines.retain(|l| l.to_lowercase().contains(&q_low));
            }
            if synth_lines.len() > tail && tail > 0 {
                let skip = synth_lines.len() - tail;
                synth_lines = synth_lines.into_iter().skip(skip).collect();
            }
            return Ok(synth_lines);
        }
        if let Some(ws) = ws_hint {
            let host_ws = crate::agent::subagent::transcript::resolve_host_workspace_root(ws);
            let log_dir = host_ws.join(".minicode").join("logs").join("tasks");
            for candidate in [id.as_str().to_string(), format!("worker-{}", raw_id)] {
                let log_file = log_dir.join(format!("{}.log", candidate));
                if log_file.exists() {
                    if let Ok(content) = std::fs::read_to_string(&log_file) {
                        let mut file_lines: Vec<String> =
                            content.lines().map(ToString::to_string).collect();
                        if let Some(q) = filter {
                            let q_low = q.to_lowercase();
                            file_lines.retain(|l| l.to_lowercase().contains(&q_low));
                        }
                        if file_lines.len() > tail && tail > 0 {
                            let skip = file_lines.len() - tail;
                            file_lines = file_lines.into_iter().skip(skip).collect();
                        }
                        return Ok(file_lines);
                    }
                }
            }
        }
        if handle_opt.is_some() {
            return Ok(Vec::new());
        }
        Err(DevError::NotFound(id.to_string()).into())
    }

    /// Stops a managed process or subagent worker by ID with flexible ID matching.
    pub async fn stop(&self, id: &DevProcessId) -> Result<bool> {
        if id.as_str() == "browser" || id.as_str() == "chrome" {
            let stopped = crate::tools::browser::BrowserManager::shutdown_live_engine().await?;
            if stopped {
                let _ = self.refresh_telemetry_cache().await;
                return Ok(true);
            }
        }

        let handle = {
            let lock = self.processes.read().await;
            Self::lookup_handle(&lock, id)
                .cloned()
                .ok_or_else(|| DevError::NotFound(id.to_string()))?
        };

        let current_pgid = handle.pgid();
        let current_pid = handle.pid();
        handle.terminate().await?;

        self.untrack_process_ids(current_pid, current_pgid);
        let _ = self.refresh_telemetry_cache().await;
        Ok(true)
    }

    /// Restarts a managed process, spawning a fresh instance with identical parameters.
    pub async fn restart(
        &self,
        workspace_root: &Path,
        id: &DevProcessId,
    ) -> Result<DevProcessSummary> {
        let (req, old_handle) = {
            let lock = self.processes.read().await;
            let handle = lock
                .get(id)
                .ok_or_else(|| DevError::NotFound(id.to_string()))?;
            let req = SpawnDevRequest {
                command: handle.command.clone(),
                name: Some(handle.name.clone()),
                process_type: handle.process_type,
                working_dir: Some(handle.working_dir.clone()),
                extra_env: handle.extra_env.clone(),
                port_hint: handle.ports.read().await.first().copied(),
                max_memory_mb: handle.max_memory_mb,
                port_policy: handle.port_policy,
                restart_policy: Some(handle.restart_policy.clone()),
            };
            (req, Arc::clone(handle))
        };

        // Terminate old process
        let _ = old_handle.terminate().await;
        self.untrack_process_ids(old_handle.pid(), old_handle.pgid());
        if let Ok(mut set) = self.reported_completed_tasks.lock() {
            set.remove(id);
        }

        // Spawn new process preserving the canonical ID and daemon/task mode
        let new_handle = crate::dev::process::spawn_process_group_with_options(
            workspace_root,
            req,
            Some(id.clone()),
            old_handle.is_daemon,
        )
        .await?;
        let new_arc = Arc::new(new_handle);
        self.track_process_ids(new_arc.pid(), new_arc.pgid());
        let summary = self.create_summary(&new_arc).await;

        {
            let mut lock = self.processes.write().await;
            lock.insert(id.clone(), new_arc);
        }
        let _ = self.refresh_telemetry_cache().await;

        Ok(summary)
    }

    /// Terminates ALL active managed processes, containers, and browser instances.
    /// This is guaranteed to leave zero orphan processes.
    pub async fn kill_all(&self) -> Result<usize> {
        // 1. Terminate browser engine
        let _ = crate::tools::browser::BrowserManager::shutdown_live_engine().await;

        // 2. Concurrently terminate all managed process handles
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };
        let mut count = 0;
        let mut term_futs = Vec::new();
        for handle in handles {
            if handle.status.read().await.is_alive() {
                count += 1;
                term_futs.push(async move {
                    let _ = handle.terminate().await;
                });
            }
        }
        futures::future::join_all(term_futs).await;

        // 3. Clean up any registered docker containers
        let dock_lock = self.docker_containers.read().await;
        for container_name in dock_lock.iter() {
            let _ = tokio::process::Command::new("docker")
                .args(["stop", container_name])
                .output()
                .await;
        }

        // 4. Force synchronous sweep of all remaining processes, groups, descendants & children
        kill_all_sync();

        if let Ok(mut set) = self.active_pgids.lock() {
            set.clear();
        }
        if let Ok(mut set) = self.active_pids.lock() {
            set.clear();
        }

        self.cached_active_count.store(0, Ordering::Relaxed);
        self.cached_total_memory_mb.store(0, Ordering::Relaxed);
        self.cached_total_cpu_pct.store(0, Ordering::Relaxed);

        Ok(count)
    }

    /// Registers an autonomous subagent or delegated worker task into the dev registry.
    #[allow(clippy::too_many_arguments)]
    pub async fn register_worker(
        &self,
        id: DevProcessId,
        name: String,
        command: String,
        working_dir: PathBuf,
        pid: u32,
        pgid: u32,
        cancel_hook: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Arc<DevProcessHandle> {
        self.register_managed_handle(
            DevProcessType::Worker,
            id,
            name,
            command,
            working_dir,
            pid,
            pgid,
            cancel_hook,
        )
        .await
    }

    /// Registers an autonomous swarm orchestrator or swarm worker task into the dev registry.
    #[allow(clippy::too_many_arguments)]
    pub async fn register_swarm_process(
        &self,
        id: DevProcessId,
        name: String,
        command: String,
        working_dir: PathBuf,
        pid: u32,
        pgid: u32,
        cancel_hook: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Arc<DevProcessHandle> {
        self.register_managed_handle(
            DevProcessType::Swarm,
            id,
            name,
            command,
            working_dir,
            pid,
            pgid,
            cancel_hook,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn register_managed_handle(
        &self,
        process_type: DevProcessType,
        id: DevProcessId,
        name: String,
        command: String,
        working_dir: PathBuf,
        pid: u32,
        pgid: u32,
        cancel_hook: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Arc<DevProcessHandle> {
        let handle = Self::build_virtual_handle(
            id.clone(),
            name,
            process_type,
            command,
            working_dir,
            pid,
            pgid,
            cancel_hook,
            None,
        );

        self.track_process_ids(pid, pgid);

        {
            let mut lock = self.processes.write().await;
            lock.insert(id, Arc::clone(&handle));
        }
        self.cached_active_count.fetch_add(1, Ordering::Relaxed);
        self.trigger_background_telemetry_refresh();
        handle
    }

    /// Schedules a periodic background interval, recurring cron, or one-shot countdown task.
    pub async fn schedule(
        &self,
        workspace_root: &Path,
        req: ScheduleRequest,
    ) -> Result<DevProcessSummary> {
        let is_one_shot = req.is_one_shot();
        let interval_secs = req.effective_interval_secs().max(1);
        let process_type = if is_one_shot {
            DevProcessType::Timer
        } else {
            DevProcessType::Cron
        };

        let short_uuid = &Uuid::new_v4().to_string()[..8];
        let name_prefix = req
            .name
            .as_deref()
            .unwrap_or(if is_one_shot { "timer" } else { "cron" });
        let clean_prefix = name_prefix.trim().to_lowercase().replace(' ', "-");
        let id = DevProcessId::from(format!("sched-{}-{}", clean_prefix, short_uuid));
        let display_name = req.name.clone().unwrap_or_else(|| {
            if is_one_shot {
                format!("Timer ({}s)", interval_secs)
            } else {
                format!("Schedule (every {}s)", interval_secs)
            }
        });

        let now_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let next_epoch = now_epoch + interval_secs;

        let schedule_info = ScheduleInfo {
            interval_secs,
            is_one_shot,
            iteration_count: 0,
            max_iterations: req.max_iterations,
            last_run_timestamp: None,
            next_run_timestamp: Some(next_epoch),
        };

        let (abort_tx, mut abort_rx) = tokio::sync::watch::channel(false);
        let cancel_hook: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = abort_tx.send(true);
        });

        let handle = Self::build_virtual_handle(
            id.clone(),
            display_name.clone(),
            process_type,
            req.command.clone(),
            workspace_root.to_path_buf(),
            0,
            0,
            Some(cancel_hook),
            Some(schedule_info),
        );

        let handle_clone = Arc::clone(&handle);
        let work_dir_buf = workspace_root.to_path_buf();
        let cmd_str = req.command.clone();
        let max_iters = req.max_iterations;
        let cached_active = Arc::clone(&self.cached_active_count);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            // First tick fires immediately in tokio::time::interval; skip it to respect interval_secs delay
            ticker.tick().await;

            let mut current_iter = 0;
            loop {
                tokio::select! {
                    _ = abort_rx.changed() => {
                        if *abort_rx.borrow() {
                            break;
                        }
                    }
                    _ = ticker.tick() => {
                        if *abort_rx.borrow() || handle_clone.is_shutting_down.load(Ordering::SeqCst) {
                            break;
                        }
                        current_iter += 1;
                        let run_time = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        let next_time = if is_one_shot { None } else { Some(run_time + interval_secs) };
                        {
                            let mut info_lock = handle_clone.schedule_info.write().await;
                            if let Some(ref mut info) = *info_lock {
                                info.iteration_count = current_iter;
                                info.last_run_timestamp = Some(run_time);
                                info.next_run_timestamp = next_time;
                            }
                        }

                        let header = if is_one_shot {
                            format!("⏰ One-shot timer fired (duration: {}s): {}", interval_secs, cmd_str)
                        } else {
                            format!("⏰ Scheduled execution #{} (interval: {}s): {}", current_iter, interval_secs, cmd_str)
                        };
                        handle_clone.append_log(header).await;

                        // Execute command in background
                        let output = tokio::process::Command::new("sh")
                            .arg("-c")
                            .arg(&cmd_str)
                            .current_dir(&work_dir_buf)
                            .output()
                            .await;

                        match output {
                            Ok(out) => {
                                let stdout = String::from_utf8_lossy(&out.stdout);
                                for line in stdout.lines() {
                                    handle_clone.append_log(line).await;
                                }
                                let stderr = String::from_utf8_lossy(&out.stderr);
                                for line in stderr.lines() {
                                    handle_clone.append_log(format!("⚠ {}", line)).await;
                                }
                                if !out.status.success() {
                                    handle_clone.append_log(format!("✗ Process exited with code {:?}", out.status.code())).await;
                                }
                            }
                            Err(e) => {
                                handle_clone.append_log(format!("✗ Failed to execute scheduled command: {}", e)).await;
                            }
                        }

                        if is_one_shot {
                            handle_clone.append_log("✔ One-shot timer completed.").await;
                            {
                                let mut st = handle_clone.status.write().await;
                                *st = DevProcessStatus::Stopped;
                            }
                            decrement_atomic_saturating(&cached_active);
                            get_global_dev_registry().trigger_background_telemetry_refresh();
                            break;
                        }

                        if let Some(max) = max_iters {
                            if current_iter >= max {
                                handle_clone.append_log(format!("✔ Completed all {} scheduled iterations.", max)).await;
                                {
                                    let mut st = handle_clone.status.write().await;
                                    *st = DevProcessStatus::Stopped;
                                }
                                decrement_atomic_saturating(&cached_active);
                                get_global_dev_registry().trigger_background_telemetry_refresh();
                                break;
                            }
                        }
                    }
                }
            }
        });

        {
            let mut lock = self.processes.write().await;
            lock.insert(id, Arc::clone(&handle));
        }
        self.cached_active_count.fetch_add(1, Ordering::Relaxed);
        self.trigger_background_telemetry_refresh();
        let summary = self.create_summary(&handle).await;
        Ok(summary)
    }

    /// Updates an active PGID when a monitored process is auto-restarted with a new PID/PGID.
    pub fn update_active_pgid(&self, old_pgid: u32, new_pgid: u32) {
        if let Ok(mut set) = self.active_pgids.lock() {
            set.remove(&old_pgid);
            if new_pgid > 0 && new_pgid != std::process::id() {
                set.insert(new_pgid);
            }
        }
        if let Ok(mut set) = self.active_pids.lock() {
            set.remove(&old_pgid);
            if new_pgid > 0 && new_pgid != std::process::id() {
                set.insert(new_pgid);
            }
        }
    }

    /// Registers an external child PID (such as Chrome or a background subagent)
    /// to ensure it is terminated on minicode exit.
    pub fn register_external_pid(&self, pid: u32) {
        if pid > 1 && pid != std::process::id() {
            if let Ok(mut set) = self.active_pids.lock() {
                set.insert(pid);
            }
            if let Ok(mut browsers) = self.browser_pids.try_write() {
                browsers.insert(pid);
            }
        }
    }

    /// Unregisters an external child PID that completed its lifecycle cleanly.
    pub fn unregister_external_pid(&self, pid: u32) {
        if let Ok(mut set) = self.active_pids.lock() {
            set.remove(&pid);
        }
        if let Ok(mut set) = self.active_pgids.lock() {
            set.remove(&pid);
        }
        if let Ok(mut browsers) = self.browser_pids.try_write() {
            browsers.remove(&pid);
        }
    }

    /// Triggers a coalesced, non-blocking background telemetry refresh on the Tokio runtime
    /// so spawning/completing `MiniPub` workers or background tasks never stalls the caller's frame.
    pub fn trigger_background_telemetry_refresh(&self) {
        if self
            .telemetry_refresh_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(async move {
                let registry = get_global_dev_registry();
                let _ = registry.refresh_telemetry_cache().await;
                registry
                    .telemetry_refresh_in_flight
                    .store(false, Ordering::Release);
            });
        } else {
            self.telemetry_refresh_in_flight
                .store(false, Ordering::Release);
        }
    }

    /// Recomputes and synchronizes the lock-free atomic telemetry counters (`cached_active_count`,
    /// `cached_total_memory_mb`, `cached_total_cpu_pct`) with all live OS processes and in-process workers.
    pub async fn refresh_telemetry_cache(&self) -> RuntimeResourceSummary {
        self.resources().await
    }

    /// Gathers aggregate real-time resource telemetry across all managed processes and synchronizes
    /// the lock-free TUI telemetry cache.
    pub async fn resources(&self) -> RuntimeResourceSummary {
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };
        let my_pid = std::process::id();
        let mut pids = Vec::new();
        let mut in_proc_active = 0usize;
        let mut in_proc_cpu = 0.0f32;
        let mut in_proc_mem_mb = 0.0f32;

        for handle in &handles {
            let status = handle.status.read().await.clone();
            if !status.is_alive() {
                continue;
            }
            let pid = handle.pid();
            if pid == 0 || pid == my_pid {
                in_proc_active += 1;
                let u = Self::estimate_in_process_usage(handle).await;
                in_proc_cpu += u.cpu_percent;
                in_proc_mem_mb += u.memory_rss_mb;
            } else if crate::dev::ports::is_process_running(pid) {
                pids.push(pid);
            }
        }
        if crate::tools::browser::BrowserManager::is_live_engine_running().await {
            if let Some((_engine, pid, _port, _uptime)) =
                crate::tools::browser::BrowserManager::get_live_engine_details().await
            {
                if pid > 0 && crate::dev::ports::is_process_running(pid) {
                    pids.push(pid);
                }
            }
        }
        let mut agg = sample_aggregate_metrics(&pids);
        agg.total_active_processes += in_proc_active;
        agg.total_cpu_percent = ((agg.total_cpu_percent + in_proc_cpu) * 10.0).round() / 10.0;
        agg.total_memory_rss_mb =
            ((agg.total_memory_rss_mb + in_proc_mem_mb) * 10.0).round() / 10.0;

        self.cached_active_count
            .store(agg.total_active_processes as u32, Ordering::Relaxed);
        self.cached_total_memory_mb.store(
            (agg.total_memory_rss_mb * 10.0).round() as u32,
            Ordering::Relaxed,
        );
        self.cached_total_cpu_pct.store(
            (agg.total_cpu_percent * 10.0).round() as u32,
            Ordering::Relaxed,
        );

        agg
    }

    /// Computes the actual in-process heap & context memory footprint and proportional OS CPU usage
    /// for Tokio tasks (`MiniPub` workers, Swarm nodes, and Timers) that share `minicode`'s OS PID.
    async fn estimate_in_process_usage(
        handle: &DevProcessHandle,
    ) -> crate::dev::models::DevProcessResourceUsage {
        let log_bytes: usize = {
            let logs = handle.logs.read().await;
            std::mem::size_of_val(&*logs)
                + logs.capacity() * std::mem::size_of::<String>()
                + logs.iter().map(String::capacity).sum::<usize>()
        };
        let handle_bytes = std::mem::size_of_val(handle)
            + handle.id.as_str().len()
            + handle.name.capacity()
            + handle.command.capacity()
            + log_bytes;

        let id_str = handle.id.as_str();
        let sub_id = ["swarm-worker-", "worker-", "minipub-", "swarm-"]
            .into_iter()
            .find_map(|prefix| id_str.strip_prefix(prefix))
            .unwrap_or(id_str);

        let pool_metrics =
            crate::agent::subagent::try_get_global_subagent_pool().and_then(|pool| {
                let (worker_bytes, is_executing) = pool.estimate_worker_memory_bytes(sub_id)?;
                let active_workers = pool.active_worker_count().max(1) as f32;
                Some((worker_bytes, is_executing, active_workers))
            });

        let (extra_bytes, cpu_percent) = match pool_metrics {
            Some((worker_bytes, is_executing, active_workers)) => {
                let cpu = if is_executing {
                    let self_cpu = sample_process_metrics(std::process::id()).cpu_percent;
                    ((self_cpu / active_workers) * 10.0).round() / 10.0
                } else {
                    0.0
                };
                (worker_bytes, cpu)
            }
            None => (0, 0.0),
        };

        let total_bytes = handle_bytes + extra_bytes;
        let memory_rss_mb =
            (((total_bytes as f32) / (1024.0 * 1024.0) * 10.0).round() / 10.0).max(0.1);
        crate::dev::models::DevProcessResourceUsage {
            pid: Some(handle.pid()),
            cpu_percent,
            memory_rss_mb,
            open_ports: Vec::new(),
        }
    }

    /// Internal helper to assemble DevProcessSummary with live metrics.
    async fn create_summary(&self, handle: &DevProcessHandle) -> DevProcessSummary {
        let mut status = handle.status.read().await.clone();
        let mut ports = handle.ports.read().await.clone();
        let current_pid = handle.pid();
        let my_pid = std::process::id();
        let usage = if status.is_alive() {
            if current_pid == 0 || current_pid == my_pid {
                Self::estimate_in_process_usage(handle).await
            } else {
                sample_process_metrics(current_pid)
            }
        } else {
            Default::default()
        };

        if status.is_alive() && current_pid > 0 && current_pid != my_pid {
            for proc_port in &usage.open_ports {
                if !ports.contains(proc_port) {
                    ports.push(*proc_port);
                }
            }
            if !ports.is_empty() {
                let mut p_lock = handle.ports.write().await;
                for &p in &ports {
                    if !p_lock.contains(&p) {
                        p_lock.push(p);
                    }
                }
                if matches!(status, DevProcessStatus::Running) {
                    status = DevProcessStatus::Healthy;
                    *handle.status.write().await = DevProcessStatus::Healthy;
                }
            }
        }

        let primary_port = {
            let listening_ports: Vec<u16> = ports
                .iter()
                .copied()
                .filter(|&p| crate::dev::ports::is_port_listening(p))
                .collect();
            if listening_ports.len() == 1 {
                listening_ports.first().copied()
            } else if listening_ports.len() > 1 {
                let my_desc = crate::dev::ports::find_all_descendants(current_pid);
                let owned = listening_ports.iter().copied().find(|&p| {
                    if let Some(owner) = crate::dev::ports::find_pid_by_port(p) {
                        owner == current_pid || my_desc.contains(&owner)
                    } else {
                        false
                    }
                });
                owned.or_else(|| listening_ports.last().copied())
            } else {
                ports.last().copied()
            }
        };
        let url = primary_port.map(|p| {
            let host = crate::dev::ports::resolve_listening_loopback_host(p).unwrap_or("localhost");
            format!("http://{}:{}", host, p)
        });
        let restart_count = handle.restart_stats.read().await.restart_count;
        let port_resolution = handle.port_resolution.read().await.clone();

        DevProcessSummary {
            id: handle.id.clone(),
            name: handle.name.clone(),
            process_type: handle.process_type,
            status,
            pid: Some(current_pid),
            ports,
            url,
            cpu_percent: usage.cpu_percent,
            memory_rss_mb: usage.memory_rss_mb,
            uptime_secs: handle.uptime_secs(),
            restart_count,
            restart_policy: handle.restart_policy.clone(),
            port_resolution,
            schedule_info: handle.schedule_info.read().await.clone(),
        }
    }
}

/// Synchronously kills all tracked process groups, direct PIDs, and their descendant process trees
/// via direct OS signals. Safe to invoke from panic hooks, signal handlers, and Drop guards.
pub fn kill_all_sync() {
    let registry = get_global_dev_registry();
    let my_pid = std::process::id();
    #[cfg(unix)]
    let my_pgrp = unsafe { libc::getpgrp() };
    #[cfg(unix)]
    let my_sid = unsafe { libc::getsid(0) };

    // 1. Gather explicitly isolated process groups created by minicode
    let mut target_pgids = HashSet::new();
    if let Ok(mut set) = registry.active_pgids.lock() {
        for &pgid in set.iter() {
            #[cfg(unix)]
            if pgid > 1
                && pgid != my_pid
                && (pgid as i32) != my_pgrp
                && (pgid as i32) != my_sid
                && (crate::dev::ports::is_process_running(pgid)
                    || unsafe { libc::kill(-(pgid as i32), 0) == 0 })
            {
                target_pgids.insert(pgid);
            }
            #[cfg(not(unix))]
            if pgid > 1 && pgid != my_pid {
                target_pgids.insert(pgid);
            }
        }
        set.clear();
    }

    if let Ok(procs) = registry.processes.try_read() {
        for handle in procs.values() {
            if handle.finished_elapsed_secs.load(Ordering::Relaxed) != u64::MAX {
                continue;
            }
            let pgid = handle.pgid();
            #[cfg(unix)]
            if pgid > 1
                && pgid != my_pid
                && (pgid as i32) != my_pgrp
                && (pgid as i32) != my_sid
                && (crate::dev::ports::is_process_running(pgid)
                    || unsafe { libc::kill(-(pgid as i32), 0) == 0 })
            {
                target_pgids.insert(pgid);
            }
            #[cfg(not(unix))]
            if pgid > 1 && pgid != my_pid {
                target_pgids.insert(pgid);
            }
        }
    }

    // 2. Gather all direct child PIDs that are still running
    let mut target_pids = HashSet::new();

    if let Ok(mut set) = registry.active_pids.lock() {
        for &pid in set.iter() {
            if pid > 1 && pid != my_pid && crate::dev::ports::is_process_running(pid) {
                target_pids.insert(pid);
            }
        }
        set.clear();
    }

    if let Ok(mut browsers) = registry.browser_pids.try_write() {
        for &pid in browsers.iter() {
            if pid > 1 && pid != my_pid && crate::dev::ports::is_process_running(pid) {
                target_pids.insert(pid);
            }
        }
        browsers.clear();
    }

    if let Ok(procs) = registry.processes.try_read() {
        for handle in procs.values() {
            if handle.finished_elapsed_secs.load(Ordering::Relaxed) != u64::MAX {
                continue;
            }
            let pid = handle.pid();
            if pid > 1 && pid != my_pid && crate::dev::ports::is_process_running(pid) {
                target_pids.insert(pid);
            }
        }
    }

    if target_pgids.is_empty() && target_pids.is_empty() {
        #[cfg(unix)]
        unsafe {
            let mut status = 0;
            while libc::waitpid(-1, &mut status, libc::WNOHANG) > 0 {}
        }
        return;
    }

    #[cfg(unix)]
    {
        // Discover ALL descendant PIDs across all target root processes via /proc
        let mut all_descendants = HashSet::new();
        for &target in target_pids.iter().chain(target_pgids.iter()) {
            let desc = crate::dev::ports::find_all_descendants(target);
            for d in desc {
                if d > 1 && d != my_pid {
                    all_descendants.insert(d);
                }
            }
        }

        // Phase 1: Graceful SIGTERM to verified process groups (-pgid) and direct PIDs (pid)
        unsafe {
            for &pgid in &target_pgids {
                let _ = libc::kill(-(pgid as i32), libc::SIGTERM);
                let _ = libc::kill(pgid as i32, libc::SIGTERM);
            }
            for &p in &target_pids {
                let _ = libc::kill(p as i32, libc::SIGTERM);
            }
            for &d in &all_descendants {
                let _ = libc::kill(d as i32, libc::SIGTERM);
            }
        }

        // Check if any target process or descendant is still alive after SIGTERM
        let any_alive = || {
            target_pids
                .iter()
                .chain(target_pgids.iter())
                .chain(all_descendants.iter())
                .any(|&p| crate::dev::ports::is_process_running(p))
        };

        if any_alive() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        if any_alive() {
            // Phase 2: Forceful SIGKILL to verified process groups (-pgid) and direct PIDs (pid)
            unsafe {
                for &pgid in &target_pgids {
                    let _ = libc::kill(-(pgid as i32), libc::SIGKILL);
                    let _ = libc::kill(pgid as i32, libc::SIGKILL);
                }
                for &p in &target_pids {
                    let _ = libc::kill(p as i32, libc::SIGKILL);
                }
                for &d in &all_descendants {
                    let _ = libc::kill(d as i32, libc::SIGKILL);
                }
            }

            // Final sweep: catch any late children
            for &target in target_pids.iter().chain(target_pgids.iter()) {
                let late_desc = crate::dev::ports::find_all_descendants(target);
                for ld in late_desc {
                    if ld > 1 && ld != my_pid {
                        unsafe {
                            let _ = libc::kill(ld as i32, libc::SIGKILL);
                        }
                    }
                }
            }
        }

        // Reap any direct child processes that were terminated to release them from zombie state
        unsafe {
            let mut status = 0;
            while libc::waitpid(-1, &mut status, libc::WNOHANG) > 0 {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_registry_crud_lifecycle() {
        let registry = MiniDevRegistry::new();
        let temp = tempdir().unwrap();

        // 1. Spawn a test job
        let req = SpawnDevRequest {
            command: "echo 'API ready on http://localhost:8999'; sleep 30".to_string(),
            name: Some("test-api".to_string()),
            process_type: DevProcessType::Backend,
            working_dir: None,
            extra_env: HashMap::new(),
            port_hint: None,
            max_memory_mb: None,
            port_policy: None,
            restart_policy: None,
        };

        let summary = registry.spawn(temp.path(), req).await.unwrap();
        assert_eq!(summary.name, "test-api");
        assert_eq!(summary.process_type, DevProcessType::Backend);

        // Wait brief moment for stdout port discovery
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        // 2. List jobs
        let list = registry.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].ports, vec![8999]);
        assert_eq!(list[0].url, Some("http://localhost:8999".to_string()));

        // 3. Query logs
        let logs = registry.logs(&summary.id, 10, None).await.unwrap();
        assert!(logs.iter().any(|l| l.contains("API ready")));

        // 4. Resources
        let res = registry.resources().await;
        assert_eq!(res.total_active_processes, 1);

        // 5. Kill all
        let killed = registry.kill_all().await.unwrap();
        assert_eq!(killed, 1);

        // Verify status after kill
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let updated = registry.get(&summary.id).await.unwrap();
        assert!(!updated.status.is_alive());
    }

    #[tokio::test]
    async fn test_register_worker_lifecycle() {
        let registry = MiniDevRegistry::new();
        let temp = tempdir().unwrap();

        let cancel_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel_called);

        let dev_id = DevProcessId::from("worker-subagent-coder-1");
        let handle = registry
            .register_worker(
                dev_id.clone(),
                "Subagent (Coder) - Task 1".to_string(),
                "minicode run -d /tmp".to_string(),
                temp.path().to_path_buf(),
                std::process::id(),
                0,
                Some(Arc::new(move || {
                    cancel_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                })),
            )
            .await;

        handle.append_log("Worker spawned in worktree").await;
        handle.append_log("Running tests...").await;

        // Verify list_workers includes it
        let workers = registry.list_workers().await;
        assert_eq!(workers.len(), 1);
        assert_eq!(workers[0].process_type, DevProcessType::Worker);
        assert_eq!(workers[0].name, "Subagent (Coder) - Task 1");

        // Verify flexible get without worker- prefix
        let raw_id = DevProcessId::from("subagent-coder-1");
        let summary = registry.get(&raw_id).await.expect("found by raw id");
        assert_eq!(summary.id, dev_id);

        // Verify logs
        let logs = registry.logs(&raw_id, 10, None).await.unwrap();
        assert_eq!(logs.len(), 2);
        assert!(logs[0].contains("Worker spawned"));

        // Verify stop invokes cancel hook
        registry.stop(&raw_id).await.unwrap();
        assert!(cancel_called.load(std::sync::atomic::Ordering::SeqCst));

        let updated = registry.get(&dev_id).await.unwrap();
        assert_eq!(updated.status, DevProcessStatus::Stopped);
    }

    #[tokio::test]
    async fn test_restart_lifecycle_preserves_id_and_env() {
        use crate::dev::models::SpawnDevRequest;

        let registry = MiniDevRegistry::new();
        let temp = tempdir().unwrap();

        let mut env = HashMap::new();
        env.insert("CUSTOM_KEY".to_string(), "CUSTOM_VAL_123".to_string());

        let req = SpawnDevRequest {
            command: "echo 'Server started'; sleep 60".to_string(),
            name: Some("resilient-service".to_string()),
            process_type: DevProcessType::Backend,
            working_dir: None,
            extra_env: env,
            port_hint: None,
            max_memory_mb: None,
            port_policy: None,
            restart_policy: None,
        };

        let summary1 = registry.spawn(temp.path(), req).await.expect("spawn ok");
        let orig_id = summary1.id.clone();
        let orig_pid = summary1.pid.expect("pid ok");

        // Give process a moment to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Restart process
        let summary2 = registry
            .restart(temp.path(), &orig_id)
            .await
            .expect("restart ok");

        // Invariant 1: DevProcessId must be strictly preserved across restarts
        assert_eq!(
            summary2.id, orig_id,
            "DevProcessId must match canonical original ID"
        );
        assert_ne!(
            summary2.pid,
            Some(orig_pid),
            "New PID must be assigned on restart"
        );

        // Invariant 2: Subsequent lookup by original ID must succeed
        let queried = registry
            .get(&orig_id)
            .await
            .expect("must find restarted process by original ID");
        assert_eq!(queried.id, orig_id);

        // Invariant 3: Environment variables must be preserved in handle
        {
            let lock = registry.processes.read().await;
            let handle = lock.get(&orig_id).expect("handle exists");
            assert_eq!(
                handle.extra_env.get("CUSTOM_KEY").map(|s| s.as_str()),
                Some("CUSTOM_VAL_123"),
                "Environment variable must survive restart"
            );
        }

        // Invariant 4: Stop by original ID must cleanly terminate new instance
        registry.stop(&orig_id).await.expect("stop ok");
        let stopped = registry.get(&orig_id).await.expect("exists");
        assert_eq!(stopped.status, DevProcessStatus::Stopped);
    }

    #[tokio::test]
    async fn test_schedule_interval_execution_and_cancellation() {
        let registry = MiniDevRegistry::new();
        let temp = tempdir().expect("tempdir");

        let req = ScheduleRequest {
            command: "echo 'tick_interval'".to_string(),
            name: Some("test-timer".to_string()),
            interval_seconds: Some(1),
            duration_seconds: None,
            cron_expression: None,
            max_iterations: Some(3),
        };

        let summary = registry.schedule(temp.path(), req).await.expect("schedule");
        assert_eq!(summary.name, "test-timer");
        assert!(summary.id.as_str().starts_with("sched-test-timer-"));
        assert_eq!(summary.process_type, DevProcessType::Cron);
        assert!(summary.schedule_info.is_some());
        let s_info = summary.schedule_info.unwrap();
        assert_eq!(s_info.interval_secs, 1);
        assert!(!s_info.is_one_shot);

        // Sleep to allow tick to fire
        tokio::time::sleep(Duration::from_millis(1300)).await;

        let logs = registry.logs(&summary.id, 10, None).await.expect("logs");
        assert!(
            logs.iter()
                .any(|l| l.contains("tick_interval") || l.contains("Scheduled execution")),
            "Logs must contain scheduled tick output: {:?}",
            logs
        );

        // Stop scheduled task early
        let stopped = registry.stop(&summary.id).await.expect("stop");
        assert!(stopped);
        let updated = registry.get(&summary.id).await.expect("exists");
        assert_eq!(updated.status, DevProcessStatus::Stopped);
    }

    #[tokio::test]
    async fn test_schedule_one_shot_timer() {
        let registry = MiniDevRegistry::new();
        let temp = tempdir().expect("tempdir");

        let req = ScheduleRequest {
            command: "echo 'timer_done'".to_string(),
            name: Some("test-one-shot".to_string()),
            interval_seconds: None,
            duration_seconds: Some(1),
            cron_expression: None,
            max_iterations: None,
        };

        let summary = registry.schedule(temp.path(), req).await.expect("schedule");
        assert_eq!(summary.process_type, DevProcessType::Timer);
        let s_info = summary.schedule_info.expect("schedule_info");
        assert!(s_info.is_one_shot);

        // Sleep to allow one-shot timer to fire and complete
        tokio::time::sleep(Duration::from_millis(1400)).await;

        let logs = registry.logs(&summary.id, 10, None).await.expect("logs");
        assert!(
            logs.iter()
                .any(|l| l.contains("timer_done") || l.contains("One-shot timer")),
            "Logs must contain one-shot output: {:?}",
            logs
        );

        let updated = registry.get(&summary.id).await.expect("exists");
        assert_eq!(updated.status, DevProcessStatus::Stopped);
    }

    #[tokio::test]
    async fn test_register_and_list_swarm_process() {
        let registry = MiniDevRegistry::new();
        let handle = registry
            .register_swarm_process(
                DevProcessId::from("swarm-test-123"),
                "Swarm Test DAG".to_string(),
                "minicode swarm run test".to_string(),
                PathBuf::from("/tmp"),
                std::process::id(),
                0,
                None,
            )
            .await;

        handle.append_log("Initializing swarm worker 1").await;
        handle.append_log("Worker 1 completed successfully").await;

        let swarms = registry.list_swarms().await;
        assert_eq!(swarms.len(), 1);
        assert_eq!(swarms[0].id.as_str(), "swarm-test-123");
        assert_eq!(swarms[0].process_type, DevProcessType::Swarm);

        let logs = registry
            .logs(&DevProcessId::from("swarm-test-123"), 10, None)
            .await
            .expect("logs");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0], "Initializing swarm worker 1");

        handle.update_status(DevProcessStatus::Stopped).await;
        let summary = registry
            .get(&DevProcessId::from("swarm-test-123"))
            .await
            .expect("summary");
        assert_eq!(summary.status, DevProcessStatus::Stopped);
    }

    #[tokio::test]
    async fn test_non_daemon_background_task_stdin_await_and_harvest() {
        let registry = MiniDevRegistry::new();
        let temp = tempdir().expect("tempdir");

        let req = SpawnDevRequest {
            command: "read line_in; echo \"built:$line_in\"".to_string(),
            name: Some("bun-build-task".to_string()),
            process_type: DevProcessType::Script,
            working_dir: None,
            extra_env: HashMap::new(),
            port_hint: None,
            max_memory_mb: None,
            port_policy: None,
            restart_policy: None,
        };

        let (summary, handle) = registry
            .spawn_with_options(temp.path(), req, false)
            .await
            .expect("spawn non-daemon task");
        assert!(!handle.is_daemon);
        assert!(handle.log_path.is_some());
        assert_eq!(registry.active_non_daemon_task_count().await, 1);

        // Send interactive input to the running background task
        registry
            .send_input(&summary.id, "bundle-v1")
            .await
            .expect("send_input");

        // Wait for task to settle
        registry
            .wait_until_active_tasks_settle(Duration::from_secs(3))
            .await;
        assert_eq!(registry.active_non_daemon_task_count().await, 0);

        // Harvest newly completed task notification block
        let (ids, block) = registry
            .harvest_newly_completed_tasks_block(temp.path())
            .await
            .expect("harvest completed task");
        assert_eq!(ids, vec![summary.id.as_str().to_string()]);
        assert!(block.contains("<completed_background_tasks>"));
        assert!(block.contains("built:bundle-v1"));

        // Second harvest should be None since it was already reported
        assert!(registry
            .harvest_newly_completed_tasks_block(temp.path())
            .await
            .is_none());

        // Disk log file should also exist and contain the output
        let log_path = handle.log_path.as_ref().expect("log path");
        let disk_log = std::fs::read_to_string(log_path).expect("read disk log");
        assert!(disk_log.contains("built:bundle-v1"));
    }

    #[tokio::test]
    async fn test_live_steering_prompt_wakes_active_minitask_barrier_without_killing_task() {
        let registry = Arc::new(MiniDevRegistry::new());
        let temp = tempdir().expect("tempdir");

        let req = SpawnDevRequest {
            command: "sleep 0.6; echo \"bg-build-complete\"".to_string(),
            name: Some("cargo-build".to_string()),
            process_type: DevProcessType::Script,
            working_dir: None,
            extra_env: HashMap::new(),
            port_hint: None,
            max_memory_mb: None,
            port_policy: None,
            restart_policy: None,
        };

        let (_summary, _handle) = registry
            .spawn_with_options(temp.path(), req, false)
            .await
            .expect("spawn non-daemon task");
        assert_eq!(registry.active_non_daemon_task_count().await, 1);

        let reg_clone = Arc::clone(&registry);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            reg_clone.push_steering_prompt("what is the next step in todo.md?".to_string());
        });

        let wait_start = std::time::Instant::now();
        registry
            .wait_until_active_tasks_settle(Duration::from_secs(5))
            .await;
        // Must wake up early when the live steering prompt arrives (~80ms) while the 600ms task is still running
        assert!(wait_start.elapsed() < Duration::from_millis(450));
        assert!(registry.is_continuation_armed());
        assert_eq!(registry.active_non_daemon_task_count().await, 1);

        let active_ctx = registry.format_active_tasks_context().await;
        assert!(active_ctx.contains("<active_minitask_processes>"));
        assert!(active_ctx.contains("cargo-build"));

        let drained = registry.drain_steering_prompts();
        assert_eq!(
            drained,
            vec!["what is the next step in todo.md?".to_string()]
        );
        assert!(!registry.has_steering_prompts());

        // Now wait for the background task to actually finish and verify continuation harvest
        registry
            .wait_until_active_tasks_settle(Duration::from_secs(3))
            .await;
        assert_eq!(registry.active_non_daemon_task_count().await, 0);
        assert!(registry.has_unreported_completed_tasks().await);
        let (_ids, block) = registry
            .harvest_newly_completed_tasks_block(temp.path())
            .await
            .expect("harvest after completion");
        assert!(block.contains("bg-build-complete"));
    }

    #[tokio::test]
    async fn test_exec_managed_background_task_detaches_on_steering_and_fast_kill_all_sync() {
        let temp = tempdir().expect("tempdir");
        let global_reg = get_global_dev_registry();
        let _ = global_reg.drain_steering_prompts();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(60)).await;
            get_global_dev_registry()
                .push_steering_prompt("status of current bun build?".to_string());
        });

        let start = std::time::Instant::now();
        let out = crate::tools::exec::exec_managed_background_task(
            temp.path(),
            "sleep 0.5; echo 'bun-build-ok'",
            5_000,
            None,
        )
        .await
        .expect("exec_managed_background_task");

        assert!(
            start.elapsed() < Duration::from_millis(400),
            "Should detach immediately on live steering prompt without waiting 5s"
        );
        assert!(out.contains("Live User Prompt Received"));
        let _ = global_reg.drain_steering_prompts();

        // Wait for the background task to complete naturally
        global_reg
            .wait_until_active_tasks_settle(Duration::from_secs(3))
            .await;

        // Verify kill_all_sync() is instantaneous (<20ms) when no processes are running
        let shutdown_start = std::time::Instant::now();
        kill_all_sync();
        assert!(
            shutdown_start.elapsed() < Duration::from_millis(20),
            "kill_all_sync() must be instantaneous when no background processes remain alive"
        );
    }
}
