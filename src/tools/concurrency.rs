use serde::{Deserialize, Serialize};

/// Concurrency safety classification for tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSafetyLevel {
    /// Pure inspection / read-only tool. Side-effect free, idempotent,
    /// safe to run concurrently in parallel with other read-only tools.
    ReadOnly,
    /// Mutating or stateful tool (modifies files, executes shell commands,
    /// alters git state, or spawns mutating subagents).
    /// Must execute sequentially as a barrier stage.
    Mutating,
    /// Meta/control-plane tool that reconfigures the agent loop or active schemas
    /// (e.g. `activate_tools`). Must execute sequentially and immediately trigger
    /// dynamic state reloading.
    ControlBarrier,
}

impl ToolSafetyLevel {
    /// Returns true if this tool is completely read-only and safe for parallel execution.
    #[inline]
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::ReadOnly)
    }

    /// Returns true if this tool requires sequential isolation as a barrier.
    #[inline]
    pub fn is_barrier(&self) -> bool {
        matches!(self, Self::Mutating | Self::ControlBarrier)
    }

    /// Returns true if this tool is a control-plane reconfigurer.
    #[inline]
    #[allow(dead_code)]
    pub fn is_control_barrier(&self) -> bool {
        matches!(self, Self::ControlBarrier)
    }
}

/// Classifies a tool by name into its corresponding concurrency safety level.
pub fn classify_tool(name: &str) -> ToolSafetyLevel {
    if name == "activate_tools" {
        return ToolSafetyLevel::ControlBarrier;
    }

    // MCP tools default to sequential barrier execution for external safety
    if name.starts_with(crate::constants::MCP_TOOL_PREFIX) {
        return ToolSafetyLevel::Mutating;
    }

    match name {
        // Filesystem Inspection (Read-Only)
        "read_file" | "list_dir" | "file_info" | "get_transaction_status" => {
            ToolSafetyLevel::ReadOnly
        }

        // Filesystem Mutations & Transactions
        "write_file"
        | "patch_file"
        | "ast_replace_node"
        | "delete_file"
        | "create_dir"
        | "copy_file"
        | "move_file"
        | "begin_transaction"
        | "commit_transaction"
        | "rollback_transaction" => ToolSafetyLevel::Mutating,

        // Search & AST Tools (Read-Only)
        "grep_search"
        | "locate_symbol"
        | "hybrid_search"
        | "semantic_search"
        | "search_symbols_semantic"
        | "ast_query"
        | "ast_extract_symbol"
        | "ast_diff"
        | "locate_fault" => ToolSafetyLevel::ReadOnly,

        // Exploration & CodeGraph Tools (Read-Only)
        "code_explore"
        | "diff_impact"
        | "blast_radius"
        | "get_ast_outline"
        | "score_task_complexity"
        | "check_architecture"
        | "impact_analysis" => ToolSafetyLevel::ReadOnly,

        // Git & GitHub Inspection (Read-Only)
        "git_status" | "git_diff" | "git_log" | "git_conflicts" | "git_review"
        | "git_branch_list" | "git_show" | "github_issue_view" | "github_issue_list"
        | "github_pr_view" | "github_pr_diff" | "github_ci_status" | "github_ci_logs" => {
            ToolSafetyLevel::ReadOnly
        }

        // Git & GitHub Mutations
        "git_commit"
        | "git_checkout"
        | "git_branch"
        | "git_reset"
        | "git_stash"
        | "create_pr"
        | "github_issue_create"
        | "github_pr_create"
        | "resolve_git_conflicts" => ToolSafetyLevel::Mutating,

        // Terminal / Command Execution (Mutating Barrier)
        "exec_cmd" => ToolSafetyLevel::Mutating,

        // Web & Browser Tools
        "search_web" | "fetch_or_browse" | "browser_snapshot" => ToolSafetyLevel::ReadOnly,
        "browser_navigate" => ToolSafetyLevel::Mutating,

        // onpkg Scaffolding Tools
        "onpkg_stack_list" => ToolSafetyLevel::ReadOnly,
        "onpkg_stack_add" | "onpkg_pkg_add" => ToolSafetyLevel::Mutating,

        // Multi-Agent & Reasoning Inspection
        "explore_hypotheses"
        | "evaluate_branch"
        | "select_best_branch"
        | "prune_context"
        | "critic_review"
        | "sequential_thinking"
        | "list_reproducers" => ToolSafetyLevel::ReadOnly,

        // Multi-Agent Mutations
        "dispatch_subagent"
        | "fanout_subagents"
        | "merge_subagent_worktree"
        | "synthesize_reproducer"
        | "verify_reproducer" => ToolSafetyLevel::Mutating,

        // Memory & Context Inspection (Read-Only)
        "read_plan"
        | "view_memory"
        | "inspect_skill"
        | "list_skills"
        | "lsp_diagnostics"
        | "lsp_goto_definition"
        | "lsp_find_references"
        | "wiki_read"
        | "wiki_search"
        | "get_next_task" => ToolSafetyLevel::ReadOnly,

        // Memory & Context Mutations
        "remember_fact" | "update_fact" | "forget_fact" | "create_plan" | "log_finding"
        | "update_progress" | "archive_plan" | "create_skill" | "wiki_write"
        | "create_task_dag" | "complete_task" => ToolSafetyLevel::Mutating,

        // Fail-safe default: treat any unrecognized tool as Mutating barrier
        _ => ToolSafetyLevel::Mutating,
    }
}

/// Convenience helper to check if a tool is read-only.
#[inline]
#[allow(dead_code)]
pub fn is_read_only(name: &str) -> bool {
    classify_tool(name).is_read_only()
}

/// Convenience helper to check if a tool requires a sequential barrier.
#[inline]
#[allow(dead_code)]
pub fn is_barrier(name: &str) -> bool {
    classify_tool(name).is_barrier()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_only_tools_classified() {
        assert_eq!(classify_tool("read_file"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("grep_search"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("locate_symbol"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("locate_fault"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("git_status"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("git_diff"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("search_web"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("code_explore"), ToolSafetyLevel::ReadOnly);
        assert!(is_read_only("read_file"));
        assert!(!is_barrier("read_file"));
    }

    #[test]
    fn test_mutating_tools_classified() {
        assert_eq!(classify_tool("write_file"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("patch_file"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("ast_replace_node"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("exec_cmd"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("git_commit"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("fanout_subagents"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("create_plan"), ToolSafetyLevel::Mutating);
        assert!(!is_read_only("write_file"));
        assert!(is_barrier("write_file"));
    }

    #[test]
    fn test_control_barrier_classified() {
        assert_eq!(
            classify_tool("activate_tools"),
            ToolSafetyLevel::ControlBarrier
        );
        assert!(is_barrier("activate_tools"));
        assert!(classify_tool("activate_tools").is_control_barrier());
    }

    #[test]
    fn test_mcp_tools_default_to_mutating() {
        assert_eq!(
            classify_tool("mcp__github__get_issue"),
            ToolSafetyLevel::Mutating
        );
        assert!(is_barrier("mcp__server__tool"));
    }

    #[test]
    fn test_unknown_tools_default_to_mutating() {
        assert_eq!(classify_tool("some_future_tool"), ToolSafetyLevel::Mutating);
        assert!(is_barrier("some_future_tool"));
    }

    #[test]
    fn test_all_registered_schemas_have_classification() {
        let schemas = crate::tools::ToolRegistry::get_tool_schemas();
        let mut read_only_count = 0;
        let mut mutating_count = 0;
        let mut barrier_count = 0;

        for s in &schemas {
            let safety = classify_tool(&s.name);
            match safety {
                ToolSafetyLevel::ReadOnly => read_only_count += 1,
                ToolSafetyLevel::Mutating => mutating_count += 1,
                ToolSafetyLevel::ControlBarrier => barrier_count += 1,
            }
        }

        assert!(
            read_only_count > 20,
            "Expected substantial read-only tools, found {}",
            read_only_count
        );
        assert!(
            mutating_count > 10,
            "Expected substantial mutating tools, found {}",
            mutating_count
        );
        assert_eq!(
            read_only_count + mutating_count + barrier_count,
            schemas.len()
        );
    }
}
