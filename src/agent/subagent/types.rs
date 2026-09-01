use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

/// Specialized role preset for a subagent worker
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentRole {
    /// Read-only deep codebase explorer and web documentation researcher
    Researcher,
    /// Multi-axis code reviewer evaluating diffs, AST contracts, and standards
    CodeReviewer,
    /// Test runner and failure reproducer
    TestEngineer,
    /// Vulnerability, secret leak, and security policy auditor
    SecurityAuditor,
    /// Custom user-defined role with custom prompt and toolset
    Custom(String),
}

impl SubagentRole {
    /// Returns default tool whitelist for this role
    pub fn default_tool_whitelist(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        match self {
            SubagentRole::Researcher => {
                // Read-only tools
                for t in &[
                    "read_file",
                    "grep_search",
                    "locate_symbol",
                    "view_outline",
                    "fetch_or_browse",
                    "search_web",
                ] {
                    set.insert(t.to_string());
                }
            }
            SubagentRole::CodeReviewer => {
                // Inspection and analysis tools
                for t in &[
                    "read_file",
                    "grep_search",
                    "locate_symbol",
                    "view_outline",
                    "browser_snapshot",
                    "browser_eval",
                ] {
                    set.insert(t.to_string());
                }
            }
            SubagentRole::TestEngineer => {
                // Testing & diagnostic execution
                for t in &[
                    "read_file",
                    "grep_search",
                    "view_outline",
                    "exec_cmd",
                    "write_file",
                ] {
                    set.insert(t.to_string());
                }
            }
            SubagentRole::SecurityAuditor => {
                // Security inspection
                for t in &["read_file", "grep_search", "locate_symbol", "view_outline"] {
                    set.insert(t.to_string());
                }
            }
            SubagentRole::Custom(_) => {
                // All standard tools permitted by default for custom
                for t in &[
                    "read_file",
                    "write_file",
                    "patch_file",
                    "grep_search",
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

    /// Returns default max token budget for this role
    pub fn default_token_budget(&self) -> usize {
        match self {
            SubagentRole::Researcher => 24_000,
            SubagentRole::CodeReviewer => 16_000,
            SubagentRole::TestEngineer => 32_000,
            SubagentRole::SecurityAuditor => 16_000,
            SubagentRole::Custom(_) => 24_000,
        }
    }

    /// Returns default max turns for this role
    pub fn default_max_turns(&self) -> usize {
        match self {
            SubagentRole::Researcher => 12,
            SubagentRole::CodeReviewer => 6,
            SubagentRole::TestEngineer => 10,
            SubagentRole::SecurityAuditor => 6,
            SubagentRole::Custom(_) => 10,
        }
    }

    /// Returns a human-readable role badge
    pub fn badge(&self) -> &'static str {
        match self {
            SubagentRole::Researcher => "Researcher",
            SubagentRole::CodeReviewer => "CodeReviewer",
            SubagentRole::TestEngineer => "TestEngineer",
            SubagentRole::SecurityAuditor => "SecurityAuditor",
            SubagentRole::Custom(_) => "CustomWorker",
        }
    }
}

/// Lifecycle state of an active or finished subagent worker
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentState {
    Idle,
    Running,
    Completed,
    Failed(String),
    Canceled,
}

#[allow(dead_code)]
impl SubagentState {
    pub fn as_str(&self) -> &str {
        match self {
            SubagentState::Idle => "idle",
            SubagentState::Running => "running",
            SubagentState::Completed => "completed",
            SubagentState::Failed(_) => "failed",
            SubagentState::Canceled => "canceled",
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
