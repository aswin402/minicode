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
    ("view_outline", "look_at"),
];

/// Tools that spawn subagents, swarms, or DAG workers (delegated LLM execution).
pub const DELEGATION_TOOLS: &[&str] = &[
    "delegate_task",
    "fanout_subagents",
    "execute_task_dag",
    "power_worktree_task",
];

/// Read-only local workspace query tools eligible for per-session fetch/read deduplication.
pub const LOCAL_QUERY_TOOLS: &[&str] = &[
    "read_file",
    "look_at",
    "batch_read",
    "grep_search",
    "file_search",
    "locate_symbol",
    "code_explore",
];

/// External web / documentation fetch tools whose results stay valid across local file edits.
pub const WEB_FETCH_TOOLS: &[&str] = &[
    "search_web",
    "webfetch",
    "crawl_documentation",
    "crawl_sitemap",
    "search_crawled_docs",
];

/// Non-read-only or barrier tools that do NOT mutate workspace source files on disk (so they
/// must not flush the local read deduplication cache).
pub const NON_WORKSPACE_MUTATIONS: &[&str] = &[
    "ask_user",
    "power_review",
    "power_verify",
    "block_search",
    "block_get",
    "block_palettes",
    "activate_tools",
];

/// Declarative UI display title table mapping tool identifiers (and legacy aliases) to concise
/// human-readable badge labels. Tools not listed fall back to `Title Case` split on `_`.
pub const DISPLAY_TITLE_RULES: &[(&[&str], &str)] = &[
    (&["exec_cmd", "bash", "run_command", "shell", "cmd"], "Bash"),
    (&["read_file", "cat", "view_file"], "Read File"),
    (&["batch_read"], "Batch Read"),
    (&["look_at", "view_outline"], "Look At"),
    (&["write_file", "create_file"], "Write File"),
    (
        &["patch_file", "edit_file", "replace_file_content"],
        "Edit File",
    ),
    (&["list_directory", "list_dir", "ls"], "List Dir"),
    (&["grep_search", "search_code", "grep"], "Grep Search"),
    (
        &["find_files", "find_by_name", "locate_files", "file_search"],
        "Find Files",
    ),
    (&["file_info", "stat_file"], "File Info"),
    (&["make_directory", "mkdir"], "Make Dir"),
    (&["remove_file", "rm"], "Remove File"),
    (&["webfetch"], "Web Fetch"),
    (&["web_search", "search_web"], "Web Search"),
    (&["git_status"], "Git Status"),
    (&["git_diff"], "Git Diff"),
    (&["git_commit"], "Git Commit"),
    (&["git_log"], "Git Log"),
    (&["git_branch"], "Git Branch"),
    (&["code_graph", "symbol_graph"], "Code Graph"),
    (&["semantic_search"], "Semantic Search"),
    (
        &["explore_symbol", "symbol_definition", "locate_symbol"],
        "Explore Symbol",
    ),
    (&["blast_radius"], "Blast Radius"),
    (&["kit_doctor", "onpkg_doctor"], "MiniKit Doctor"),
    (
        &["kit_stack_list", "onpkg_stack_list"],
        "MiniKit Stack List",
    ),
    (&["kit_stack_add", "onpkg_stack_add"], "MiniKit Stack Add"),
    (&["kit_stack_new", "onpkg_stack_new"], "MiniKit Stack New"),
    (
        &["kit_stack_remove", "onpkg_stack_remove"],
        "MiniKit Stack Remove",
    ),
    (
        &["kit_stack_diff", "onpkg_stack_diff"],
        "MiniKit Stack Diff",
    ),
    (
        &["kit_skill_remove", "onpkg_skill_remove"],
        "MiniKit Skill Remove",
    ),
    (
        &["kit_info", "kit_pkg_info", "onpkg_pkg_info"],
        "MiniKit Package Info",
    ),
    (
        &["kit_add", "kit_pkg_add", "onpkg_pkg_add"],
        "MiniKit Package Add",
    ),
    (
        &["kit_remove", "kit_pkg_remove", "onpkg_pkg_remove"],
        "MiniKit Package Remove",
    ),
    (&["spawn_subagent", "delegate_task"], "Spawn Subagent"),
    (&["send_subagent_message"], "Message Subagent"),
    (&["begin_transaction"], "Begin Transaction"),
    (&["commit_transaction"], "Commit Transaction"),
    (&["rollback_transaction"], "Rollback Transaction"),
];

/// Bitflags describing the cross-cutting behavioral semantics of a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolEffectFlags(u16);

impl ToolEffectFlags {
    pub const NONE: Self = Self(0);
    pub const FILE_MUTATION: Self = Self(1 << 0);
    pub const DELEGATION: Self = Self(1 << 1);
    pub const STRICT_APPROVAL: Self = Self(1 << 2);
    pub const DISCOVERY_SAFE: Self = Self(1 << 3);
    pub const LOCAL_QUERY: Self = Self(1 << 4);
    pub const WEB_FETCH: Self = Self(1 << 5);
    pub const NON_WORKSPACE_MUTATION: Self = Self(1 << 6);

    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[inline]
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

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

/// Computes the combined [`ToolEffectFlags`] bitmask for `name` (after alias resolution).
#[must_use]
pub fn effect_flags(name: &str) -> ToolEffectFlags {
    let canonical = canonical_name(name);
    let mut flags = ToolEffectFlags::NONE;
    if FILE_MODIFYING_TOOLS.contains(&canonical) {
        flags = flags.union(ToolEffectFlags::FILE_MUTATION);
    }
    if DELEGATION_TOOLS.contains(&canonical) {
        flags = flags.union(ToolEffectFlags::DELEGATION);
    }
    if APPROVAL_REQUIRED_TOOLS.contains(&canonical) {
        flags = flags.union(ToolEffectFlags::STRICT_APPROVAL);
    }
    if DISCOVERY_SAFE_MUTATIONS.contains(&canonical) {
        flags = flags.union(ToolEffectFlags::DISCOVERY_SAFE);
    }
    if LOCAL_QUERY_TOOLS.contains(&canonical) {
        flags = flags.union(ToolEffectFlags::LOCAL_QUERY);
    }
    if WEB_FETCH_TOOLS.contains(&canonical) {
        flags = flags.union(ToolEffectFlags::WEB_FETCH);
    }
    if NON_WORKSPACE_MUTATIONS.contains(&canonical)
        || DISCOVERY_SAFE_MUTATIONS.contains(&canonical)
        || canonical.starts_with("browser_")
    {
        flags = flags.union(ToolEffectFlags::NON_WORKSPACE_MUTATION);
    }
    flags
}

/// Returns true if `name` is a local read/search query tool eligible for fetch deduplication.
#[must_use]
pub fn is_local_query(name: &str) -> bool {
    effect_flags(name).contains(ToolEffectFlags::LOCAL_QUERY)
}

/// Returns true if `name` is an external web/documentation fetch tool.
#[must_use]
pub fn is_web_fetch(name: &str) -> bool {
    effect_flags(name).contains(ToolEffectFlags::WEB_FETCH)
}

/// Returns true if executing `name` can modify workspace files on disk and must therefore
/// invalidate cached local read deduplication state.
#[must_use]
pub fn invalidates_local_reads(name: &str) -> bool {
    let flags = effect_flags(name);
    if flags.contains(ToolEffectFlags::LOCAL_QUERY)
        || flags.contains(ToolEffectFlags::WEB_FETCH)
        || flags.contains(ToolEffectFlags::NON_WORKSPACE_MUTATION)
    {
        return false;
    }
    !crate::tools::concurrency::classify_tool(canonical_name(name)).is_read_only()
}

/// Formats an internal tool identifier into a human-readable UI badge title using the
/// declarative [`DISPLAY_TITLE_RULES`] table, falling back to Title Case on `_` boundaries.
#[must_use]
pub fn format_display_title(raw_name: &str) -> String {
    for (names, title) in DISPLAY_TITLE_RULES {
        if names.contains(&raw_name) {
            return (*title).to_string();
        }
    }
    raw_name
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    fn test_local_query_and_web_fetch_tools_are_registered() {
        assert_all_registered("LOCAL_QUERY_TOOLS", LOCAL_QUERY_TOOLS);
        assert_all_registered("WEB_FETCH_TOOLS", WEB_FETCH_TOOLS);
        assert_all_registered("NON_WORKSPACE_MUTATIONS", NON_WORKSPACE_MUTATIONS);
    }

    #[test]
    fn test_effect_flags_and_display_titles() {
        assert!(is_local_query("read_file"));
        assert!(is_local_query("view_outline"));
        assert!(is_web_fetch("webfetch"));
        assert!(!invalidates_local_reads("browser_screenshot"));
        assert!(!invalidates_local_reads("create_plan"));
        assert!(invalidates_local_reads("patch_file"));
        assert_eq!(format_display_title("exec_cmd"), "Bash");
        assert_eq!(format_display_title("custom_tool_xyz"), "Custom Tool Xyz");
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
