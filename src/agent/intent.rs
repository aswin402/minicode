use serde::{Deserialize, Serialize};

/// High-level user intent categories recognized by minicode's autonomous intent router
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentIntent {
    /// Scaffold a project stack or template via MiniKit (`/stack`)
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
    /// Surgically explore codebase AST symbols, call graph & blast radius (`/explore`)
    CodeExplore,
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
            Self::CodeExplore => ("🧭", "Code Exploration"),
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
            Self::CodeExplore => Some("/explore"),
            Self::SessionHistory => Some("/resume"),
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
    if !trimmed.starts_with('/') {
        return None;
    }

    let (cmd_token, rest) = match trimmed.find(char::is_whitespace) {
        Some(idx) => (&trimmed[..idx], trimmed[idx..].trim()),
        None => (trimmed, ""),
    };
    let cmd_lower = cmd_token.to_lowercase();

    let (intent, suggested_command, keep_query) = match cmd_lower.as_str() {
        "/stack" => (AgentIntent::StackScaffold, "/stack", true),
        "/stacks" if rest.is_empty() => (AgentIntent::StackScaffold, "/stack", false),
        "/plan" => (AgentIntent::MilestonePlan, "/plan", true),
        "/goal" => (AgentIntent::AutonomousGoal, "/goal", true),
        "/review" => (AgentIntent::CodeReview, "/review", true),
        "/diff" => (AgentIntent::GitDiff, "/diff", false),
        "/diffs" if rest.is_empty() => (AgentIntent::GitDiff, "/diff", false),
        "/explore" => (AgentIntent::CodeExplore, "/explore", true),
        "/resume" | "/sessions" | "/session" => (AgentIntent::SessionHistory, "/resume", true),
        "/history" if rest.is_empty() => (AgentIntent::SessionHistory, "/resume", false),
        "/undo" => (AgentIntent::UndoRollback, "/undo", true),
        "/map" if rest.is_empty() => (AgentIntent::RepoMap, "/map", false),
        "/compact" if rest.is_empty() => (AgentIntent::ContextCompact, "/compact", false),
        "/commands" | "/help" if rest.is_empty() => {
            (AgentIntent::CommandCatalog, "/commands", false)
        }
        _ => return None,
    };

    Some(IntentMatch {
        intent,
        confidence: clamp_confidence(1.0),
        query: if keep_query {
            rest.to_string()
        } else {
            String::new()
        },
        suggested_command: Some(suggested_command.to_string()),
    })
}

/// Clamps a confidence score to the valid [0.0, 1.0] range.
/// Applied defensively at every return site in match_intent.
#[inline]
fn clamp_confidence(val: f64) -> f32 {
    (val.clamp(0.0_f64, 1.0_f64)) as f32
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
    fn test_non_slash_prompts_return_none() {
        assert!(match_intent("scaffold a nextjs stack with onpkg").is_none());
        assert!(match_intent("create a plan for OAuth2 migration").is_none());
        assert!(match_intent("execute goal to fix all tests").is_none());
        assert!(match_intent("review my changes on git diff").is_none());
        assert!(match_intent("show past sessions").is_none());
        assert!(match_intent("show commands").is_none());
        assert!(match_intent("how do I implement quicksort in Rust?").is_none());
        assert!(match_intent("what is the time complexity of B-trees?").is_none());
    }
}
