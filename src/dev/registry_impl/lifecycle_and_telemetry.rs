//! Process lifecycle operations (stop, restart, kill_all, worker/swarm registration, scheduling),
//! telemetry synthesis, and synchronous OS process tree cleanup for [`MiniDevRegistry`].

use super::*;
use crate::dev::metrics::{sample_aggregate_metrics, sample_process_metrics};
use crate::dev::models::{
    DevProcessId, DevProcessStatus, DevProcessSummary, DevProcessType, RuntimeResourceSummary,
    ScheduleInfo, ScheduleRequest, SpawnDevRequest,
};
use crate::dev::process::DevProcessHandle;
use crate::error::{DevError, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use uuid::Uuid;

impl MiniDevRegistry {
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

        if handle.process_type == DevProcessType::Worker {
            let raw_id = handle
                .id
                .as_str()
                .strip_prefix("worker-")
                .unwrap_or(handle.id.as_str());
            if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                let _ = pool.kill_subagent(raw_id).await;
            }
        }

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

    /// Terminates ALL active managed processes, containers, and browser instances unconditionally.
    /// Used by explicit `kill_all` commands and OS termination signal handlers (`SIGINT`/`SIGTERM`/`SIGHUP`).
    pub async fn kill_all(&self) -> Result<usize> {
        let _ = crate::tools::browser::BrowserManager::shutdown_live_engine_opt(true).await;
        self.terminate_managed_processes(false).await
    }

    /// Graceful normal-exit teardown for CLI/TUI session completion.
    ///
    /// When an interactive GUI browser window was launched and preserved for the user (`BrowserMode::Gui`),
    /// detaches active `Frontend` / `Backend` daemon servers from teardown tracking so the live localhost
    /// preview remains reachable in the user's open browser window, while still reaping all non-daemon
    /// tasks, workers, swarms, headless browsers, and containers.
    pub async fn shutdown_for_exit(&self) -> Result<usize> {
        let preserve_gui_session = matches!(
            crate::tools::browser::BrowserManager::live_engine_mode().await,
            Some(crate::tools::browser::BrowserMode::Gui)
        );
        let _ = crate::tools::browser::BrowserManager::shutdown_live_engine().await;
        self.terminate_managed_processes(preserve_gui_session).await
    }

    async fn terminate_managed_processes(&self, preserve_gui_daemons: bool) -> Result<usize> {
        if preserve_gui_daemons {
            let mut lock = self.processes.write().await;
            let mut preserved_ids = Vec::new();
            for (id, handle) in lock.iter() {
                let alive = handle.status.read().await.is_alive();
                if alive
                    && handle.is_daemon
                    && matches!(
                        handle.process_type,
                        DevProcessType::Frontend | DevProcessType::Backend
                    )
                {
                    self.untrack_process_ids(handle.pid(), handle.pgid());
                    preserved_ids.push(id.clone());
                }
            }
            for id in preserved_ids {
                tracing::info!(
                    id = %id,
                    "Preserving detached dev server daemon on exit to back open GUI browser session"
                );
                lock.remove(&id);
            }
            let mut browsers = self.browser_pids.write().await;
            browsers.clear();
        }

        // Concurrently terminate all remaining managed process handles
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

        // Clean up any registered docker containers
        let dock_lock = self.docker_containers.read().await;
        for container_name in dock_lock.iter() {
            let _ = tokio::process::Command::new("docker")
                .args(["stop", container_name])
                .output()
                .await;
        }

        // Force synchronous sweep of all remaining non-preserved processes, groups, descendants & children
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
    pub(super) async fn estimate_in_process_usage(
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
    pub(super) async fn create_summary(&self, handle: &DevProcessHandle) -> DevProcessSummary {
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
