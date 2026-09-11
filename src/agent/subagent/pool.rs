use super::types::{SubagentConfig, SubagentInfo, SubagentResult, SubagentRole, SubagentState};
use super::worker::SubagentWorker;
use crate::agent::provider::Provider;
use crate::error::{MinicodeError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handle to a registered subagent worker
struct WorkerHandle {
    info: Arc<RwLock<SubagentInfo>>,
    cancel_flag: Arc<AtomicBool>,
}

/// Thread-safe supervisor pool managing concurrent subagents
#[derive(Clone)]
pub struct SubagentPool {
    workers: Arc<RwLock<HashMap<String, WorkerHandle>>>,
    counter: Arc<RwLock<usize>>,
    workspace_root: PathBuf,
    provider: Arc<RwLock<Option<Arc<dyn Provider>>>>,
}

#[allow(dead_code)]
impl SubagentPool {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            counter: Arc::new(RwLock::new(1)),
            workspace_root: workspace_root.to_path_buf(),
            provider: Arc::new(RwLock::new(None)),
        }
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

    /// Spawns a subagent worker and executes it to completion synchronously
    pub async fn run_subagent(
        &self,
        role: SubagentRole,
        prompt: &str,
        custom_config: Option<SubagentConfig>,
        provider: Arc<dyn Provider>,
    ) -> Result<SubagentResult> {
        let id = self.next_id(&role).await;
        let config = custom_config.unwrap_or_else(|| SubagentConfig::for_role(role.clone()));

        let worker =
            SubagentWorker::new(id.clone(), prompt.to_string(), config, &self.workspace_root);
        let info = Arc::clone(&worker.info);
        let cancel_flag = Arc::clone(&worker.cancel_flag);

        {
            let mut workers = self.workers.write().await;
            workers.insert(id.clone(), WorkerHandle { info, cancel_flag });
        }

        worker.run(provider).await
    }

    /// Spawns an autonomous background subagent worker without blocking caller
    pub async fn spawn_background_worker(
        &self,
        role: SubagentRole,
        prompt: &str,
        custom_config: Option<SubagentConfig>,
        provider: Arc<dyn Provider>,
        isolate_worktree: bool,
    ) -> Result<String> {
        let id = self.next_id(&role).await;
        let config = custom_config.unwrap_or_else(|| SubagentConfig::for_role(role.clone()));

        // Worktree isolation if requested
        let effective_root = if isolate_worktree {
            let worktree_mgr = crate::git::worktree::WorktreeManager::new(&self.workspace_root);
            match worktree_mgr.create_worktree(&id).await {
                Ok(path) => path,
                Err(e) => {
                    tracing::warn!(
                        subagent_id = %id,
                        error = %e,
                        "Failed to create worktree, falling back to main workspace"
                    );
                    self.workspace_root.clone()
                }
            }
        } else {
            self.workspace_root.clone()
        };

        let worker = SubagentWorker::new(id.clone(), prompt.to_string(), config, &effective_root);
        {
            let mut info_guard = worker.info.write().await;
            info_guard.isolate_worktree = isolate_worktree;
            info_guard.status_message = Some("Spawned in background".to_string());
        }

        let info = Arc::clone(&worker.info);
        let cancel_flag = Arc::clone(&worker.cancel_flag);

        {
            let mut workers = self.workers.write().await;
            workers.insert(id.clone(), WorkerHandle { info, cancel_flag });
        }

        let worker_id = id.clone();
        let worker_role = role.clone();
        let ws_root = self.workspace_root.clone();

        tokio::spawn(async move {
            let res = worker.run(provider).await;
            let scratchpad = crate::agent::subagent::get_global_scratchpad();
            match res {
                Ok(sub_res) => {
                    let title = format!("Subagent {} ({}) Report", worker_id, worker_role.badge());
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
                    let _ = scratchpad.save_to_disk(&ws_root);
                    tracing::info!(subagent_id = %worker_id, "Background subagent completed");
                }
                Err(e) => {
                    tracing::error!(subagent_id = %worker_id, error = %e, "Background subagent failed");
                    let title = format!("Subagent {} ({}) Failure", worker_id, worker_role.badge());
                    scratchpad.write_entry(
                        &format!("subagent_{}", worker_id),
                        &title,
                        &format!("Execution error: {}", e),
                        &worker_id,
                    );
                    let _ = scratchpad.save_to_disk(&ws_root);
                }
            }
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

    /// Cancels a running subagent by ID
    pub async fn kill_subagent(&self, id: &str) -> Result<()> {
        let workers = self.workers.read().await;
        if let Some(handle) = workers.get(id) {
            handle.cancel_flag.store(true, Ordering::SeqCst);
            let mut info = handle.info.write().await;
            info.state = SubagentState::Canceled;
            info.current_tool = None;
            info.status_message = Some("Canceled by user".to_string());
            Ok(())
        } else {
            Err(MinicodeError::Channel(format!(
                "Subagent '{}' not found",
                id
            )))
        }
    }

    /// Cancels all currently running subagents
    pub async fn kill_all(&self) {
        let workers = self.workers.read().await;
        for handle in workers.values() {
            handle.cancel_flag.store(true, Ordering::SeqCst);
            if let Ok(mut info) = handle.info.try_write() {
                info.state = SubagentState::Canceled;
                info.current_tool = None;
                info.status_message = Some("Canceled by user".to_string());
            }
        }
    }

    /// Formats a clean Markdown summary report of all subagents in the pool
    pub async fn format_swarm_summary(&self) -> String {
        let list = self.list_subagents().await;
        if list.is_empty() {
            return "No subagent workers have been spawned in this session.".to_string();
        }

        let mut out = String::from("### Subagent Swarm Status\n\n");
        out.push_str(
            "| ID | Role | Status | Active Tool / Detail | Turns | Tokens Used | Task Prompt |\n",
        );
        out.push_str("| :--- | :--- | :--- | :--- | :---: | :---: | :--- |\n");

        for item in list {
            let status_badge = match &item.state {
                SubagentState::Idle => "○ Idle",
                SubagentState::Running => "◉ Running",
                SubagentState::Completed => "✔ Done",
                SubagentState::Failed(_) => "✗ Failed",
                SubagentState::Canceled => "⊘ Canceled",
            };
            let detail = if let Some(ref t) = item.current_tool {
                format!("⚡ `{}`", t)
            } else if let Some(ref s) = item.status_message {
                s.clone()
            } else {
                "-".to_string()
            };
            let short_prompt = crate::utils::truncate_ellipsis(&item.prompt, 40);
            out.push_str(&format!(
                "| `{}` | **{}** | {} | {} | {} | {} | {} |\n",
                item.id,
                item.role.badge(),
                status_badge,
                detail,
                item.turns_executed,
                item.tokens_used,
                short_prompt
            ));
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

        let id1 = pool.next_id(&SubagentRole::Researcher).await;
        assert_eq!(id1, "researcher-1");

        let id2 = pool.next_id(&SubagentRole::TestEngineer).await;
        assert_eq!(id2, "testengineer-2");
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
                SubagentRole::Researcher,
                "Analyze codebase architecture",
                None,
                mock,
                false,
            )
            .await
            .unwrap();

        assert_eq!(id, "researcher-1");
        assert_eq!(pool.worker_count(), 1);

        let snapshots = pool.snapshot_subagents();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, "researcher-1");
        assert_eq!(snapshots[0].role, SubagentRole::Researcher);

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
        assert!(summary.contains("researcher-1"));
        assert!(summary.contains("Researcher"));
    }

    #[tokio::test]
    async fn test_kill_subagent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pool = SubagentPool::new(temp_dir.path());

        let mock = Arc::new(MockProvider::new("mock", "mock-model"));
        // No responses queued so it would wait or finish
        let id = pool
            .spawn_background_worker(
                SubagentRole::CodeReviewer,
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
        assert_eq!(info.state, SubagentState::Canceled);
        assert_eq!(info.status_message.as_deref(), Some("Canceled by user"));
    }
}
