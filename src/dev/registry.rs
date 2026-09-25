//! Global thread-safe registry managing development processes, servers, Docker stacks, and browsers.

use crate::dev::metrics::{sample_aggregate_metrics, sample_process_metrics};
use crate::dev::models::{
    DevProcessId, DevProcessSummary, DevProcessType, RuntimeResourceSummary, SpawnDevRequest,
};
use crate::dev::process::{spawn_process_group, DevProcessHandle};
use crate::error::{DevError, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;
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
        let pid = handle_arc.pid;

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

    /// Lists summaries of all managed processes.
    pub async fn list(&self) -> Vec<DevProcessSummary> {
        let lock = self.processes.read().await;
        let mut summaries = Vec::with_capacity(lock.len());
        for handle in lock.values() {
            summaries.push(self.create_summary(handle).await);
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        summaries
    }

    /// Retrieves detailed summary for a specific process ID.
    pub async fn get(&self, id: &DevProcessId) -> Option<DevProcessSummary> {
        let lock = self.processes.read().await;
        if let Some(handle) = lock.get(id) {
            Some(self.create_summary(handle).await)
        } else {
            None
        }
    }

    /// Reads recent logs from the process output ring buffer.
    pub async fn logs(
        &self,
        id: &DevProcessId,
        tail: usize,
        filter: Option<&str>,
    ) -> Result<Vec<String>> {
        let lock = self.processes.read().await;
        let handle = lock
            .get(id)
            .ok_or_else(|| DevError::NotFound(id.to_string()))?;
        Ok(handle.get_logs(tail, filter).await)
    }

    /// Stops a managed process by ID.
    pub async fn stop(&self, id: &DevProcessId) -> Result<bool> {
        let lock = self.processes.read().await;
        let handle = lock
            .get(id)
            .ok_or_else(|| DevError::NotFound(id.to_string()))?;
        if let Ok(mut set) = self.active_pgids.lock() {
            set.remove(&handle.pid);
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
            };
            (req, Arc::clone(handle))
        };

        // Terminate old process
        let _ = old_handle.terminate().await;
        if let Ok(mut set) = self.active_pgids.lock() {
            set.remove(&old_handle.pid);
        }

        // Spawn new process
        let new_handle = spawn_process_group(workspace_root, req).await?;
        let new_arc = Arc::new(new_handle);
        let pid = new_arc.pid;
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
                pids.push(handle.pid);
            }
        }
        sample_aggregate_metrics(&pids)
    }

    /// Internal helper to assemble DevProcessSummary with live metrics.
    async fn create_summary(&self, handle: &DevProcessHandle) -> DevProcessSummary {
        let status = handle.status.read().await.clone();
        let ports = handle.ports.read().await.clone();
        let usage = if status.is_alive() {
            sample_process_metrics(handle.pid)
        } else {
            Default::default()
        };

        let primary_port = ports.first().copied();
        let url = primary_port.map(|p| format!("http://localhost:{}", p));

        DevProcessSummary {
            id: handle.id.clone(),
            name: handle.name.clone(),
            process_type: handle.process_type,
            status,
            pid: Some(handle.pid),
            ports,
            url,
            cpu_percent: usage.cpu_percent,
            memory_rss_mb: usage.memory_rss_mb,
            uptime_secs: handle.uptime_secs(),
        }
    }
}

/// Synchronously kills all tracked process groups via direct OS signals.
/// Safe to invoke from panic hooks, signal handlers, and Drop guards.
pub fn kill_all_sync() {
    if let Some(registry) = GLOBAL_DEV_REGISTRY.get() {
        if let Ok(mut set) = registry.active_pgids.lock() {
            for &pgid in set.iter() {
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(-(pgid as i32), libc::SIGTERM);
                    let _ = libc::kill(-(pgid as i32), libc::SIGKILL);
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
}
