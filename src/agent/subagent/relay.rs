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
    /// initial user goal and most recent turn, using AST-preserving Markdown compression and
    /// lossless `CcrCache` backing when turns exceed inline budget.
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

        let compress_turn = |text: &str| -> String {
            let compressed = crate::agent::caveman::compress_markdown_ast_auto(
                text,
                crate::agent::caveman::CavemanLevel::Full,
            );
            if compressed.len() > 600 {
                let ccr_id = crate::context::budget::ccr_cache::CcrCache::store(text);
                format!(
                    "{} [Full turn: retrieve_observation(id=\"{}\")]",
                    crate::utils::strings::truncate_ellipsis(&compressed, 550),
                    ccr_id
                )
            } else {
                compressed
            }
        };

        let first_user_raw = turns
            .iter()
            .find(|(role, _)| role == "USER")
            .map(|(_, text)| text.as_str());
        let first_user = first_user_raw.map(compress_turn);

        let last_turn = turns
            .last()
            .map(|(role, text)| (role.as_str(), text.as_str(), compress_turn(text)));

        let mut out = String::from("<parent_session_context>\n");
        if let Some(ref goal) = first_user {
            out.push_str(&format!("**Parent Session Goal**: {}\n", goal));
        }
        if let Some((role, raw_latest, ref latest)) = last_turn {
            if first_user_raw != Some(raw_latest) {
                out.push_str(&format!("**Latest Parent {} Turn**: {}\n", role, latest));
            }
        }
        out.push_str("</parent_session_context>");
        out
    }

    /// Resolves all `$RESULT[name]` and `$TURN[...]` tokens in a subagent prompt in a single
    /// code-fence-aware $O(N)$ pass, ignoring tokens inside fenced code blocks (```` ``` ````)
    /// or inline code spans (`` `...` ``) and never re-scanning substituted payloads.
    pub fn interpolate_prompt(&self, workspace_root: &Path, prompt: &str) -> String {
        self.scan_and_interpolate(workspace_root, prompt, true, true)
    }

    /// Replaces `$RESULT[name]` occurrences with the captured output from prior subagents.
    /// For large relayed outputs (> 4,000 bytes), applies AST-preserving Markdown compression
    /// and registers a lossless `CcrCache` key so recipient subagents stay within token budget.
    pub fn resolve_result_references(&self, workspace_root: &Path, text: &str) -> String {
        self.scan_and_interpolate(workspace_root, text, true, false)
    }

    /// Replaces `$TURN[n]`, `$TURN[:k]`, `$TURN[:a:b]`, and `$TURN[*]` with formatted parent session turns.
    pub fn resolve_turn_references(&self, workspace_root: &Path, text: &str) -> String {
        self.scan_and_interpolate(workspace_root, text, false, true)
    }

    fn scan_and_interpolate(
        &self,
        workspace_root: &Path,
        text: &str,
        resolve_result: bool,
        resolve_turn: bool,
    ) -> String {
        let has_result = resolve_result && text.contains("$RESULT[");
        let has_turn = resolve_turn && text.contains("$TURN[");
        if !has_result && !has_turn {
            return text.to_string();
        }

        let turns = if has_turn {
            let ws_key = Self::workspace_key(workspace_root);
            self.parent_turns
                .read()
                .ok()
                .and_then(|m| m.get(&ws_key).cloned())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut out = String::with_capacity(len);
        let mut copy_from = 0usize;
        let mut idx = 0usize;
        let mut at_line_start = true;
        let mut in_fence = false;
        let mut in_inline_code = false;

        while idx < len {
            let b = bytes[idx];
            if b == b'\n' {
                at_line_start = true;
                in_inline_code = false;
                idx += 1;
                continue;
            }

            if at_line_start {
                if b == b' ' || b == b'\t' {
                    idx += 1;
                    continue;
                }
                at_line_start = false;
                let tail = &text[idx..];
                if tail.starts_with("```") || tail.starts_with("~~~") {
                    in_fence = !in_fence;
                    idx += 3;
                    continue;
                }
            }

            if !in_fence && b == b'`' {
                in_inline_code = !in_inline_code;
                idx += 1;
                continue;
            }

            if !in_fence && !in_inline_code && b == b'$' {
                let tail = &text[idx..];
                if has_result && tail.starts_with("$RESULT[") {
                    let after_prefix = idx + "$RESULT[".len();
                    if let Some(rel_end) = text[after_prefix..].find(']') {
                        let name_raw = &text[after_prefix..after_prefix + rel_end];
                        if !name_raw.contains('\n') {
                            if copy_from < idx {
                                out.push_str(&text[copy_from..idx]);
                            }
                            let name = name_raw.trim();
                            if let Some(val) = self.get_result(workspace_root, name) {
                                let payload = if val.len() > 4_000 {
                                    let ccr_id =
                                        crate::context::budget::ccr_cache::CcrCache::store(&val);
                                    let compressed =
                                        crate::agent::caveman::compress_markdown_ast_auto(
                                            &val,
                                            crate::agent::caveman::CavemanLevel::Full,
                                        );
                                    format!(
                                        "{}\n[Full subagent result cached: retrieve_observation(id=\"{}\")]",
                                        compressed, ccr_id
                                    )
                                } else {
                                    val
                                };
                                out.push_str(&format!(
                                    "<subagent_result name=\"{}\">\n{}\n</subagent_result>",
                                    name, payload
                                ));
                            } else {
                                out.push_str(&format!("[Result '{}' not found]", name));
                            }
                            idx = after_prefix + rel_end + 1;
                            copy_from = idx;
                            continue;
                        }
                    }
                } else if has_turn && tail.starts_with("$TURN[") {
                    let after_prefix = idx + "$TURN[".len();
                    if let Some(rel_end) = text[after_prefix..].find(']') {
                        let spec_raw = &text[after_prefix..after_prefix + rel_end];
                        if !spec_raw.contains('\n') {
                            if copy_from < idx {
                                out.push_str(&text[copy_from..idx]);
                            }
                            let spec = spec_raw.trim();
                            out.push_str(&Self::format_turn_slice(&turns, spec));
                            idx = after_prefix + rel_end + 1;
                            copy_from = idx;
                            continue;
                        }
                    }
                }
            }

            idx += text[idx..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
        }

        if copy_from < len {
            out.push_str(&text[copy_from..]);
        }
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

    #[test]
    fn test_interpolation_skips_fenced_and_inline_code_spans() {
        let tmp =
            std::env::temp_dir().join(format!("minicode_relay_fence_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let store = SubagentRelayStore::default();
        store.record_subagent_result(
            &tmp,
            "scout-1",
            "scout",
            Some("r1"),
            "Resolved output mentioning $TURN[1] inside payload",
        );

        let prompt =
            "Use `$RESULT[r1]` syntax or:\n```markdown\n$RESULT[r1]\n```\nActual: $RESULT[r1]";
        let out = store.interpolate_prompt(&tmp, prompt);
        // Inline and fenced occurrences remain untouched; only the unquoted occurrence is expanded
        assert!(out.contains("Use `$RESULT[r1]` syntax"));
        assert!(out.contains("```markdown\n$RESULT[r1]\n```"));
        assert!(out.contains("<subagent_result name=\"r1\">\nResolved output mentioning $TURN[1] inside payload\n</subagent_result>"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
