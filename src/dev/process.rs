//! Isolated process group management, death-pipe sentinels, ring buffers, and graceful teardown.

use crate::dev::models::{
    DevProcessId, DevProcessStatus, DevProcessType, PortConflictPolicy, PortResolution,
    RestartPolicy, RestartStats, SpawnDevRequest,
};
use crate::dev::ports::{
    arbitrate_port, detect_requested_port, rewrite_command_port, scan_ports_from_output,
};
use crate::error::{DevError, Result};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;

/// Maximum number of log lines retained in the in-memory ring buffer per process.
pub const MAX_RING_BUFFER_LINES: usize = crate::constants::DEFAULT_DEV_LOG_RING_BUFFER_SIZE;

/// Thread-safe cancellation hook invoked when a process or worker handle is terminated.
#[derive(Clone)]
pub struct CancelHook(pub Arc<dyn Fn() + Send + Sync>);

impl CancelHook {
    pub fn new(f: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self(f)
    }
}

impl std::fmt::Debug for CancelHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CancelHook")
    }
}

/// Live handle to a background managed process.
#[derive(Debug)]
pub struct DevProcessHandle {
    pub id: DevProcessId,
    pub name: String,
    pub process_type: DevProcessType,
    pub command: String,
    pub working_dir: PathBuf,
    pub extra_env: HashMap<String, String>,
    pub port_policy: Option<PortConflictPolicy>,
    pub max_memory_mb: Option<u64>,
    pub pid: Arc<AtomicU32>,
    pub pgid: Arc<AtomicU32>,
    pub started_at: Instant,
    pub status: Arc<RwLock<DevProcessStatus>>,
    pub ports: Arc<RwLock<Vec<u16>>>,
    pub logs: Arc<RwLock<VecDeque<String>>>,
    pub is_shutting_down: Arc<AtomicBool>,
    pub cancel_hook: Option<CancelHook>,
    pub restart_policy: RestartPolicy,
    pub restart_stats: Arc<RwLock<RestartStats>>,
    pub port_resolution: Arc<RwLock<Option<PortResolution>>>,
}

impl DevProcessHandle {
    /// Returns the live operating system process ID.
    pub fn pid(&self) -> u32 {
        self.pid.load(Ordering::SeqCst)
    }

    /// Returns the live process group ID.
    pub fn pgid(&self) -> u32 {
        self.pgid.load(Ordering::SeqCst)
    }

    /// Returns the uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Reads up to `tail` recent log lines from the ring buffer, optionally matching a filter.
    pub async fn get_logs(&self, tail: usize, filter: Option<&str>) -> Vec<String> {
        let lock = self.logs.read().await;
        let mut lines: Vec<String> = if let Some(q) = filter {
            let q_lower = q.to_lowercase();
            lock.iter()
                .filter(|l| l.to_lowercase().contains(&q_lower))
                .cloned()
                .collect()
        } else {
            lock.iter().cloned().collect()
        };

        if lines.len() > tail {
            lines.drain(0..(lines.len() - tail));
        }
        lines
    }

    /// Appends a log line into the in-memory ring buffer.
    pub async fn append_log(&self, line: impl Into<String>) {
        let mut lock = self.logs.write().await;
        if lock.len() >= MAX_RING_BUFFER_LINES {
            lock.pop_front();
        }
        lock.push_back(line.into());
    }

    /// Updates the process lifecycle status.
    pub async fn update_status(&self, new_status: DevProcessStatus) {
        let mut lock = self.status.write().await;
        *lock = new_status;
    }

    /// Terminates the process group cleanly with optional cancel hook, SIGTERM followed by SIGKILL fallback.
    pub async fn terminate(&self) -> Result<()> {
        self.is_shutting_down.store(true, Ordering::SeqCst);
        if let Some(ref hook) = self.cancel_hook {
            (hook.0)();
        }
        let mut status_lock = self.status.write().await;
        if !status_lock.is_alive() {
            return Ok(());
        }

        let current_pgid = self.pgid();
        if current_pgid > 0 && current_pgid != std::process::id() {
            terminate_process_group(current_pgid).await?;
        }
        *status_lock = DevProcessStatus::Stopped;
        Ok(())
    }
}

/// Helper function to configure a standard tokio Command for process group isolation.
fn create_tokio_command(
    command_str: &str,
    work_dir: &Path,
    extra_env: &std::collections::HashMap<String, String>,
) -> tokio::process::Command {
    let mut std_cmd = std::process::Command::new("sh");
    std_cmd.arg("-c").arg(command_str);
    std_cmd.current_dir(work_dir);

    for (k, v) in extra_env {
        std_cmd.env(k, v);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            std_cmd.pre_exec(|| {
                // 1. Create a new process group so this child is the group leader (PGID == PID)
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                // 2. On Linux, ensure the child receives SIGTERM if parent minicode dies
                #[cfg(target_os = "linux")]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }

                Ok(())
            });
        }
    }

    let mut tokio_cmd = tokio::process::Command::from(std_cmd);
    tokio_cmd.kill_on_drop(false); // Managed explicitly via process group
    tokio_cmd.stdout(std::process::Stdio::piped());
    tokio_cmd.stderr(std::process::Stdio::piped());
    tokio_cmd
}

/// Spawns a command inside a dedicated isolated process group with streaming output ring buffers,
/// port conflict arbitration, and auto-restart watchdog supervision.
pub async fn spawn_process_group(
    workspace_root: &Path,
    req: SpawnDevRequest,
) -> Result<DevProcessHandle> {
    spawn_process_group_with_id(workspace_root, req, None).await
}

/// Spawns a command inside an isolated process group, optionally preserving an existing DevProcessId across restarts.
pub async fn spawn_process_group_with_id(
    workspace_root: &Path,
    mut req: SpawnDevRequest,
    explicit_id: Option<DevProcessId>,
) -> Result<DevProcessHandle> {
    let dev_id = explicit_id.unwrap_or_else(|| DevProcessId::new(req.name.as_deref()));
    let display_name = req
        .name
        .clone()
        .unwrap_or_else(|| format!("{}-{}", req.process_type, dev_id));

    let work_dir = if let Some(ref rel) = req.working_dir {
        workspace_root.join(rel)
    } else {
        workspace_root.to_path_buf()
    };

    if !work_dir.is_dir() {
        return Err(DevError::SpawnFailed(format!(
            "Working directory '{}' does not exist",
            work_dir.display()
        ))
        .into());
    }

    // 1. Port conflict arbitration
    let port_policy = req.port_policy.unwrap_or_default();
    let port_resolution = if let Some(requested_port) = detect_requested_port(&req) {
        let res = arbitrate_port(requested_port, port_policy).await?;
        match &res {
            PortResolution::Shifted {
                requested,
                resolved,
                ..
            } => {
                req.extra_env
                    .insert("PORT".to_string(), resolved.to_string());
                req.command = rewrite_command_port(&req.command, *requested, *resolved);
                req.port_hint = Some(*resolved);
            }
            PortResolution::Reclaimed { port, .. } => {
                req.port_hint = Some(*port);
            }
            PortResolution::Unchanged { port } | PortResolution::Ignored { port } => {
                req.port_hint = Some(*port);
            }
        }
        Some(res)
    } else {
        None
    };

    // 2. Initial spawn
    let mut tokio_cmd = create_tokio_command(&req.command, &work_dir, &req.extra_env);
    let child = tokio_cmd
        .spawn()
        .map_err(|e| DevError::SpawnFailed(format!("Failed to spawn child process: {}", e)))?;

    let child_pid = child
        .id()
        .ok_or_else(|| DevError::SpawnFailed("Failed to acquire child PID".to_string()))?;
    let pgid = child_pid; // Since setpgid(0, 0) was called in pre_exec

    let pid_atomic = Arc::new(AtomicU32::new(child_pid));
    let pgid_atomic = Arc::new(AtomicU32::new(pgid));
    let status = Arc::new(RwLock::new(DevProcessStatus::Running));
    let ports = Arc::new(RwLock::new(Vec::new()));
    if let Some(hint) = req.port_hint {
        ports.write().await.push(hint);
    }

    let logs = Arc::new(RwLock::new(VecDeque::with_capacity(MAX_RING_BUFFER_LINES)));
    let is_shutting_down = Arc::new(AtomicBool::new(false));
    let restart_stats = Arc::new(RwLock::new(RestartStats::default()));
    let restart_policy = req.restart_policy.clone().unwrap_or_default();
    let port_res_lock = Arc::new(RwLock::new(port_resolution.clone()));

    // Log port resolution note if shifted
    if let Some(PortResolution::Shifted {
        requested,
        resolved,
        ref conflict,
    }) = port_resolution
    {
        let pid_str = conflict
            .conflicting_pid
            .map(|p| format!("PID {}", p))
            .unwrap_or_else(|| "unknown process".to_string());
        let name_str = conflict.process_name.as_deref().unwrap_or("unknown");
        let msg = format!(
            "[PORT_ARBITRATOR] Port {} was occupied by {} ({}). Automatically shifted to port {} (PORT={}).",
            requested, pid_str, name_str, resolved, resolved
        );
        logs.write().await.push_back(msg);
    }

    // 3. Supervisor & IO monitor loop with watchdog auto-restart
    let restart_policy_clone = restart_policy.clone();
    let command_clone = req.command.clone();
    let work_dir_clone = work_dir.clone();
    let extra_env_clone = req.extra_env.clone();
    let logs_clone = Arc::clone(&logs);
    let ports_clone = Arc::clone(&ports);
    let status_clone = Arc::clone(&status);
    let is_shutting_down_clone = Arc::clone(&is_shutting_down);
    let pid_clone = Arc::clone(&pid_atomic);
    let pgid_clone = Arc::clone(&pgid_atomic);
    let stats_clone = Arc::clone(&restart_stats);

    tokio::spawn(async move {
        let mut current_child = child;
        let mut spawn_time = Instant::now();

        loop {
            // Stream stdout
            if let Some(stdout) = current_child.stdout.take() {
                let logs_c = Arc::clone(&logs_clone);
                let ports_c = Arc::clone(&ports_clone);
                let status_c = Arc::clone(&status_clone);
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        {
                            let mut lock = logs_c.write().await;
                            if lock.len() >= MAX_RING_BUFFER_LINES {
                                lock.pop_front();
                            }
                            lock.push_back(line.clone());
                        }

                        let discovered = scan_ports_from_output(&line);
                        if !discovered.is_empty() {
                            let mut p_lock = ports_c.write().await;
                            for p in discovered {
                                if !p_lock.contains(&p) {
                                    p_lock.push(p);
                                }
                            }
                            *status_c.write().await = DevProcessStatus::Healthy;
                        }
                    }
                });
            }

            // Stream stderr
            if let Some(stderr) = current_child.stderr.take() {
                let logs_c = Arc::clone(&logs_clone);
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        let mut lock = logs_c.write().await;
                        if lock.len() >= MAX_RING_BUFFER_LINES {
                            lock.pop_front();
                        }
                        lock.push_back(format!("[STDERR] {}", line));
                    }
                });
            }

            // Wait for child process to exit
            let exit_result = current_child.wait().await;

            if is_shutting_down_clone.load(Ordering::SeqCst) {
                *status_clone.write().await = DevProcessStatus::Stopped;
                break;
            }

            let exit_code = match exit_result {
                Ok(status) => status.code(),
                Err(_) => None,
            };

            let elapsed = spawn_time.elapsed();
            let is_fast_crash =
                elapsed < Duration::from_secs(crate::constants::DEFAULT_WATCHDOG_FAST_CRASH_SECS);

            let mut stats = stats_clone.write().await;
            if is_fast_crash {
                stats.consecutive_fast_crashes += 1;
            } else {
                stats.consecutive_fast_crashes = 0;
            }

            // Crash loop protection: rapid crashes in succession stops auto-restart
            if stats.consecutive_fast_crashes
                >= crate::constants::DEFAULT_WATCHDOG_RAPID_CRASH_LIMIT
            {
                let msg = format!(
                    "[WATCHDOG] Crash loop detected: {} rapid crashes within {}s of spawn (last exit code: {:?}). Auto-restart disabled.",
                    crate::constants::DEFAULT_WATCHDOG_RAPID_CRASH_LIMIT,
                    crate::constants::DEFAULT_WATCHDOG_FAST_CRASH_SECS,
                    exit_code
                );
                {
                    let mut l_lock = logs_clone.write().await;
                    if l_lock.len() >= MAX_RING_BUFFER_LINES {
                        l_lock.pop_front();
                    }
                    l_lock.push_back(msg);
                }
                *status_clone.write().await = DevProcessStatus::Degraded(format!(
                    "Crash loop detected ({} rapid failures)",
                    crate::constants::DEFAULT_WATCHDOG_RAPID_CRASH_LIMIT
                ));
                break;
            }

            // Check if restart is triggered
            if restart_policy_clone.should_restart(exit_code)
                && stats.restart_count < restart_policy_clone.max_retries()
            {
                stats.restart_count += 1;
                stats.last_crash_reason = Some(format!("Exit code {:?}", exit_code));
                let attempt = stats.restart_count;
                let max = restart_policy_clone.max_retries();
                let base_backoff = restart_policy_clone.backoff_ms();
                let backoff = base_backoff.saturating_mul(1 << (attempt.saturating_sub(1)).min(4));
                drop(stats);

                let warn_msg = format!(
                    "[WATCHDOG] Process crashed (exit {:?}). Restarting (attempt {}/{}) in {}ms...",
                    exit_code, attempt, max, backoff
                );
                {
                    let mut l_lock = logs_clone.write().await;
                    if l_lock.len() >= MAX_RING_BUFFER_LINES {
                        l_lock.pop_front();
                    }
                    l_lock.push_back(warn_msg);
                }
                *status_clone.write().await =
                    DevProcessStatus::Degraded(format!("Restarting ({}/{})...", attempt, max));

                tokio::time::sleep(Duration::from_millis(backoff)).await;

                if is_shutting_down_clone.load(Ordering::SeqCst) {
                    *status_clone.write().await = DevProcessStatus::Stopped;
                    break;
                }

                // Spawn fresh child instance
                let mut next_cmd =
                    create_tokio_command(&command_clone, &work_dir_clone, &extra_env_clone);
                match next_cmd.spawn() {
                    Ok(new_child) => {
                        if let Some(new_pid) = new_child.id() {
                            let old_pgid = pgid_clone.load(Ordering::SeqCst);
                            let new_pgid = new_pid;
                            pid_clone.store(new_pid, Ordering::SeqCst);
                            pgid_clone.store(new_pgid, Ordering::SeqCst);

                            // Update registry active_pgids
                            let registry = crate::dev::registry::get_global_dev_registry();
                            registry.update_active_pgid(old_pgid, new_pgid);

                            spawn_time = Instant::now();
                            current_child = new_child;
                            *status_clone.write().await = DevProcessStatus::Running;

                            let restart_msg = format!(
                                "[WATCHDOG] Process respawned successfully with new PID {}.",
                                new_pid
                            );
                            {
                                let mut l_lock = logs_clone.write().await;
                                if l_lock.len() >= MAX_RING_BUFFER_LINES {
                                    l_lock.pop_front();
                                }
                                l_lock.push_back(restart_msg);
                            }
                            continue;
                        } else {
                            *status_clone.write().await = DevProcessStatus::Degraded(
                                "Failed to acquire new PID during restart".to_string(),
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("[WATCHDOG] Failed to respawn process: {}", e);
                        {
                            let mut l_lock = logs_clone.write().await;
                            if l_lock.len() >= MAX_RING_BUFFER_LINES {
                                l_lock.pop_front();
                            }
                            l_lock.push_back(err_msg);
                        }
                        *status_clone.write().await =
                            DevProcessStatus::Degraded(format!("Restart spawn failed: {}", e));
                        break;
                    }
                }
            } else {
                // Exit cleanly or retries exhausted
                if let Some(0) = exit_code {
                    *status_clone.write().await = DevProcessStatus::Exited(Some(0));
                } else {
                    *status_clone.write().await = DevProcessStatus::Exited(exit_code);
                    if stats.restart_count >= restart_policy_clone.max_retries()
                        && restart_policy_clone.max_retries() > 0
                    {
                        let exhausted_msg = format!(
                            "[WATCHDOG] Maximum restart retries ({}) exhausted. Process terminated.",
                            restart_policy_clone.max_retries()
                        );
                        let mut l_lock = logs_clone.write().await;
                        if l_lock.len() >= MAX_RING_BUFFER_LINES {
                            l_lock.pop_front();
                        }
                        l_lock.push_back(exhausted_msg);
                    }
                }
                break;
            }
        }
    });

    Ok(DevProcessHandle {
        id: dev_id,
        name: display_name,
        process_type: req.process_type,
        command: req.command,
        working_dir: work_dir,
        extra_env: req.extra_env,
        port_policy: req.port_policy,
        max_memory_mb: req.max_memory_mb,
        pid: pid_atomic,
        pgid: pgid_atomic,
        started_at: Instant::now(),
        status,
        ports,
        logs,
        is_shutting_down,
        cancel_hook: None,
        restart_policy,
        restart_stats,
        port_resolution: port_res_lock,
    })
}

/// Terminates an entire process tree given its process group ID.
/// Sends SIGTERM to the process group, waits up to 1,000ms, and falls back to SIGKILL if still alive.
#[cfg(unix)]
pub async fn terminate_process_group(pgid: u32) -> Result<()> {
    let pgid_i32 = pgid as i32;

    // Send SIGTERM to the entire process group (-pgid)
    unsafe {
        let _ = libc::kill(-pgid_i32, libc::SIGTERM);
    }

    // Grace period poll: up to 1,000ms in 50ms increments
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let is_alive = unsafe { libc::kill(-pgid_i32, 0) == 0 };
        if !is_alive {
            return Ok(());
        }
    }

    // Forceful SIGKILL fallback to eliminate all remaining child nodes
    unsafe {
        let _ = libc::kill(-pgid_i32, libc::SIGKILL);
    }

    // Wait a brief moment to reap
    tokio::time::sleep(Duration::from_millis(50)).await;
    Ok(())
}

#[cfg(not(unix))]
pub async fn terminate_process_group(_pgid: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_spawn_and_terminate_process_group() {
        let temp = tempdir().unwrap();
        let req = SpawnDevRequest {
            command: "echo 'Starting server'; sleep 60".to_string(),
            name: Some("test-sleep".to_string()),
            process_type: DevProcessType::Script,
            working_dir: None,
            extra_env: std::collections::HashMap::new(),
            port_hint: None,
            max_memory_mb: None,
            port_policy: None,
            restart_policy: None,
        };

        let handle = spawn_process_group(temp.path(), req).await.unwrap();
        assert_eq!(handle.name, "test-sleep");
        assert!(handle.pid() > 0);
        assert!(handle.status.read().await.is_alive());

        // Wait a brief moment for output
        tokio::time::sleep(Duration::from_millis(150)).await;
        let logs = handle.get_logs(10, None).await;
        assert!(logs.iter().any(|l| l.contains("Starting server")));

        // Terminate cleanly
        handle.terminate().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!handle.status.read().await.is_alive());
    }

    #[tokio::test]
    async fn test_watchdog_auto_restart_on_failure() {
        let temp = tempdir().unwrap();
        let req = SpawnDevRequest {
            command: "echo 'Process tick'; exit 1".to_string(),
            name: Some("test-crash".to_string()),
            process_type: DevProcessType::Script,
            working_dir: None,
            extra_env: std::collections::HashMap::new(),
            port_hint: None,
            max_memory_mb: None,
            port_policy: None,
            restart_policy: Some(RestartPolicy::OnFailure {
                max_retries: 2,
                backoff_ms: 50,
            }),
        };

        let handle = spawn_process_group(temp.path(), req).await.unwrap();

        // Wait for restarts to happen
        tokio::time::sleep(Duration::from_millis(400)).await;

        let stats = handle.restart_stats.read().await.clone();
        assert!(stats.restart_count > 0 || stats.consecutive_fast_crashes > 0);

        let logs = handle.get_logs(20, None).await;
        assert!(logs.iter().any(|l| l.contains("Process tick")));

        // Clean up
        let _ = handle.terminate().await;
    }
}
