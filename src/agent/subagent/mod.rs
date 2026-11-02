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
use std::sync::{Arc, OnceLock, RwLock as StdRwLock};
use tokio::sync::mpsc;

type LiveEventSender = Option<mpsc::UnboundedSender<crate::agent::types::AgentEvent>>;

static GLOBAL_POOL: OnceLock<SubagentPool> = OnceLock::new();
static GLOBAL_SCRATCHPAD: OnceLock<SharedScratchpad> = OnceLock::new();
static GLOBAL_MESSAGE_BUS: OnceLock<WorkerMessageBus> = OnceLock::new();
static LIVE_EVENT_SENDER: OnceLock<StdRwLock<LiveEventSender>> = OnceLock::new();
static LIVE_ORCHESTRATOR_MODEL: OnceLock<StdRwLock<Option<(String, String)>>> = OnceLock::new();

/// Returns a reference to the global SubagentPool for the workspace
pub fn get_global_subagent_pool(workspace_root: &Path) -> &'static SubagentPool {
    GLOBAL_POOL.get_or_init(|| SubagentPool::new(workspace_root))
}

/// Synchronizes the Orchestrator's live active `Provider`, `model`, and `AgentEvent` telemetry channel
/// into the global `SubagentPool` so all spawned `MiniPub` workers inherit the active runtime chain
/// and stream real-time `SubagentProgress` / `SubagentCompleted` events to the TUI and JSON stream.
pub fn sync_live_orchestrator_runtime(
    workspace_root: &Path,
    provider: Arc<dyn crate::agent::provider::Provider>,
    model: &str,
    event_sender: Option<mpsc::UnboundedSender<crate::agent::types::AgentEvent>>,
) {
    let prov_name = provider.name().trim().to_ascii_lowercase();
    let model_trimmed = model.trim().to_string();
    if !prov_name.is_empty() && !model_trimmed.is_empty() {
        let lock = LIVE_ORCHESTRATOR_MODEL.get_or_init(|| StdRwLock::new(None));
        if let Ok(mut guard) = lock.write() {
            *guard = Some((prov_name, model_trimmed));
        }
    }
    if let Some(sender) = event_sender {
        let lock = LIVE_EVENT_SENDER.get_or_init(|| StdRwLock::new(None));
        if let Ok(mut guard) = lock.write() {
            *guard = Some(sender);
        }
    }
    let pool = get_global_subagent_pool(workspace_root);
    pool.set_provider(provider);
}

/// Returns a clone of the live Orchestrator `AgentEvent` sender if active.
pub fn clone_live_event_sender() -> Option<mpsc::UnboundedSender<crate::agent::types::AgentEvent>> {
    let lock = LIVE_EVENT_SENDER.get()?;
    lock.read().ok().and_then(|g| g.clone())
}

/// Clears the live Orchestrator `AgentEvent` sender when a turn ends so headless event consumers exit cleanly.
pub fn clear_live_event_sender() {
    if let Some(lock) = LIVE_EVENT_SENDER.get() {
        if let Ok(mut guard) = lock.write() {
            *guard = None;
        }
    }
}

/// Emits a structured `AgentEvent` (such as `SubagentProgress` or `SubagentCompleted`) to the active host channel.
pub fn emit_subagent_event(event: crate::agent::types::AgentEvent) {
    if let Some(sender) = clone_live_event_sender() {
        let _ = sender.send(event);
    }
}

/// Returns the Orchestrator's live active model if its provider matches `provider_name`.
pub fn get_live_orchestrator_model_for_provider(provider_name: &str) -> Option<String> {
    let lock = LIVE_ORCHESTRATOR_MODEL.get()?;
    let guard = lock.read().ok()?;
    let (live_prov, live_model) = guard.as_ref()?;
    live_prov
        .eq_ignore_ascii_case(provider_name.trim())
        .then(|| live_model.clone())
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
