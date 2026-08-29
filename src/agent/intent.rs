use serde::{Deserialize, Serialize};

/// High-level user intent categories recognized by minicode's autonomous intent router
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentIntent {
    /// Scaffold a project stack or template via onpkg (`/stack`)
    StackScaffold,
    /// Break down a feature or design into structured milestones in todo.md (`/plan`)
    MilestonePlan,
    /// Execute multi-step autonomous tasks until completion (`/goal`)
    AutonomousGoal,
    /// Run multi-dimensional adversarial code review on git changes (`/review`)
    CodeReview,
    /// View or inspect git diff and modified files (`/diff`)
    GitDiff,
    /// Browse, inspect, or resume previous session trajectories (`/history`, `/sessions`)
    SessionHistory,
    /// Revert file modifications from previous turn checkpoints (`/undo`)
    UndoRollback,
    /// Render AST PageRank repository dependency graph (`/map`)
    RepoMap,
    /// Compact context tokens and summarize conversation (`/compact`)
    ContextCompact,
    /// Display interactive catalog of all available slash commands (`/commands`, `/help`)
    CommandCatalog,
    /// General engineering conversation or query
    GeneralQuery,
}

impl AgentIntent {
    /// Returns a human-friendly display label and emoji badge for the intent
    pub fn badge(&self) -> (&'static str, &'static str) {
        match self {
            Self::StackScaffold => ("🏗️", "Stack Scaffolding"),
            Self::MilestonePlan => ("📋", "Milestone Planning"),
            Self::AutonomousGoal => ("🎯", "Autonomous Goal"),
            Self::CodeReview => ("🛡️", "Adversarial Code Review"),
            Self::GitDiff => ("📊", "Git Diff Inspection"),
            Self::SessionHistory => ("📜", "Session History"),
            Self::UndoRollback => ("⏮️", "Checkpoint Undo"),
            Self::RepoMap => ("🗺️", "AST Repository Map"),
            Self::ContextCompact => ("🗜️", "Context Compaction"),
            Self::CommandCatalog => ("🧭", "Command Catalog"),
            Self::GeneralQuery => ("💬", "General Query"),
        }
    }

    /// Returns the associated primary slash command, if one exists
    #[allow(dead_code)]
    pub fn slash_command(&self) -> Option<&'static str> {
        match self {
            Self::StackScaffold => Some("/stack"),
            Self::MilestonePlan => Some("/plan"),
            Self::AutonomousGoal => Some("/goal"),
            Self::CodeReview => Some("/review"),
            Self::GitDiff => Some("/diff"),
            Self::SessionHistory => Some("/history"),
            Self::UndoRollback => Some("/undo"),
            Self::RepoMap => Some("/map"),
            Self::ContextCompact => Some("/compact"),
            Self::CommandCatalog => Some("/commands"),
            Self::GeneralQuery => None,
        }
    }
}

/// The result of an intent routing match
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentMatch {
    pub intent: AgentIntent,
    pub confidence: f32,
    pub query: String,
    pub suggested_command: Option<String>,
}

/// Fast, deterministic intent router using keyword and pattern heuristics
pub fn match_intent(input: &str) -> Option<IntentMatch> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_lowercase();

    // 1. Direct Slash Command Equivalents (1.0 Confidence)
    if lower == "/stack" || lower.starts_with("/stack ") || lower == "/stacks" {
        let query = trimmed
            .strip_prefix("/stack")
            .unwrap_or("")
            .trim()
            .to_string();
        return Some(IntentMatch {
            intent: AgentIntent::StackScaffold,
            confidence: 1.0,
            query,
            suggested_command: Some("/stack".to_string()),
        });
    }

    if lower == "/plan" || lower.starts_with("/plan ") {
        let query = trimmed
            .strip_prefix("/plan")
            .unwrap_or("")
            .trim()
            .to_string();
        return Some(IntentMatch {
            intent: AgentIntent::MilestonePlan,
            confidence: 1.0,
            query,
            suggested_command: Some("/plan".to_string()),
        });
    }

    if lower == "/goal" || lower.starts_with("/goal ") {
        let query = trimmed
            .strip_prefix("/goal")
            .unwrap_or("")
            .trim()
            .to_string();
        return Some(IntentMatch {
            intent: AgentIntent::AutonomousGoal,
            confidence: 1.0,
            query,
            suggested_command: Some("/goal".to_string()),
        });
    }

    if lower == "/review" || lower.starts_with("/review ") {
        let query = trimmed
            .strip_prefix("/review")
            .unwrap_or("")
            .trim()
            .to_string();
        return Some(IntentMatch {
            intent: AgentIntent::CodeReview,
            confidence: 1.0,
            query,
            suggested_command: Some("/review".to_string()),
        });
    }

    if lower == "/diff" || lower == "/diffs" || lower.starts_with("/diff ") {
        return Some(IntentMatch {
            intent: AgentIntent::GitDiff,
            confidence: 1.0,
            query: String::new(),
            suggested_command: Some("/diff".to_string()),
        });
    }

    if lower == "/history" || lower == "/sessions" || lower.starts_with("/sessions ") {
        return Some(IntentMatch {
            intent: AgentIntent::SessionHistory,
            confidence: 1.0,
            query: String::new(),
            suggested_command: Some("/history".to_string()),
        });
    }

    if lower == "/undo" || lower.starts_with("/undo ") {
        let query = trimmed
            .strip_prefix("/undo")
            .unwrap_or("")
            .trim()
            .to_string();
        return Some(IntentMatch {
            intent: AgentIntent::UndoRollback,
            confidence: 1.0,
            query,
            suggested_command: Some("/undo".to_string()),
        });
    }

    if lower == "/map" {
        return Some(IntentMatch {
            intent: AgentIntent::RepoMap,
            confidence: 1.0,
            query: String::new(),
            suggested_command: Some("/map".to_string()),
        });
    }

    if lower == "/compact" {
        return Some(IntentMatch {
            intent: AgentIntent::ContextCompact,
            confidence: 1.0,
            query: String::new(),
            suggested_command: Some("/compact".to_string()),
        });
    }

    if lower == "/commands" || lower == "/help" {
        return Some(IntentMatch {
            intent: AgentIntent::CommandCatalog,
            confidence: 1.0,
            query: String::new(),
            suggested_command: Some("/commands".to_string()),
        });
    }

    // 2. Natural Language Intent Pattern Heuristics (0.80 - 0.95 Confidence)

    // Stack Scaffolding
    if (lower.contains("scaffold")
        && (lower.contains("stack")
            || lower.contains("app")
            || lower.contains("project")
            || lower.contains("template")))
        || lower.contains("onpkg stack")
        || ((lower.starts_with("create ")
            || lower.starts_with("bootstrap ")
            || lower.starts_with("setup "))
            && (lower.contains("stack")
                || lower.contains("nextjs")
                || lower.contains("react")
                || lower.contains("vite")
                || lower.contains("fastapi")
                || lower.contains("flutter")
                || lower.contains("hono")))
    {
        return Some(IntentMatch {
            intent: AgentIntent::StackScaffold,
            confidence: 0.90,
            query: trimmed.to_string(),
            suggested_command: Some("/stack".to_string()),
        });
    }

    // Milestone Planning
    if (lower.starts_with("plan ")
        || lower.starts_with("create a plan")
        || lower.starts_with("make a plan")
        || lower.starts_with("break down")
        || lower.starts_with("design plan"))
        || lower.contains("implementation plan")
        || lower.contains("todo.md plan")
        || (lower.contains("plan for ") || lower.contains("plan the "))
    {
        let query = extract_planning_query(trimmed);
        return Some(IntentMatch {
            intent: AgentIntent::MilestonePlan,
            confidence: 0.88,
            query,
            suggested_command: Some("/plan".to_string()),
        });
    }

    // Autonomous Goal Execution
    if (lower.contains("autonomously")
        || lower.contains("autonomous mode")
        || lower.contains("goal mode"))
        || lower.starts_with("execute goal")
        || lower.starts_with("run goal")
        || (lower.starts_with("complete all")
            && (lower.contains("tasks") || lower.contains("todo") || lower.contains("checklist")))
    {
        return Some(IntentMatch {
            intent: AgentIntent::AutonomousGoal,
            confidence: 0.92,
            query: trimmed.to_string(),
            suggested_command: Some("/goal".to_string()),
        });
    }

    // Code Review
    if (lower.starts_with("review ")
        || lower.contains("code review")
        || lower.contains("review my changes")
        || lower.contains("review the diff")
        || lower.contains("review staged")
        || lower.contains("adversarial review"))
        && !lower.contains("plan")
    {
        return Some(IntentMatch {
            intent: AgentIntent::CodeReview,
            confidence: 0.89,
            query: trimmed.to_string(),
            suggested_command: Some("/review".to_string()),
        });
    }

    // Git Diff Inspection
    if lower == "show diff"
        || lower == "view diff"
        || lower == "show git diff"
        || lower == "inspect diff"
        || lower == "what did i change"
        || lower == "what files changed"
        || lower == "show uncommitted changes"
    {
        return Some(IntentMatch {
            intent: AgentIntent::GitDiff,
            confidence: 0.90,
            query: String::new(),
            suggested_command: Some("/diff".to_string()),
        });
    }

    // Session History & Previous Transcripts
    if (lower.contains("past session")
        || lower.contains("previous session")
        || lower.contains("session history")
        || lower.contains("chat history"))
        && (lower.contains("show")
            || lower.contains("view")
            || lower.contains("list")
            || lower.contains("browse")
            || lower.contains("open"))
    {
        return Some(IntentMatch {
            intent: AgentIntent::SessionHistory,
            confidence: 0.88,
            query: String::new(),
            suggested_command: Some("/history".to_string()),
        });
    }

    // Undo / Rollback
    if lower.starts_with("undo")
        || lower.starts_with("revert ")
        || lower.contains("revert changes")
        || lower.contains("revert last turn")
        || lower.starts_with("rollback")
    {
        return Some(IntentMatch {
            intent: AgentIntent::UndoRollback,
            confidence: 0.92,
            query: trimmed.to_string(),
            suggested_command: Some("/undo".to_string()),
        });
    }

    // Repo Map
    if lower == "show repo map"
        || lower == "render repo map"
        || lower == "show code graph"
        || lower == "ast map"
    {
        return Some(IntentMatch {
            intent: AgentIntent::RepoMap,
            confidence: 0.95,
            query: String::new(),
            suggested_command: Some("/map".to_string()),
        });
    }

    // Context Compaction
    if lower == "compact context"
        || lower == "compress context"
        || lower == "prune context"
        || lower == "compact tokens"
    {
        return Some(IntentMatch {
            intent: AgentIntent::ContextCompact,
            confidence: 0.95,
            query: String::new(),
            suggested_command: Some("/compact".to_string()),
        });
    }

    // Command Catalog / Help
    if lower == "show commands"
        || lower == "list commands"
        || lower == "what are the slash commands"
        || lower == "help menu"
    {
        return Some(IntentMatch {
            intent: AgentIntent::CommandCatalog,
            confidence: 0.90,
            query: String::new(),
            suggested_command: Some("/commands".to_string()),
        });
    }

    None
}

/// Helper to strip common conversational prefixes from planning queries
fn extract_planning_query(input: &str) -> String {
    let prefixes = [
        "plan the following implementation:",
        "plan the implementation for",
        "plan the implementation of",
        "create an implementation plan for",
        "create a plan for",
        "make an implementation plan for",
        "make a plan for",
        "break down",
        "design plan for",
        "design plan",
        "plan for",
        "plan the",
        "plan a",
        "plan",
    ];

    let lower = input.to_lowercase();
    for prefix in &prefixes {
        if lower.starts_with(prefix) {
            let rest = &input[prefix.len()..];
            return rest.trim_start_matches(':').trim().to_string();
        }
    }

    input.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slash_command_direct_matches() {
        assert_eq!(
            match_intent("/stack").map(|m| (m.intent, m.confidence)),
            Some((AgentIntent::StackScaffold, 1.0))
        );
        assert_eq!(
            match_intent("/plan auth migration").map(|m| (m.intent, m.query)),
            Some((AgentIntent::MilestonePlan, "auth migration".to_string()))
        );
        assert_eq!(
            match_intent("/goal fix all warnings").map(|m| (m.intent, m.confidence)),
            Some((AgentIntent::AutonomousGoal, 1.0))
        );
        assert_eq!(
            match_intent("/review --staged").map(|m| (m.intent, m.confidence)),
            Some((AgentIntent::CodeReview, 1.0))
        );
        assert_eq!(
            match_intent("/commands").map(|m| (m.intent, m.confidence)),
            Some((AgentIntent::CommandCatalog, 1.0))
        );
    }

    #[test]
    fn test_natural_language_intent_matching() {
        let m1 = match_intent("scaffold a nextjs stack with onpkg").unwrap();
        assert_eq!(m1.intent, AgentIntent::StackScaffold);
        assert!(m1.confidence >= 0.85);

        let m2 = match_intent("create a plan for OAuth2 migration").unwrap();
        assert_eq!(m2.intent, AgentIntent::MilestonePlan);
        assert_eq!(m2.query, "OAuth2 migration");

        let m3 = match_intent("execute goal to fix all tests").unwrap();
        assert_eq!(m3.intent, AgentIntent::AutonomousGoal);

        let m4 = match_intent("review my changes on git diff").unwrap();
        assert_eq!(m4.intent, AgentIntent::CodeReview);

        let m5 = match_intent("show past sessions").unwrap();
        assert_eq!(m5.intent, AgentIntent::SessionHistory);

        let m6 = match_intent("show commands").unwrap();
        assert_eq!(m6.intent, AgentIntent::CommandCatalog);
    }

    #[test]
    fn test_general_query_returns_none() {
        assert!(match_intent("how do I implement quicksort in Rust?").is_none());
        assert!(match_intent("what is the time complexity of B-trees?").is_none());
    }
}
