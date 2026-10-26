use crate::agent::types::{Message, Role};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

/// Thread-safe store for automatic subagent result relaying (`$RESULT[...]`)
/// and parent conversation turn slicing (`$TURN[...]`).
#[derive(Default)]
pub struct SubagentRelayStore {
    /// Named and automatic subagent outputs keyed by `(workspace_key, alias_or_id)`.
    results: RwLock<HashMap<(String, String), String>>,
    /// Snapshot of parent session messages (`Role::User` and `Role::Assistant`) keyed by `workspace_key`.
    parent_turns: RwLock<HashMap<String, Vec<(String, String)>>>,
    /// Pending file paths modified by subagents/swarm workers keyed by `workspace_key`,
    /// drained by the parent `AgentLoop` so subagent edits participate in `VerificationBarrier`
    /// and `WorkingMemory` task reconciliation.
    modified_files: RwLock<HashMap<String, Vec<String>>>,
}

static GLOBAL_RELAY_STORE: OnceLock<SubagentRelayStore> = OnceLock::new();

/// Returns a reference to the global `SubagentRelayStore`.
pub fn get_global_relay_store() -> &'static SubagentRelayStore {
    GLOBAL_RELAY_STORE.get_or_init(SubagentRelayStore::default)
}

impl SubagentRelayStore {
    fn workspace_key(workspace_root: &Path) -> String {
        crate::context::governance::dox::DoxEngine::canonical_workspace_root(workspace_root)
            .to_string_lossy()
            .to_string()
    }

    /// Automatically records a completed subagent's output under:
    /// - `<subagent_id>` (e.g. `scout-1`)
    /// - `latest:<role_slug>` (e.g. `latest:scout`)
    /// - `latest`
    /// - optional explicit `result_alias` (`as` parameter, e.g. `auth_plan`)
    pub fn record_subagent_result(
        &self,
        workspace_root: &Path,
        subagent_id: &str,
        role_slug: &str,
        result_alias: Option<&str>,
        output: &str,
    ) {
        let ws_key = Self::workspace_key(workspace_root);
        let clean_output = output.trim().to_string();
        if clean_output.is_empty() {
            return;
        }

        if let Ok(mut map) = self.results.write() {
            map.insert(
                (ws_key.clone(), subagent_id.trim().to_string()),
                clean_output.clone(),
            );
            map.insert(
                (
                    ws_key.clone(),
                    format!("latest:{}", role_slug.trim().to_ascii_lowercase()),
                ),
                clean_output.clone(),
            );
            map.insert((ws_key.clone(), "latest".to_string()), clean_output.clone());
            if let Some(alias) = result_alias.map(str::trim).filter(|a| !a.is_empty()) {
                map.insert((ws_key, alias.to_string()), clean_output.clone());
            }
        }

        // Also mirror named alias onto the shared scratchpad blackboard so `scratchpad_read` sees it
        if let Some(alias) = result_alias.map(str::trim).filter(|a| !a.is_empty()) {
            let sp = super::get_global_scratchpad();
            sp.write_entry(
                alias,
                &format!("Subagent `{}` ({}) captured result", subagent_id, role_slug),
                &clean_output,
                subagent_id,
            );
        }
    }

    /// Looks up a captured subagent result by name, ID, or `latest[:role]`, falling back to `SharedScratchpad`.
    pub fn get_result(&self, workspace_root: &Path, name: &str) -> Option<String> {
        let ws_key = Self::workspace_key(workspace_root);
        let key = name.trim();
        if let Ok(map) = self.results.read() {
            if let Some(val) = map.get(&(ws_key, key.to_string())) {
                return Some(val.clone());
            }
        }
        super::get_global_scratchpad()
            .read_entry(key)
            .map(|e| e.content)
    }

    /// Records file paths modified by a completed subagent or merged swarm worker for the given workspace.
    pub fn record_modified_files(&self, workspace_root: &Path, files: &[String]) {
        if files.is_empty() {
            return;
        }
        let ws_key = Self::workspace_key(workspace_root);
        if let Ok(mut map) = self.modified_files.write() {
            let entry = map.entry(ws_key).or_default();
            for file in files {
                let clean = file.trim();
                if !clean.is_empty() && !entry.iter().any(|existing| existing == clean) {
                    entry.push(clean.to_string());
                }
            }
        }
    }

    /// Drains all pending subagent-modified file paths for the given workspace so the parent
    /// `AgentLoop` can include them in `turn_files_modified`, `WorkingMemory`, and `VerificationBarrier`.
    pub fn drain_modified_files(&self, workspace_root: &Path) -> Vec<String> {
        let ws_key = Self::workspace_key(workspace_root);
        if let Ok(mut map) = self.modified_files.write() {
            return map.remove(&ws_key).unwrap_or_default();
        }
        Vec::new()
    }

    /// Synchronizes the parent Orchestrator's conversation messages (`User` and `Assistant`)
    /// so `$TURN[...]` references and automatic parent grounding always have fresh context.
    pub fn sync_parent_messages(&self, workspace_root: &Path, messages: &[Message]) {
        let ws_key = Self::workspace_key(workspace_root);
        let mut turns = Vec::new();

        for msg in messages {
            match msg.role {
                Role::User => {
                    let text = msg.content.trim();
                    if !text.is_empty() && !text.starts_with("[System:") {
                        turns.push(("USER".to_string(), text.to_string()));
                    }
                }
                Role::Assistant => {
                    let clean = crate::agent::prompt::strip_thought_blocks(&msg.content);
                    let trimmed = clean.trim();
                    if !trimmed.is_empty() {
                        turns.push(("ASSISTANT".to_string(), trimmed.to_string()));
                    }
                }
                _ => {}
            }
        }

        if let Ok(mut map) = self.parent_turns.write() {
            map.insert(ws_key, turns);
        }
    }

    /// Formats a compact automatic `<parent_session_context>` block from the parent session's
    /// initial user goal and most recent turn so subagents never operate without session context.
    pub fn auto_parent_context_block(&self, workspace_root: &Path) -> String {
        let ws_key = Self::workspace_key(workspace_root);
        let Ok(map) = self.parent_turns.read() else {
            return String::new();
        };
        let Some(turns) = map.get(&ws_key) else {
            return String::new();
        };
        if turns.is_empty() {
            return String::new();
        }

        let first_user = turns
            .iter()
            .find(|(role, _)| role == "USER")
            .map(|(_, text)| crate::utils::strings::truncate_ellipsis(text, 500));

        let last_turn = turns.last().map(|(role, text)| {
            (
                role.as_str(),
                crate::utils::strings::truncate_ellipsis(text, 500),
            )
        });

        let mut out = String::from("<parent_session_context>\n");
        if let Some(ref goal) = first_user {
            out.push_str(&format!("**Parent Session Goal**: {}\n", goal));
        }
        if let Some((role, ref latest)) = last_turn {
            if first_user.as_deref() != Some(latest.as_str()) {
                out.push_str(&format!("**Latest Parent {} Turn**: {}\n", role, latest));
            }
        }
        out.push_str("</parent_session_context>");
        out
    }

    /// Resolves all `$RESULT[name]` and `$TURN[...]` tokens in a subagent prompt.
    pub fn interpolate_prompt(&self, workspace_root: &Path, prompt: &str) -> String {
        let after_results = self.resolve_result_references(workspace_root, prompt);
        self.resolve_turn_references(workspace_root, &after_results)
    }

    /// Replaces `$RESULT[name]` occurrences with the captured output from prior subagents.
    pub fn resolve_result_references(&self, workspace_root: &Path, text: &str) -> String {
        if !text.contains("$RESULT[") {
            return text.to_string();
        }

        let mut out = String::with_capacity(text.len());
        let mut rest = text;

        while let Some(start) = rest.find("$RESULT[") {
            out.push_str(&rest[..start]);
            let after_prefix = &rest[start + "$RESULT[".len()..];
            if let Some(end) = after_prefix.find(']') {
                let name = after_prefix[..end].trim();
                if let Some(val) = self.get_result(workspace_root, name) {
                    out.push_str(&format!(
                        "<subagent_result name=\"{}\">\n{}\n</subagent_result>",
                        name, val
                    ));
                } else {
                    out.push_str(&format!("[Result '{}' not found]", name));
                }
                rest = &after_prefix[end + 1..];
            } else {
                out.push_str(&rest[start..]);
                return out;
            }
        }
        out.push_str(rest);
        out
    }

    /// Replaces `$TURN[n]`, `$TURN[:k]`, `$TURN[:a:b]`, and `$TURN[*]` with formatted parent session turns.
    pub fn resolve_turn_references(&self, workspace_root: &Path, text: &str) -> String {
        if !text.contains("$TURN[") {
            return text.to_string();
        }

        let ws_key = Self::workspace_key(workspace_root);
        let turns = self
            .parent_turns
            .read()
            .ok()
            .and_then(|m| m.get(&ws_key).cloned())
            .unwrap_or_default();

        let mut out = String::with_capacity(text.len());
        let mut rest = text;

        while let Some(start) = rest.find("$TURN[") {
            out.push_str(&rest[..start]);
            let after_prefix = &rest[start + "$TURN[".len()..];
            if let Some(end) = after_prefix.find(']') {
                let spec = after_prefix[..end].trim();
                let formatted = Self::format_turn_slice(&turns, spec);
                out.push_str(&formatted);
                rest = &after_prefix[end + 1..];
            } else {
                out.push_str(&rest[start..]);
                return out;
            }
        }
        out.push_str(rest);
        out
    }

    fn format_turn_slice(turns: &[(String, String)], spec: &str) -> String {
        if turns.is_empty() {
            return "[TURN: no parent messages recorded yet]".to_string();
        }

        let selected: Vec<&(String, String)> = if spec == "*" {
            turns.iter().collect()
        } else if let Some(indices_str) = spec.strip_prefix(':') {
            // Specific 1-based indices from the end: e.g. ":3" or ":2:5:8"
            indices_str
                .split(':')
                .filter_map(|s| s.trim().parse::<usize>().ok())
                .filter(|&idx| idx >= 1 && idx <= turns.len())
                .filter_map(|idx| turns.get(turns.len() - idx))
                .collect()
        } else if let Ok(last_n) = spec.parse::<usize>() {
            let n = last_n.max(1);
            let skip = turns.len().saturating_sub(n);
            turns.iter().skip(skip).collect()
        } else {
            return format!("$TURN[{}]", spec);
        };

        let mut body = String::new();
        for (role, content) in selected {
            if !body.is_empty() {
                body.push_str("\n\n");
            }
            body.push_str(&format!(
                "--- {} ---\n{}",
                role,
                crate::utils::strings::truncate_ellipsis(content, 1_600)
            ));
        }
        crate::utils::strings::truncate_ellipsis(&body, 8_000)
    }
}

/// Checks whether a subagent response contains an explicit loop break signal:
/// `<subtask loop="break"/>`, `<subtask loop=break/>`, `<subtask2 loop="break"/>`, etc.
pub fn has_loop_break_signal(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("loop=\"break\"")
        || lower.contains("loop='break'")
        || lower.contains("loop=break")
}

/// Builds the `subtask2`-inspired conditional loop evaluation prompt injected into a MiniPub
/// worker to verify whether `<completion-condition>` is genuinely satisfied before returning.
pub fn build_loop_evaluation_prompt(condition: &str, iteration: usize, max: usize) -> String {
    format!(
        "<instructions minipub=\"loop-evaluation\">\n\
        Conditional Loop Evaluation — Iteration {iteration}/{max}.\n\n\
        **Evaluate the work completed so far and verify whether the current workspace state genuinely satisfies the required completion condition:**\n\n\
        <completion-condition>\n\
        {condition}\n\
        </completion-condition>\n\n\
        You may read files, check diffs, or run verification commands (`exec_cmd` / `power_verify`) if needed.\n\
        - If the condition IS 100% satisfied: respond with `<subtask loop=\"break\"/>` followed by your final summary.\n\
        - If the condition is NOT yet satisfied: do NOT emit `<subtask loop=\"break\"/>`; instead invoke your tools now to fix the remaining gaps.\n\
        </instructions>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_relay_and_turn_interpolation() {
        let tmp =
            std::env::temp_dir().join(format!("minicode_relay_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let store = SubagentRelayStore::default();
        store.record_subagent_result(
            &tmp,
            "scout-1",
            "scout",
            Some("auth_findings"),
            "Found JWT middleware in `src/auth.rs:10-45`.",
        );

        let msgs = vec![
            Message::user("We need to refactor JWT validation.".to_string()),
            Message::assistant(
                "<thought>Checking</thought>I will inspect `src/auth.rs`.".to_string(),
            ),
            Message::user("Make sure token expiry is checked.".to_string()),
        ];
        store.sync_parent_messages(&tmp, &msgs);

        let raw_prompt = "Implement fixes based on $RESULT[auth_findings] and $RESULT[latest:scout].\nContext:\n$TURN[2]";
        let interpolated = store.interpolate_prompt(&tmp, raw_prompt);

        assert!(interpolated.contains("Found JWT middleware in `src/auth.rs:10-45`."));
        assert!(interpolated.contains("--- ASSISTANT ---\nI will inspect `src/auth.rs`."));
        assert!(interpolated.contains("--- USER ---\nMake sure token expiry is checked."));
        assert!(!interpolated.contains("<thought>"));

        let specific = store.resolve_turn_references(&tmp, "$TURN[:3]");
        assert!(specific.contains("We need to refactor JWT validation."));
        assert!(!specific.contains("Make sure token expiry is checked."));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_loop_break_signal_detection() {
        assert!(has_loop_break_signal(
            "All tests pass! <subtask loop=\"break\"/>"
        ));
        assert!(has_loop_break_signal("Done <subtask2 loop=break/>"));
        assert!(!has_loop_break_signal(
            "Still fixing failing test in auth.rs"
        ));
    }

    #[test]
    fn test_record_and_drain_modified_files() {
        let tmp =
            std::env::temp_dir().join(format!("minicode_relay_mod_files_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let store = SubagentRelayStore::default();
        store.record_modified_files(&tmp, &["src/auth.rs".to_string(), "src/lib.rs".to_string()]);
        store.record_modified_files(
            &tmp,
            &["src/auth.rs".to_string(), "tests/auth_test.rs".to_string()],
        );

        let drained = store.drain_modified_files(&tmp);
        assert_eq!(
            drained,
            vec![
                "src/auth.rs".to_string(),
                "src/lib.rs".to_string(),
                "tests/auth_test.rs".to_string()
            ]
        );
        assert!(store.drain_modified_files(&tmp).is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
