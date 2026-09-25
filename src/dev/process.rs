//! Isolated process group management, death-pipe sentinels, ring buffers, and graceful teardown.

use crate::dev::models::{DevProcessId, DevProcessStatus, DevProcessType, SpawnDevRequest};
use crate::dev::ports::scan_ports_from_output;
use crate::error::{DevError, Result};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;

/// Maximum number of log lines retained in the in-memory ring buffer per process.
pub const MAX_RING_BUFFER_LINES: usize = 1000;

/// Live handle to a background managed process.
#[derive(Debug)]
pub struct DevProcessHandle {
    pub id: DevProcessId,
    pub name: String,
    pub process_type: DevProcessType,
    pub command: String,
    pub working_dir: PathBuf,
    pub pid: u32,
    pub pgid: u32,
    pub started_at: Instant,
    pub status: Arc<RwLock<DevProcessStatus>>,
    pub ports: Arc<RwLock<Vec<u16>>>,
    pub logs: Arc<RwLock<VecDeque<String>>>,
    pub is_shutting_down: Arc<AtomicBool>,
}

impl DevProcessHandle {
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

    /// Terminates the process group cleanly with SIGTERM followed by SIGKILL fallback.
    pub async fn terminate(&self) -> Result<()> {
        self.is_shutting_down.store(true, Ordering::SeqCst);
        let mut status_lock = self.status.write().await;
        if !status_lock.is_alive() {
            return Ok(());
        }

        terminate_process_group(self.pgid).await?;
        *status_lock = DevProcessStatus::Stopped;
        Ok(())
    }
}

/// Spawns a command inside a dedicated isolated process group with streaming output ring buffers.
pub async fn spawn_process_group(
    workspace_root: &Path,
    req: SpawnDevRequest,
) -> Result<DevProcessHandle> {
    let dev_id = DevProcessId::new(req.name.as_deref());
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

    let mut std_cmd = std::process::Command::new("sh");
    std_cmd.arg("-c").arg(&req.command);
    std_cmd.current_dir(&work_dir);

    for (k, v) in &req.extra_env {
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

    let mut child = tokio_cmd
        .spawn()
        .map_err(|e| DevError::SpawnFailed(format!("Failed to spawn child process: {}", e)))?;

    let child_pid = child
        .id()
        .ok_or_else(|| DevError::SpawnFailed("Failed to acquire child PID".to_string()))?;
    let pgid = child_pid; // Since setpgid(0, 0) was called in pre_exec

    let status = Arc::new(RwLock::new(DevProcessStatus::Running));
    let ports = Arc::new(RwLock::new(Vec::new()));
    if let Some(hint) = req.port_hint {
        ports.write().await.push(hint);
    }

    let logs = Arc::new(RwLock::new(VecDeque::with_capacity(MAX_RING_BUFFER_LINES)));
    let is_shutting_down = Arc::new(AtomicBool::new(false));

    // Spawn background stdout reader & port scanner
    if let Some(stdout) = child.stdout.take() {
        let logs_clone = Arc::clone(&logs);
        let ports_clone = Arc::clone(&ports);
        let status_clone = Arc::clone(&status);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                // Append to ring buffer
                {
                    let mut lock = logs_clone.write().await;
                    if lock.len() >= MAX_RING_BUFFER_LINES {
                        lock.pop_front();
                    }
                    lock.push_back(line.clone());
                }

                // Check for exposed ports
                let discovered = scan_ports_from_output(&line);
                if !discovered.is_empty() {
                    let mut p_lock = ports_clone.write().await;
                    for p in discovered {
                        if !p_lock.contains(&p) {
                            p_lock.push(p);
                        }
                    }
                    *status_clone.write().await = DevProcessStatus::Healthy;
                }
            }
        });
    }

    // Spawn background stderr reader
    if let Some(stderr) = child.stderr.take() {
        let logs_clone = Arc::clone(&logs);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let mut lock = logs_clone.write().await;
                if lock.len() >= MAX_RING_BUFFER_LINES {
                    lock.pop_front();
                }
                lock.push_back(format!("[STDERR] {}", line));
            }
        });
    }

    // Spawn background monitor task for child process exit
    let status_clone = Arc::clone(&status);
    let is_shutting_down_clone = Arc::clone(&is_shutting_down);
    tokio::spawn(async move {
        match child.wait().await {
            Ok(exit_status) => {
                let mut lock = status_clone.write().await;
                if is_shutting_down_clone.load(Ordering::SeqCst) {
                    *lock = DevProcessStatus::Stopped;
                } else if exit_status.success() {
                    *lock = DevProcessStatus::Exited(Some(0));
                } else {
                    *lock = DevProcessStatus::Exited(exit_status.code());
                }
            }
            Err(_) => {
                let mut lock = status_clone.write().await;
                if !is_shutting_down_clone.load(Ordering::SeqCst) {
                    *lock = DevProcessStatus::Exited(None);
                }
            }
        }
    });

    Ok(DevProcessHandle {
        id: dev_id,
        name: display_name,
        process_type: req.process_type,
        command: req.command,
        working_dir: work_dir,
        pid: child_pid,
        pgid,
        started_at: Instant::now(),
        status,
        ports,
        logs,
        is_shutting_down,
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
        };

        let handle = spawn_process_group(temp.path(), req).await.unwrap();
        assert_eq!(handle.name, "test-sleep");
        assert!(handle.pid > 0);
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
}
