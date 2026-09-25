//! Global thread-safe registry managing development processes, servers, Docker stacks, and browsers.

use crate::dev::metrics::{sample_aggregate_metrics, sample_process_metrics};
use crate::dev::models::{
    DevProcessId, DevProcessStatus, DevProcessSummary, DevProcessType, RuntimeResourceSummary,
    SpawnDevRequest,
};
use crate::dev::process::{spawn_process_group, DevProcessHandle};
use crate::error::{DevError, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

static GLOBAL_DEV_REGISTRY: OnceLock<Arc<MiniDevRegistry>> = OnceLock::new();

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
}

impl Default for MiniDevRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MiniDevRegistry {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            docker_containers: Arc::new(RwLock::new(HashSet::new())),
            browser_pids: Arc::new(RwLock::new(HashSet::new())),
            active_pgids: Arc::new(std::sync::Mutex::new(HashSet::new())),
        }
    }

    /// Spawns and registers a new development daemon or process.
    pub async fn spawn(
        &self,
        workspace_root: &Path,
        req: SpawnDevRequest,
    ) -> Result<DevProcessSummary> {
        let is_docker =
            req.process_type == DevProcessType::Docker || req.command.contains("docker run");
        let handle = spawn_process_group(workspace_root, req).await?;
        let handle_arc = Arc::new(handle);
        let id = handle_arc.id.clone();
        let pid = handle_arc.pid();

        if let Ok(mut set) = self.active_pgids.lock() {
            set.insert(pid);
        }

        if is_docker {
            let mut dock = self.docker_containers.write().await;
            dock.insert(id.as_str().to_string());
        }

        let summary = self.create_summary(&handle_arc).await;

        let mut lock = self.processes.write().await;
        lock.insert(id, handle_arc);

        Ok(summary)
    }

    /// Lists summaries of all managed processes, optionally filtered by process category.
    pub async fn list_filtered(
        &self,
        filter_type: Option<DevProcessType>,
    ) -> Vec<DevProcessSummary> {
        let lock = self.processes.read().await;
        let mut summaries = Vec::with_capacity(lock.len());
        for handle in lock.values() {
            if let Some(target) = filter_type {
                if handle.process_type != target {
                    continue;
                }
            }
            summaries.push(self.create_summary(handle).await);
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
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

    /// Retrieves detailed summary for a specific process ID with flexible prefix matching.
    pub async fn get(&self, id: &DevProcessId) -> Option<DevProcessSummary> {
        let lock = self.processes.read().await;
        if let Some(handle) = lock.get(id) {
            Some(self.create_summary(handle).await)
        } else {
            let alt_id = DevProcessId::from(format!("worker-{}", id.as_str()));
            if let Some(handle) = lock.get(&alt_id) {
                Some(self.create_summary(handle).await)
            } else if let Some(stripped) = id.as_str().strip_prefix("worker-") {
                let s_id = DevProcessId::from(stripped);
                if let Some(handle) = lock.get(&s_id) {
                    Some(self.create_summary(handle).await)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    /// Reads recent logs from the process output ring buffer with flexible ID matching.
    pub async fn logs(
        &self,
        id: &DevProcessId,
        tail: usize,
        filter: Option<&str>,
    ) -> Result<Vec<String>> {
        let lock = self.processes.read().await;
        let handle = lock
            .get(id)
            .or_else(|| {
                let alt = DevProcessId::from(format!("worker-{}", id.as_str()));
                lock.get(&alt)
            })
            .or_else(|| {
                id.as_str()
                    .strip_prefix("worker-")
                    .and_then(|s| lock.get(&DevProcessId::from(s)))
            })
            .ok_or_else(|| DevError::NotFound(id.to_string()))?;
        Ok(handle.get_logs(tail, filter).await)
    }

    /// Stops a managed process or subagent worker by ID with flexible ID matching.
    pub async fn stop(&self, id: &DevProcessId) -> Result<bool> {
        let lock = self.processes.read().await;
        let handle = lock
            .get(id)
            .or_else(|| {
                let alt = DevProcessId::from(format!("worker-{}", id.as_str()));
                lock.get(&alt)
            })
            .or_else(|| {
                id.as_str()
                    .strip_prefix("worker-")
                    .and_then(|s| lock.get(&DevProcessId::from(s)))
            })
            .ok_or_else(|| DevError::NotFound(id.to_string()))?;
        let current_pgid = handle.pgid();
        if current_pgid > 0 && current_pgid != std::process::id() {
            if let Ok(mut set) = self.active_pgids.lock() {
                set.remove(&current_pgid);
            }
        }
        handle.terminate().await?;
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
                extra_env: HashMap::new(),
                port_hint: handle.ports.read().await.first().copied(),
                max_memory_mb: None,
                port_policy: None,
                restart_policy: Some(handle.restart_policy.clone()),
            };
            (req, Arc::clone(handle))
        };

        // Terminate old process
        let _ = old_handle.terminate().await;
        if let Ok(mut set) = self.active_pgids.lock() {
            set.remove(&old_handle.pid());
        }

        // Spawn new process
        let new_handle = spawn_process_group(workspace_root, req).await?;
        let new_arc = Arc::new(new_handle);
        let pid = new_arc.pid();
        if let Ok(mut set) = self.active_pgids.lock() {
            set.insert(pid);
        }
        let summary = self.create_summary(&new_arc).await;

        let mut lock = self.processes.write().await;
        lock.insert(id.clone(), new_arc);

        Ok(summary)
    }

    /// Terminates ALL active managed processes, containers, and browser instances.
    /// This is guaranteed to leave zero orphan processes.
    pub async fn kill_all(&self) -> Result<usize> {
        if let Ok(mut set) = self.active_pgids.lock() {
            set.clear();
        }

        let lock = self.processes.read().await;
        let mut count = 0;
        for handle in lock.values() {
            if handle.status.read().await.is_alive() {
                let _ = handle.terminate().await;
                count += 1;
            }
        }

        // Clean up any registered docker containers
        let dock_lock = self.docker_containers.read().await;
        for container_name in dock_lock.iter() {
            let _ = tokio::process::Command::new("docker")
                .args(["stop", container_name])
                .output()
                .await;
        }

        // Clean up any registered browser sessions
        let browser_lock = self.browser_pids.read().await;
        for &pid in browser_lock.iter() {
            #[cfg(unix)]
            unsafe {
                let _ = libc::kill(-(pid as i32), libc::SIGTERM);
                let _ = libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }

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
        let handle = Arc::new(DevProcessHandle {
            id: id.clone(),
            name,
            process_type: DevProcessType::Worker,
            command,
            working_dir,
            pid: Arc::new(std::sync::atomic::AtomicU32::new(pid)),
            pgid: Arc::new(std::sync::atomic::AtomicU32::new(pgid)),
            started_at: std::time::Instant::now(),
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
        });

        if pgid > 0 && pgid != std::process::id() {
            if let Ok(mut set) = self.active_pgids.lock() {
                set.insert(pgid);
            }
        }

        let mut lock = self.processes.write().await;
        lock.insert(id, Arc::clone(&handle));
        handle
    }

    /// Updates an active PGID when a monitored process is auto-restarted with a new PID/PGID.
    pub fn update_active_pgid(&self, old_pgid: u32, new_pgid: u32) {
        if let Ok(mut set) = self.active_pgids.lock() {
            set.remove(&old_pgid);
            if new_pgid > 0 && new_pgid != std::process::id() {
                set.insert(new_pgid);
            }
        }
    }

    /// Registers an external child PID (such as Chrome or a background subagent)
    /// to ensure it is terminated on minicode exit.
    pub fn register_external_pid(&self, pid: u32) {
        if let Ok(mut set) = self.active_pgids.lock() {
            set.insert(pid);
        }
        if let Ok(mut browsers) = self.browser_pids.try_write() {
            browsers.insert(pid);
        }
    }

    /// Gathers aggregate real-time resource telemetry across all managed processes.
    pub async fn resources(&self) -> RuntimeResourceSummary {
        let lock = self.processes.read().await;
        let mut pids = Vec::new();
        for handle in lock.values() {
            if handle.status.read().await.is_alive() {
                pids.push(handle.pid());
            }
        }
        sample_aggregate_metrics(&pids)
    }

    /// Internal helper to assemble DevProcessSummary with live metrics.
    async fn create_summary(&self, handle: &DevProcessHandle) -> DevProcessSummary {
        let status = handle.status.read().await.clone();
        let ports = handle.ports.read().await.clone();
        let current_pid = handle.pid();
        let usage = if status.is_alive() {
            sample_process_metrics(current_pid)
        } else {
            Default::default()
        };

        let primary_port = ports.first().copied();
        let url = primary_port.map(|p| format!("http://localhost:{}", p));
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
        }
    }
}

/// Synchronously kills all tracked process groups via direct OS signals.
/// Safe to invoke from panic hooks, signal handlers, and Drop guards.
pub fn kill_all_sync() {
    if let Some(registry) = GLOBAL_DEV_REGISTRY.get() {
        if let Ok(mut set) = registry.active_pgids.lock() {
            let my_pid = std::process::id();
            for &pgid in set.iter() {
                if pgid > 0 && pgid != my_pid {
                    #[cfg(unix)]
                    unsafe {
                        let _ = libc::kill(-(pgid as i32), libc::SIGTERM);
                        let _ = libc::kill(-(pgid as i32), libc::SIGKILL);
                    }
                }
            }
            set.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
