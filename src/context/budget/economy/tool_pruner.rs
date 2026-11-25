//! Experience-driven tool schema pruning across repository sessions.
//!
//! Inspired by `microsoft/TokenEconomizer`'s dual-threshold tool pruner (`p=10` prompts across
//! `m=2` sessions, OR `s=5` sessions unused), protecting core/human-centric tools and supporting
//! instant on-demand restoration via `activate_tools` or `minicode economy restore`.

use crate::agent::provider::ToolSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

/// Relative path under workspace root for per-tool usage telemetry.
pub const TOOL_USAGE_REL_PATH: &str = ".minicode/economy/tool_usage.json";

/// Dual pruning thresholds matching TokenEconomizer defaults:
/// Prune an eligible non-essential tool if unused across:
///   (a) >= `MIN_PROMPTS_FOR_PRUNE` prompts spanning >= `MIN_SESSIONS_FOR_PRUNE` distinct sessions, OR
///   (b) >= `ABSOLUTE_SESSIONS_FOR_PRUNE` distinct sessions regardless of prompt count.
pub const MIN_PROMPTS_FOR_PRUNE: u64 = 10;
pub const MIN_SESSIONS_FOR_PRUNE: usize = 2;
pub const ABSOLUTE_SESSIONS_FOR_PRUNE: usize = 5;

/// Core and human-centric tools that are NEVER automatically pruned.
const PROTECTED_TOOLS: &[&str] = &[
    "read_file",
    "batch_read",
    "look_at",
    "write_file",
    "patch_file",
    "exec_cmd",
    "grep_search",
    "file_search",
    "search_files",
    "list_files",
    "locate_symbol",
    "retrieve_observation",
    "scratchpad_read",
    "scratchpad_write",
    "send_worker_message",
    "ask_user",
    "create_plan",
    "update_progress",
    "archive_plan",
    "record_decision",
    "activate_tools",
    "power_verify",
    "power_review",
    "git_status",
    "git_diff",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolUsageEntry {
    pub invocations: u64,
    pub prompts_unused: u64,
    #[serde(default)]
    pub sessions_unused: BTreeSet<String>,
    pub pruned: bool,
    #[serde(default)]
    pub manually_restored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
}

/// Persistent cross-session tool utilization store and schema pruner.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolUsagePruner {
    #[serde(default)]
    pub tools: BTreeMap<String, ToolUsageEntry>,
}

impl ToolUsagePruner {
    fn store_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(TOOL_USAGE_REL_PATH)
    }

    #[must_use]
    pub fn load(workspace_root: &Path) -> Self {
        let path = Self::store_path(workspace_root);
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, workspace_root: &Path) {
        let path = Self::store_path(workspace_root);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    #[must_use]
    pub fn is_protected(tool_name: &str) -> bool {
        PROTECTED_TOOLS.contains(&tool_name)
    }

    /// Prunes cold non-essential tool schemas from `schemas`, preserving all protected tools and
    /// any tool in `explicitly_requested` (e.g. `activate_tools` or skill `allowed_tools`).
    /// Returns `(pruned_tool_names, estimated_tokens_saved)`.
    pub fn filter_schemas(
        &self,
        schemas: &mut Vec<ToolSchema>,
        explicitly_requested: &HashSet<String>,
    ) -> (Vec<String>, usize) {
        let mut pruned_names = Vec::new();
        let mut tokens_saved = 0usize;

        schemas.retain(|schema| {
            let name = schema.name.as_str();
            if Self::is_protected(name) || explicitly_requested.contains(name) {
                return true;
            }
            if let Some(entry) = self.tools.get(name) {
                if entry.pruned && !entry.manually_restored {
                    let est_chars = schema.name.len()
                        + schema.description.len()
                        + schema.parameters.to_string().len();
                    tokens_saved = tokens_saved.saturating_add((est_chars / 4).max(40));
                    pruned_names.push(schema.name.clone());
                    return false;
                }
            }
            true
        });

        (pruned_names, tokens_saved)
    }

    /// Records a completed prompt turn: updates invocation counts for tools actually called and
    /// increments unused counters for exposed non-protected tools that were never invoked.
    pub fn record_turn(
        &mut self,
        session_id: &str,
        exposed_tools: &[String],
        invoked_tools: &[String],
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let invoked_set: HashSet<&str> = invoked_tools.iter().map(String::as_str).collect();

        // Any invoked tool is immediately restored and its unused streak is reset
        for name in &invoked_set {
            let entry = self.tools.entry((*name).to_string()).or_default();
            entry.invocations = entry.invocations.saturating_add(1);
            entry.prompts_unused = 0;
            entry.sessions_unused.clear();
            entry.pruned = false;
            entry.last_used_at = Some(now.clone());
        }

        // Exposed but unused non-protected tools advance toward the dual pruning threshold
        for name in exposed_tools {
            if Self::is_protected(name) || invoked_set.contains(name.as_str()) {
                continue;
            }
            let entry = self.tools.entry(name.clone()).or_default();
            if entry.manually_restored {
                continue;
            }
            entry.prompts_unused = entry.prompts_unused.saturating_add(1);
            entry.sessions_unused.insert(session_id.to_string());
            let sessions_count = entry.sessions_unused.len();
            if (entry.prompts_unused >= MIN_PROMPTS_FOR_PRUNE
                && sessions_count >= MIN_SESSIONS_FOR_PRUNE)
                || sessions_count >= ABSOLUTE_SESSIONS_FOR_PRUNE
            {
                entry.pruned = true;
            }
        }
    }

    /// Returns currently pruned tool names in deterministic alphabetical order.
    #[must_use]
    pub fn pruned_tools(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter(|(_, e)| e.pruned && !e.manually_restored)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Restores either a single named tool or all pruned tools. Returns count restored.
    pub fn restore(&mut self, tool_name: Option<&str>) -> usize {
        let mut count = 0usize;
        match tool_name {
            Some(target) => {
                if let Some(entry) = self.tools.get_mut(target) {
                    if entry.pruned {
                        count = 1;
                    }
                    entry.pruned = false;
                    entry.prompts_unused = 0;
                    entry.sessions_unused.clear();
                    entry.manually_restored = true;
                }
            }
            None => {
                for entry in self.tools.values_mut() {
                    if entry.pruned {
                        count += 1;
                    }
                    entry.pruned = false;
                    entry.prompts_unused = 0;
                    entry.sessions_unused.clear();
                    entry.manually_restored = true;
                }
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dummy_schema(name: &str) -> ToolSchema {
        ToolSchema {
            name: name.to_string(),
            description: format!("Tool description for {name} with schema details"),
            parameters: json!({"type": "object", "properties": {"a": {"type": "string"}}}),
        }
    }

    #[test]
    fn test_dual_threshold_pruning_protects_core_and_restores_cleanly() {
        let mut pruner = ToolUsagePruner::default();
        let exposed = vec![
            "read_file".to_string(),
            "vault_bundle_show".to_string(),
            "browser_mock_route".to_string(),
        ];

        // 5 prompts in session 1 + 5 prompts in session 2 without invoking unused tools
        for _ in 0..5 {
            pruner.record_turn("s1", &exposed, &["read_file".to_string()]);
        }
        assert!(
            pruner.pruned_tools().is_empty(),
            "single session (< 2 sessions) must not prune yet"
        );

        for _ in 0..5 {
            pruner.record_turn("s2", &exposed, &["read_file".to_string()]);
        }
        let pruned = pruner.pruned_tools();
        assert!(pruned.contains(&"vault_bundle_show".to_string()));
        assert!(pruned.contains(&"browser_mock_route".to_string()));
        assert!(!pruned.contains(&"read_file".to_string()));

        // Filtering strips pruned schemas unless explicitly requested
        let mut schemas = vec![
            dummy_schema("read_file"),
            dummy_schema("vault_bundle_show"),
            dummy_schema("browser_mock_route"),
        ];
        let mut explicit = HashSet::new();
        explicit.insert("browser_mock_route".to_string());
        let (removed, saved) = pruner.filter_schemas(&mut schemas, &explicit);
        assert_eq!(removed, vec!["vault_bundle_show".to_string()]);
        assert!(saved >= 40);
        assert_eq!(schemas.len(), 2);

        // One-command restore brings pruned tool back
        let restored = pruner.restore(Some("vault_bundle_show"));
        assert_eq!(restored, 1);
        assert!(!pruner
            .pruned_tools()
            .contains(&"vault_bundle_show".to_string()));
    }
}
