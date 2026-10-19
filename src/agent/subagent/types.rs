use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Unique identifier for an agent (coordinator or subagent)
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

#[allow(dead_code)]
impl AgentId {
    /// Returns the standard identifier for the coordinator / parent agent
    pub fn parent() -> Self {
        AgentId("parent".to_string())
    }

    /// Generates a unique subagent identifier with a role prefix and short UUID
    pub fn new_subagent(prefix: &str) -> Self {
        let uuid_str = uuid::Uuid::new_v4().to_string();
        let short_uuid = if uuid_str.len() >= 8 {
            &uuid_str[..8]
        } else {
            &uuid_str
        };
        AgentId(format!("{}-{}", prefix, short_uuid))
    }

    /// Returns true if this agent represents the coordinator / parent agent
    pub fn is_parent(&self) -> bool {
        self.0 == "parent"
    }

    /// Returns a string slice of the agent id
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        AgentId(s.to_string())
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        AgentId(s)
    }
}

/// Workspace isolation mode for a subagent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    Auto,
    Worktree,
    Shared,
}

#[allow(dead_code)]
impl WorkspaceMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceMode::Auto => "auto",
            WorkspaceMode::Worktree => "worktree",
            WorkspaceMode::Shared => "shared",
        }
    }
}

/// Specialized role preset defining subagent capabilities and workspace isolation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentRole {
    Scout,
    Coder,
    Tester,
    Reviewer,
}

#[allow(dead_code)]
impl SubagentRole {
    /// Returns the string representation of this role
    pub fn as_str(&self) -> &'static str {
        match self {
            SubagentRole::Scout => "scout",
            SubagentRole::Coder => "coder",
            SubagentRole::Tester => "tester",
            SubagentRole::Reviewer => "reviewer",
        }
    }

    /// Returns the default workspace isolation mode for this role
    pub fn default_workspace_mode(&self) -> WorkspaceMode {
        match self {
            SubagentRole::Scout | SubagentRole::Reviewer => WorkspaceMode::Shared,
            SubagentRole::Coder | SubagentRole::Tester => WorkspaceMode::Worktree,
        }
    }

    /// Returns the tool filtering mode ("read_only" vs "standard")
    pub fn tool_filter_mode(&self) -> &'static str {
        match self {
            SubagentRole::Scout | SubagentRole::Reviewer => "read_only",
            SubagentRole::Coder | SubagentRole::Tester => "standard",
        }
    }

    /// Human-readable badge for UI rendering
    pub fn badge(&self) -> &'static str {
        match self {
            SubagentRole::Scout => "Scout",
            SubagentRole::Coder => "Coder",
            SubagentRole::Tester => "Tester",
            SubagentRole::Reviewer => "Reviewer",
        }
    }

    /// Flexible case-insensitive role parser
    pub fn from_str_loose(s: &str) -> Self {
        let normalized = s.trim().to_lowercase().replace('-', "_");
        match normalized.as_str() {
            "scout" | "researcher" | "research" => Self::Scout,
            "reviewer" | "code_reviewer" | "code_review" | "security" | "security_auditor" => {
                Self::Reviewer
            }
            "tester" | "test_engineer" | "test" => Self::Tester,
            "coder" => Self::Coder,
            _ => Self::Coder,
        }
    }

    /// Default tool whitelist for this role
    pub fn default_tool_whitelist(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        // Shared coordination & knowledge tools available to all MiniPub workers
        for common in &[
            "read_file",
            "grep_search",
            "file_search",
            "locate_symbol",
            "view_outline",
            "kit_skill_show",
            "kit_info",
            "scratchpad_read",
            "scratchpad_write",
            "scratchpad_list",
            "send_message",
            "send_worker_message",
            "read_worker_messages",
        ] {
            set.insert((*common).to_string());
        }

        match self {
            SubagentRole::Scout => {
                for t in &[
                    "fetch_or_browse",
                    "search_web",
                    "kit_stack_list",
                    "block_search",
                    "block_get",
                    "block_palettes",
                ] {
                    set.insert((*t).to_string());
                }
            }
            SubagentRole::Reviewer => {
                for t in &[
                    "browser_inspect",
                    "browser_screenshot",
                    "browser_console",
                    "browser_evaluate",
                    "browser_snapshot",
                    "browser_eval",
                    "kit_stack_diff",
                    "power_review",
                    "power_verify",
                ] {
                    set.insert((*t).to_string());
                }
            }
            SubagentRole::Tester => {
                for t in &[
                    "exec_cmd",
                    "write_file",
                    "patch_file",
                    "browser_inspect",
                    "browser_screenshot",
                    "browser_console",
                    "browser_evaluate",
                    "kit_stack_diff",
                    "power_verify",
                ] {
                    set.insert((*t).to_string());
                }
            }
            SubagentRole::Coder => {
                for t in &[
                    "write_file",
                    "patch_file",
                    "fetch_or_browse",
                    "search_web",
                    "exec_cmd",
                    "kit_add",
                    "kit_stack_diff",
                    "kit_stack_list",
                    "block_search",
                    "block_get",
                    "block_palettes",
                    "browser_inspect",
                    "browser_screenshot",
                    "browser_console",
                    "browser_evaluate",
                    "power_verify",
                    "power_review",
                ] {
                    set.insert((*t).to_string());
                }
            }
        }
        set
    }

    /// Default token budget for this role
    pub fn default_token_budget(&self) -> usize {
        match self {
            SubagentRole::Scout => 32_000,
            SubagentRole::Coder => crate::constants::DEFAULT_MINIPUB_TOKEN_BUDGET,
            SubagentRole::Tester => 40_000,
            SubagentRole::Reviewer => 24_000,
        }
    }

    /// Default max turns for this role
    pub fn default_max_turns(&self) -> usize {
        match self {
            SubagentRole::Scout => 12,
            SubagentRole::Coder => crate::constants::DEFAULT_MINIPUB_MAX_TURNS,
            SubagentRole::Tester => 12,
            SubagentRole::Reviewer => 8,
        }
    }
}

/// Lifecycle state of an active or finished subagent worker
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentState {
    Starting,
    Running,
    WaitingForInput,
    Completed,
    Failed(String),
    Terminated,
}

#[allow(dead_code)]
impl SubagentState {
    pub fn as_str(&self) -> &str {
        match self {
            SubagentState::Starting => "starting",
            SubagentState::Running => "running",
            SubagentState::WaitingForInput => "waiting_for_input",
            SubagentState::Completed => "completed",
            SubagentState::Failed(_) => "failed",
            SubagentState::Terminated => "terminated",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            SubagentState::Starting | SubagentState::Running | SubagentState::WaitingForInput
        )
    }
}

/// Structured todo checklist item assigned by the `minicode` Orchestrator to a `minipub` worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniPubTodoItem {
    pub id: usize,
    pub task: String,
    #[serde(default)]
    pub completed: bool,
}

impl MiniPubTodoItem {
    pub fn from_strings(items: &[String]) -> Vec<Self> {
        items
            .iter()
            .enumerate()
            .filter_map(|(idx, s)| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(Self {
                        id: idx + 1,
                        task: trimmed.to_string(),
                        completed: false,
                    })
                }
            })
            .collect()
    }
}

/// Configuration settings for instantiating a `minipub` (subagent) worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    pub role: SubagentRole,
    pub model: Option<String>,
    pub token_budget: usize,
    pub max_turns: usize,
    pub tool_whitelist: HashSet<String>,
    pub system_prompt_override: Option<String>,
    /// Activity-heartbeat stall timeout in seconds (resets on every LLM token or tool output)
    #[serde(default = "default_inactivity_timeout_secs")]
    pub inactivity_timeout_secs: u64,
    /// Structured todo checklist assigned by the `minicode` Orchestrator
    #[serde(default)]
    pub todos: Vec<MiniPubTodoItem>,
    /// Optional relative file/directory boundaries restricting where a `coder`/`tester` `minipub` may write
    #[serde(default)]
    pub file_boundaries: Vec<String>,
}

fn default_inactivity_timeout_secs() -> u64 {
    crate::constants::DEFAULT_MINIPUB_INACTIVITY_TIMEOUT_SECS
}

impl SubagentConfig {
    pub fn for_role(role: SubagentRole) -> Self {
        let tool_whitelist = role.default_tool_whitelist();
        let token_budget = role.default_token_budget();
        let max_turns = role.default_max_turns();

        Self {
            role,
            model: None,
            token_budget,
            max_turns,
            tool_whitelist,
            system_prompt_override: None,
            inactivity_timeout_secs: crate::constants::DEFAULT_MINIPUB_INACTIVITY_TIMEOUT_SECS,
            todos: Vec::new(),
            file_boundaries: Vec::new(),
        }
    }

    /// Creates a `SubagentConfig` (`MiniPubConfig`) incorporating project/global `[agent.minipub]` settings.
    pub fn for_role_with_minipub_config(
        role: SubagentRole,
        mp: &crate::config::MiniPubConfig,
    ) -> Self {
        let mut cfg = Self::for_role(role);
        if mp.default_token_budget > 0 {
            cfg.token_budget = mp.default_token_budget;
        }
        if mp.default_max_turns > 0 {
            cfg.max_turns = mp.default_max_turns;
        }
        if mp.inactivity_timeout_secs > 0 {
            cfg.inactivity_timeout_secs = mp.inactivity_timeout_secs;
        }
        for extra in &mp.extra_allowed_tools {
            let trimmed = extra.trim();
            if !trimmed.is_empty() {
                cfg.tool_whitelist.insert(trimmed.to_string());
            }
        }
        cfg
    }

    /// Attaches an Orchestrator-assigned todo checklist to this `SubagentConfig`.
    pub fn with_todos(mut self, todos: Vec<String>) -> Self {
        self.todos = MiniPubTodoItem::from_strings(&todos);
        self
    }

    /// Attaches cooperative file boundaries to this `SubagentConfig`.
    pub fn with_file_boundaries(mut self, file_boundaries: Vec<String>) -> Self {
        self.file_boundaries = file_boundaries
            .into_iter()
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty())
            .collect();
        self
    }
}

/// Live metadata and dynamic runtime controls for a `minipub` (subagent) worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentInfo {
    pub id: String,
    pub role: SubagentRole,
    pub state: SubagentState,
    pub prompt: String,
    pub tokens_used: usize,
    pub turns_executed: usize,
    pub started_at_secs: u64,
    pub finished_at_secs: Option<u64>,
    #[serde(default)]
    pub last_activity_secs: u64,
    #[serde(default)]
    pub current_tool: Option<String>,
    #[serde(default)]
    pub status_message: Option<String>,
    #[serde(default)]
    pub isolate_worktree: bool,
    #[serde(default)]
    pub final_summary: Option<String>,
    #[serde(default)]
    pub todos: Vec<MiniPubTodoItem>,
    #[serde(default)]
    pub file_boundaries: Vec<String>,
    /// Live activity-heartbeat timeout in seconds (dynamically modifiable by Orchestrator)
    #[serde(default = "default_inactivity_timeout_secs")]
    pub inactivity_timeout_secs: u64,
    /// Live token budget ceiling (dynamically modifiable by Orchestrator)
    #[serde(default)]
    pub token_budget: usize,
    /// Live max turns ceiling (dynamically modifiable by Orchestrator)
    #[serde(default)]
    pub max_turns: usize,
    /// Queued in-flight steering instructions injected by the Orchestrator
    #[serde(default)]
    pub pending_steer_messages: Vec<String>,
}

/// Dynamic in-flight modification payload for an active `MiniPub` worker (Orchestrator CRUD Update)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MiniPubUpdate {
    /// New inactivity timeout in seconds (`15..=7200`)
    pub inactivity_timeout_secs: Option<u64>,
    /// New token budget ceiling
    pub token_budget: Option<usize>,
    /// New max turns ceiling
    pub max_turns: Option<usize>,
    /// Replace the worker's todo checklist
    pub todos: Option<Vec<String>>,
    /// Append additional todo items to the worker's checklist
    pub append_todos: Vec<String>,
    /// Mark specific 1-indexed todo item IDs as completed (`[x]`)
    pub complete_todo_ids: Vec<usize>,
    /// Update cooperative file boundaries
    pub file_boundaries: Option<Vec<String>>,
    /// Inject a live steering prompt into the worker's next turn without restarting it
    pub steer_prompt: Option<String>,
}

#[allow(dead_code)]
impl SubagentInfo {
    pub fn new(id: String, role: SubagentRole, prompt: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            role,
            state: SubagentState::Running,
            prompt,
            tokens_used: 0,
            turns_executed: 0,
            started_at_secs: now,
            finished_at_secs: None,
            last_activity_secs: now,
            current_tool: None,
            status_message: Some("Initialized".to_string()),
            isolate_worktree: false,
            final_summary: None,
            todos: Vec::new(),
            file_boundaries: Vec::new(),
            inactivity_timeout_secs: crate::constants::DEFAULT_MINIPUB_INACTIVITY_TIMEOUT_SECS,
            token_budget: role.default_token_budget(),
            max_turns: role.default_max_turns(),
            pending_steer_messages: Vec::new(),
        }
    }

    pub fn with_isolation(mut self, isolate_worktree: bool) -> Self {
        self.isolate_worktree = isolate_worktree;
        self
    }

    pub fn with_todos(mut self, todos: Vec<MiniPubTodoItem>) -> Self {
        self.todos = todos;
        self
    }

    pub fn with_file_boundaries(mut self, file_boundaries: Vec<String>) -> Self {
        self.file_boundaries = file_boundaries;
        self
    }

    pub fn elapsed_secs(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.finished_at_secs
            .unwrap_or(now)
            .saturating_sub(self.started_at_secs)
    }

    pub fn last_activity_secs_ago(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.last_activity_secs)
    }

    pub fn isolation_badge(&self) -> &'static str {
        if self.isolate_worktree {
            "Worktree"
        } else if !self.file_boundaries.is_empty() {
            "File-Lease"
        } else {
            "Shared"
        }
    }
}

/// Execution outcome returned by a finished `minipub` (subagent) task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentResult {
    pub id: String,
    pub task_id: String,
    pub role: SubagentRole,
    pub success: bool,
    pub final_summary: String,
    pub tokens_used: usize,
    pub turns_executed: usize,
    pub files_inspected: Vec<String>,
    pub files_modified: Vec<String>,
    pub worktree_branch: Option<String>,
}

/// Specification for dispatching a `minipub` worker in a concurrent swarm fan-out
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentTaskSpec {
    pub role: SubagentRole,
    pub prompt: String,
    #[serde(default)]
    pub isolate_worktree: Option<bool>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub todos: Vec<String>,
    #[serde(default)]
    pub file_boundaries: Vec<String>,
}

/// First-class `MiniPub` type aliases for `minicode`'s lightweight in-process subagent system
#[allow(dead_code)]
pub type MiniPubRole = SubagentRole;
#[allow(dead_code)]
pub type MiniPubState = SubagentState;
#[allow(dead_code)]
pub type MiniPubConfig = SubagentConfig;
#[allow(dead_code)]
pub type MiniPubInfo = SubagentInfo;
#[allow(dead_code)]
pub type MiniPubResult = SubagentResult;
#[allow(dead_code)]
pub type MiniPubTaskSpec = SubagentTaskSpec;
