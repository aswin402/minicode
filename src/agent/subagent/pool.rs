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
}

#[allow(dead_code)]
impl SubagentPool {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            counter: Arc::new(RwLock::new(1)),
            workspace_root: workspace_root.to_path_buf(),
        }
    }

    /// Returns the number of registered workers in the pool
    pub fn worker_count(&self) -> usize {
        self.workers.try_read().map(|w| w.len()).unwrap_or(0)
    }

    /// Generates a human-friendly subagent ID
    pub async fn next_id(&self, role: &SubagentRole) -> String {
        let mut count = self.counter.write().await;
        let id = format!("{}-{}", role.badge().to_lowercase(), *count);
        *count += 1;
        id
    }

    /// Spawns a subagent worker and executes it to completion
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

    /// Lists live telemetry info across all registered subagents
    pub async fn list_subagents(&self) -> Vec<SubagentInfo> {
        let workers = self.workers.read().await;
        let mut list = Vec::new();
        for handle in workers.values() {
            let info = handle.info.read().await;
            list.push(info.clone());
        }
        list.sort_by(|a, b| b.started_at_secs.cmp(&a.started_at_secs));
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
        out.push_str("| ID | Role | Status | Turns | Tokens Used | Task Prompt |\n");
        out.push_str("| :--- | :--- | :--- | :---: | :---: | :--- |\n");

        for item in list {
            let status_badge = match &item.state {
                SubagentState::Idle => "○ Idle",
                SubagentState::Running => "◉ Running",
                SubagentState::Completed => "✔ Done",
                SubagentState::Failed(_) => "✗ Failed",
                SubagentState::Canceled => "⊘ Canceled",
            };
            let short_prompt: String = item.prompt.chars().take(40).collect();
            out.push_str(&format!(
                "| `{}` | **{}** | {} | {} | {} | {} |\n",
                item.id,
                item.role.badge(),
                status_badge,
                item.turns_executed,
                item.tokens_used,
                short_prompt
            ));
        }

        out
    }
}
