//! Single source of truth for cross-cutting tool-name metadata.
//!
//! Other subsystems (agent loop guards, forensics, budget analytics, compaction) must query
//! these helpers instead of keeping their own hardcoded tool-name lists, so that a renamed or
//! removed tool cannot silently drift out of sync. The unit tests below assert that every name
//! referenced here is an actually registered tool schema.

use crate::constants::{APPROVAL_REQUIRED_TOOLS, FILE_MODIFYING_TOOLS};
use crate::tools::category::ToolCategory;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Foreign / legacy tool names (from other agents' conventions or older prompts) mapped to the
/// canonical registered tool. Only argument-compatible pairs are listed, because
/// [`crate::tools::ToolRegistry::dispatch`] rewrites these names before execution.
pub const TOOL_ALIASES: &[(&str, &str)] = &[
    ("edit_file", "patch_file"),
    ("view_file", "read_file"),
    ("grep", "grep_search"),
    ("symbol_search", "locate_symbol"),
    ("bash", "exec_cmd"),
    ("spawn_agent", "delegate_task"),
    ("invoke_subagent", "delegate_task"),
    ("dispatch_subagent", "delegate_task"),
    ("spawn_subagent", "delegate_task"),
    ("subagent_transcript_drilldown", "manage_subagents"),
];

/// Tools that spawn subagents, swarms, or DAG workers (delegated LLM execution).
pub const DELEGATION_TOOLS: &[&str] = &[
    "delegate_task",
    "fanout_subagents",
    "execute_task_dag",
    "power_worktree_task",
];

/// Mutating (non read-only) tools that remain allowed while the planning-phase discovery
/// write-guard is active: planning/decision bookkeeping, skill loading and mockup viewing.
/// Everything else classified as mutating is denied until a plan is validated.
pub const DISCOVERY_SAFE_MUTATIONS: &[&str] = &[
    "create_plan",
    "power_plan",
    "update_progress",
    "archive_plan",
    "record_decision",
    "remember_fact",
    "update_fact",
    "forget_fact",
    "log_finding",
    "power_speckit",
    "vault_load",
    "vault_unload",
    "checkpoint_session",
    "browser_navigate",
    "browser_close",
];

/// Resolves a foreign/legacy alias to its canonical registered tool name.
/// Returns the input unchanged when it is not an alias.
#[must_use]
pub fn canonical_name(name: &str) -> &str {
    TOOL_ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map_or(name, |(_, canonical)| canonical)
}

/// Returns true if the tool (or its alias) directly modifies workspace files.
#[must_use]
pub fn is_file_mutation(name: &str) -> bool {
    FILE_MODIFYING_TOOLS.contains(&canonical_name(name))
}

/// Returns true if the tool (or its alias) delegates work to subagents / swarm / DAG workers.
#[must_use]
pub fn is_delegation(name: &str) -> bool {
    DELEGATION_TOOLS.contains(&canonical_name(name))
}

/// Returns true if the tool (or its alias) requires approval in strict (non auto-approve) mode.
#[must_use]
pub fn requires_strict_approval(name: &str) -> bool {
    APPROVAL_REQUIRED_TOOLS.contains(&canonical_name(name))
}

/// Returns true if the (mutating) tool stays allowed during the discovery write-guard.
#[must_use]
pub fn is_discovery_safe(name: &str) -> bool {
    DISCOVERY_SAFE_MUTATIONS.contains(&canonical_name(name))
}

/// Lazily built map of every registered built-in tool name to its owning category.
fn registry_index() -> &'static HashMap<String, ToolCategory> {
    static INDEX: OnceLock<HashMap<String, ToolCategory>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map = HashMap::new();
        for cat in ToolCategory::ALL {
            for schema in cat.get_schemas() {
                map.insert(schema.name, cat);
            }
        }
        map
    })
}

/// Returns true if `name` is a registered built-in tool (aliases are not resolved).
#[must_use]
pub fn is_registered(name: &str) -> bool {
    name == "activate_tools" || registry_index().contains_key(name)
}

/// Returns the category owning the tool (after alias resolution), if registered.
#[must_use]
pub fn category_of(name: &str) -> Option<ToolCategory> {
    registry_index().get(canonical_name(name)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_all_registered(label: &str, names: &[&str]) {
        let missing: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| !is_registered(n))
            .collect();
        assert!(
            missing.is_empty(),
            "{} references unregistered tools: {:?}",
            label,
            missing
        );
    }

    #[test]
    fn test_file_modifying_tools_are_registered() {
        assert_all_registered("FILE_MODIFYING_TOOLS", FILE_MODIFYING_TOOLS);
    }

    #[test]
    fn test_approval_required_tools_are_registered() {
        assert_all_registered("APPROVAL_REQUIRED_TOOLS", APPROVAL_REQUIRED_TOOLS);
    }

    #[test]
    fn test_delegation_tools_are_registered() {
        assert_all_registered("DELEGATION_TOOLS", DELEGATION_TOOLS);
    }

    #[test]
    fn test_discovery_safe_mutations_are_registered() {
        assert_all_registered("DISCOVERY_SAFE_MUTATIONS", DISCOVERY_SAFE_MUTATIONS);
    }

    #[test]
    fn test_aliases_target_registered_tools_and_do_not_shadow() {
        for (alias, canonical) in TOOL_ALIASES {
            assert!(
                is_registered(canonical),
                "alias `{}` targets unregistered tool `{}`",
                alias,
                canonical
            );
            assert!(
                !is_registered(alias),
                "alias `{}` shadows a registered tool",
                alias
            );
        }
    }

    #[test]
    fn test_subagent_dispatch_tools_are_delegation_tools() {
        for name in [
            "dispatch_subagent",
            "delegate_task",
            "invoke_subagent",
            "spawn_subagent",
        ] {
            assert!(crate::tools::concurrency::is_subagent_dispatch_tool(name));
            assert!(is_delegation(name), "{} must be a delegation tool", name);
        }
    }

    #[test]
    fn test_alias_resolution_helpers() {
        assert_eq!(canonical_name("edit_file"), "patch_file");
        assert_eq!(canonical_name("read_file"), "read_file");
        assert!(is_file_mutation("edit_file"));
        assert!(!is_file_mutation("read_file"));
        assert!(is_delegation("spawn_agent"));
        assert!(!is_delegation("minitask"));
        assert!(requires_strict_approval("bash"));
        assert_eq!(category_of("view_file"), Some(ToolCategory::Files));
    }
}
