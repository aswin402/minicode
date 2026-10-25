pub mod fanout;
pub mod mailbox;
pub mod message;
pub mod orchestrator;
pub mod pool;
pub mod reducer;
pub mod relay;
pub mod scratchpad;
pub mod transcript;
pub mod types;
pub mod worker;

#[allow(unused_imports)]
pub use fanout::{FanoutJoinMode, FanoutOrchestrator, FanoutTaskItem, MergeStatus, WorkerResult};
#[allow(unused_imports)]
pub use mailbox::AgentMailbox;
#[allow(unused_imports)]
pub use message::{AgentMessage, MessageIntent};
#[allow(unused_imports)]
pub use orchestrator::SubagentOrchestrator;
#[allow(unused_imports)]
pub use pool::{MiniPubPool, SubagentPool};
#[allow(unused_imports)]
pub use reducer::{
    AstSymbolImpact, CommandVerificationOutcome, SubagentReport, SubagentSynthesisContext,
    SubagentSynthesisReducer,
};
#[allow(unused_imports)]
pub use relay::{get_global_relay_store, SubagentRelayStore};
#[allow(unused_imports)]
pub use scratchpad::{ScratchpadEntry, SharedScratchpad, WorkerMessage, WorkerMessageBus};
#[allow(unused_imports)]
pub use transcript::{
    get_global_transcript_store, SubagentStepRecord, SubagentTranscript, SubagentTranscriptStore,
};
pub use types::SubagentResult as SubAgentResult;
#[allow(unused_imports)]
pub use types::{
    AgentId, MiniPubConfig, MiniPubInfo, MiniPubResult, MiniPubRole, MiniPubState, MiniPubTaskSpec,
    MiniPubTodoItem, MiniPubUpdate, SubagentConfig, SubagentInfo, SubagentResult, SubagentRole,
    SubagentState, SubagentTaskSpec, WorkspaceMode,
};
#[allow(unused_imports)]
pub use worker::{MiniPubWorker, SubagentWorker};

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static GLOBAL_POOL: OnceLock<SubagentPool> = OnceLock::new();
static GLOBAL_SCRATCHPAD: OnceLock<SharedScratchpad> = OnceLock::new();
static GLOBAL_MESSAGE_BUS: OnceLock<WorkerMessageBus> = OnceLock::new();

/// Returns a reference to the global SubagentPool for the workspace
pub fn get_global_subagent_pool(workspace_root: &Path) -> &'static SubagentPool {
    GLOBAL_POOL.get_or_init(|| SubagentPool::new(workspace_root))
}

/// Returns a reference to the global SharedScratchpad
pub fn get_global_scratchpad() -> &'static SharedScratchpad {
    GLOBAL_SCRATCHPAD.get_or_init(SharedScratchpad::new)
}

/// Returns a reference to the global WorkerMessageBus
pub fn get_global_message_bus() -> &'static WorkerMessageBus {
    GLOBAL_MESSAGE_BUS.get_or_init(WorkerMessageBus::new)
}

/// Returns a reference to the global SubagentPool if already initialized
pub fn try_get_global_subagent_pool() -> Option<&'static SubagentPool> {
    GLOBAL_POOL.get()
}

use crate::error::Result;

/// Executes an autonomous `minipub` subagent in-process using the shared `SubagentPool`.
#[allow(dead_code)]
pub struct SubAgent {
    pub task_id: String,
    pub workspace_root: PathBuf,
    pub use_worktree: bool,
    pub timeout_secs: u64,
    pub config: Option<SubagentConfig>,
}

#[allow(dead_code)]
impl SubAgent {
    /// Creates a new SubAgent task runner.
    #[allow(dead_code)]
    pub fn new(
        workspace_root: &Path,
        task_id: &str,
        use_worktree: bool,
        timeout_secs: u64,
    ) -> Self {
        Self {
            task_id: task_id.to_string(),
            workspace_root: workspace_root.to_path_buf(),
            use_worktree,
            timeout_secs,
            config: None,
        }
    }

    /// Creates a new SubAgent task runner with role configuration.
    pub fn with_config(
        workspace_root: &Path,
        task_id: &str,
        use_worktree: bool,
        timeout_secs: u64,
        config: Option<SubagentConfig>,
    ) -> Self {
        Self {
            task_id: task_id.to_string(),
            workspace_root: workspace_root.to_path_buf(),
            use_worktree,
            timeout_secs,
            config,
        }
    }

    /// Executes the subagent task prompt in-process via `SubagentPool` and yields the final outcome.
    pub async fn run_task(&self, task_prompt: &str) -> Result<SubagentResult> {
        let pool = get_global_subagent_pool(&self.workspace_root);
        let provider = pool.get_or_create_provider().await;
        let assigned_role = self
            .config
            .as_ref()
            .map(|c| c.role)
            .unwrap_or(SubagentRole::Coder);
        let mut cfg = self.config.clone().unwrap_or_else(|| {
            let app_cfg = crate::config::Config::load_for_workspace(&self.workspace_root);
            SubagentConfig::for_role_with_minipub_config(assigned_role, &app_cfg.agent.minipub)
        });
        if self.timeout_secs > 0 {
            cfg.inactivity_timeout_secs = self.timeout_secs;
        }
        pool.run_subagent_with_options(
            assigned_role,
            task_prompt,
            Some(cfg),
            provider,
            self.use_worktree,
        )
        .await
    }
}

#[cfg(test)]
mod tests;
