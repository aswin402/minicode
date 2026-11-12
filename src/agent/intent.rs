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

/// Determines if a user prompt or matched intent represents a CRUD, code-editing,
/// mutation, or codebase-inspection activity that warrants code-graph indexing or drift checks.
///
/// General queries (e.g. "what is a mutex?", "explain python decorators") and non-mutating
/// UI commands (e.g. `/help`, `/model`, `/theme`) return `false`, bypassing index prompts.
pub fn is_repository_crud_intent(prompt: &str, matched_intent: Option<&IntentMatch>) -> bool {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_lowercase();

    // 1. Slash commands: filter out non-codebase utility commands
    if lower.starts_with('/') {
        let first_word = lower.split_whitespace().next().unwrap_or("");
        match first_word {
            "/help" | "/commands" | "/model" | "/models" | "/theme" | "/themes" | "/context"
            | "/tokens" | "/cost" | "/costs" | "/status" | "/exit" | "/quit" | "/clear"
            | "/setup" | "/configure" | "/config" | "/compact" | "/settings" | "/preferences" => {
                return false;
            }
            "/init" | "/index" | "/analyze" | "/map" | "/explore" | "/diff" | "/undo"
            | "/review" | "/plan" | "/goal" | "/stack" | "/stacks" => {
                return true;
            }
            _ => {}
        }
    }

    // 2. Recognized autonomous agent intents
    if let Some(m) = matched_intent {
        match m.intent {
            AgentIntent::StackScaffold
            | AgentIntent::MilestonePlan
            | AgentIntent::AutonomousGoal
            | AgentIntent::CodeReview
            | AgentIntent::GitDiff
            | AgentIntent::UndoRollback
            | AgentIntent::RepoMap
            | AgentIntent::CodeExplore => return true,
            AgentIntent::CommandCatalog
            | AgentIntent::ContextCompact
            | AgentIntent::SessionHistory
            | AgentIntent::GeneralQuery => {}
        }
    }

    // 3. Explicit full project analysis requests
    if lower == "analyze" || is_repo_map_analysis_command(trimmed) {
        return true;
    }

    // 4. Structural file path or syntax-scope/manifest reference via WorkspaceProfile
    let has_file_reference = trimmed.split_whitespace().any(|token| {
        let raw_unquoted = token.trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '`' | ',' | ';' | ':' | '(' | ')' | '[' | ']' | '?' | '!'
            )
        });
        let clean = token.trim_matches(|c: char| c.is_ascii_punctuation());
        if is_structural_file_path(raw_unquoted, clean) {
            return true;
        }
        if clean.contains('.') {
            let p = std::path::Path::new(clean);
            crate::context::workspace_profile::WorkspaceProfile::is_code_path(p)
                || crate::context::workspace_profile::WorkspaceProfile::is_doc_path(p)
                || crate::context::workspace_profile::PROJECT_MANIFESTS
                    .iter()
                    .any(|m| m.eq_ignore_ascii_case(clean))
        } else {
            clean.eq_ignore_ascii_case("dockerfile") || clean.eq_ignore_ascii_case("makefile")
        }
    });

    if has_file_reference {
        return true;
    }

    // 5. Expository / interrogative general-knowledge clauses without workspace deictics bypass indexing
    let first_token = lower.split_whitespace().next().unwrap_or_default();
    let is_expository_opening = matches!(
        first_token,
        "what" | "what's" | "how" | "why" | "explain" | "tell" | "difference"
    ) || lower.starts_with("can you explain");
    if is_expository_opening {
        let has_workspace_deictic = lower.contains("this project")
            || lower.contains("this repo")
            || lower.contains("our codebase");
        if !has_workspace_deictic {
            return false;
        }
    }

    // 6. Delegate task-intent scoring to IntentClassifier + RoleBm25Index (zero hardcoded noun/verb tables)
    let signal =
        crate::context::search::intent_filter::IntentClassifier::analyze_signal(trimmed, None);
    if signal.action_score == 0 {
        return false;
    }
    if signal.domain_score > 0 || signal.code_morphology_score > 0 {
        return true;
    }
    let (_, role_score) =
        crate::agent::subagent::SubagentRole::infer_from_intent_scored(trimmed, None);
    role_score > 0
}

/// Structurally classifies whether a whitespace-delimited token is a filesystem path
/// rather than a conversational slash-pair (e.g. `TCP/IP`, `CI/CD`, `client/server`, `async/await`).
fn is_structural_file_path(raw_unquoted: &str, clean: &str) -> bool {
    if !raw_unquoted.contains('/') && !raw_unquoted.contains('\\') {
        return false;
    }
    if raw_unquoted.starts_with("http://") || raw_unquoted.starts_with("https://") {
        return false;
    }
    if raw_unquoted.starts_with("./")
        || raw_unquoted.starts_with("../")
        || raw_unquoted.starts_with("~/")
    {
        return true;
    }
    // Any slash-separated token with a file extension (e.g. `foo/bar.rs`, `a/b.json`)
    if clean
        .rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && !ext.is_empty() && ext.len() <= 10)
    {
        return true;
    }
    let segments: Vec<&str> = clean.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    // Multi-level directory hierarchy (e.g. `src/ui/view`, `a/b/c`)
    if segments.len() >= 3 {
        return true;
    }
    if let Some(&first) = segments.first() {
        let lower_first = first.to_ascii_lowercase();
        if matches!(
            lower_first.as_str(),
            "src"
                | "lib"
                | "tests"
                | "benches"
                | "examples"
                | "crates"
                | "packages"
                | "app"
                | "pages"
                | "components"
                | "routes"
                | "api"
                | "cmd"
                | "pkg"
                | "internal"
                | "scripts"
                | "config"
                | "docs"
                | "public"
                | "assets"
                | "styles"
        ) {
            return true;
        }
    }
    false
}

/// Token-based classifier for direct repository analysis / CodeGraph indexing commands
/// (e.g. "analyze the full project", "index codebase", "rebuild the graph", "reindex").
#[must_use]
pub fn is_repo_map_analysis_command(input: &str) -> bool {
    let lower = input.trim().to_ascii_lowercase();
    if matches!(lower.as_str(), "reindex" | "re-index") {
        return true;
    }

    let tokens: Vec<&str> = lower
        .split_whitespace()
        .filter(|t| {
            !matches!(
                *t,
                "the" | "full" | "entire" | "whole" | "our" | "this" | "code"
            )
        })
        .collect();

    matches!(
        tokens.as_slice(),
        [
            "analyze" | "index" | "reindex" | "re-index" | "rebuild" | "generate",
            "project" | "repo" | "repository" | "codebase" | "workspace" | "graph"
        ]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crud_intent_classification() {
        // Positive cases: file references and mutations
        assert!(is_repository_crud_intent("add login in auth.rs", None));
        assert!(is_repository_crud_intent(
            "fix the typo in src/main.rs",
            None
        ));
        assert!(is_repository_crud_intent("refactor the sql queries", None));
        assert!(is_repository_crud_intent("create a user service", None));
        assert!(is_repository_crud_intent("delete deprecated route", None));
        assert!(is_repository_crud_intent("analyze the full project", None));
        assert!(is_repository_crud_intent("/explore", None));
        assert!(is_repository_crud_intent("/diff", None));
        assert!(is_repository_crud_intent("/undo", None));
        assert!(is_repository_crud_intent("/init", None));

        // Negative cases: general queries and utilities
        assert!(!is_repository_crud_intent("what is a mutex", None));
        assert!(!is_repository_crud_intent(
            "explain python decorators",
            None
        ));
        assert!(!is_repository_crud_intent(
            "how does rust ownership work",
            None
        ));
        assert!(!is_repository_crud_intent("hello there", None));
        assert!(!is_repository_crud_intent("/help", None));
        assert!(!is_repository_crud_intent("/model", None));
        assert!(!is_repository_crud_intent("/theme", None));
        assert!(!is_repository_crud_intent("/context", None));
        assert!(!is_repository_crud_intent(
            "what is the difference between TCP/IP and UDP?",
            None
        ));
        assert!(!is_repository_crud_intent("explain CI/CD pipelines", None));
        assert!(!is_repository_crud_intent(
            "what is I/O multiplexing?",
            None
        ));
        assert!(!is_repository_crud_intent(
            "explain client/server architecture",
            None
        ));
    }

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
