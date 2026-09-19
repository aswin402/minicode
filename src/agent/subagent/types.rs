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
        match self {
            SubagentRole::Scout => {
                for t in &[
                    "read_file",
                    "grep_search",
                    "file_search",
                    "locate_symbol",
                    "view_outline",
                    "fetch_or_browse",
                    "search_web",
                ] {
                    set.insert(t.to_string());
                }
            }
            SubagentRole::Reviewer => {
                for t in &[
                    "read_file",
                    "grep_search",
                    "file_search",
                    "locate_symbol",
                    "view_outline",
                    "browser_snapshot",
                    "browser_eval",
                ] {
                    set.insert(t.to_string());
                }
            }
            SubagentRole::Tester => {
                for t in &[
                    "read_file",
                    "grep_search",
                    "file_search",
                    "view_outline",
                    "exec_cmd",
                    "write_file",
                    "patch_file",
                ] {
                    set.insert(t.to_string());
                }
            }
            SubagentRole::Coder => {
                for t in &[
                    "read_file",
                    "write_file",
                    "patch_file",
                    "grep_search",
                    "file_search",
                    "locate_symbol",
                    "view_outline",
                    "fetch_or_browse",
                    "search_web",
                    "exec_cmd",
                ] {
                    set.insert(t.to_string());
                }
            }
        }
        set
    }

    /// Default token budget for this role
    pub fn default_token_budget(&self) -> usize {
        match self {
            SubagentRole::Scout => 24_000,
            SubagentRole::Coder => 32_000,
            SubagentRole::Tester => 32_000,
            SubagentRole::Reviewer => 16_000,
        }
    }

    /// Default max turns for this role
    pub fn default_max_turns(&self) -> usize {
        match self {
            SubagentRole::Scout => 12,
            SubagentRole::Coder => 15,
            SubagentRole::Tester => 10,
            SubagentRole::Reviewer => 6,
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
}

/// Configuration settings for instantiating a subagent worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    pub role: SubagentRole,
    pub model: Option<String>,
    pub token_budget: usize,
    pub max_turns: usize,
    pub tool_whitelist: HashSet<String>,
    pub system_prompt_override: Option<String>,
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
        }
    }
}

/// Live metadata and telemetry for a subagent worker
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
    pub current_tool: Option<String>,
    #[serde(default)]
    pub status_message: Option<String>,
    #[serde(default)]
    pub isolate_worktree: bool,
    #[serde(default)]
    pub final_summary: Option<String>,
}

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
            current_tool: None,
            status_message: Some("Initialized".to_string()),
            isolate_worktree: false,
            final_summary: None,
        }
    }
}

/// Execution outcome returned by a finished subagent task
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

/// Specification for dispatching a subagent in a concurrent swarm fan-out
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
}
