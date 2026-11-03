//! Persistent `.minicode/boulder.json` State Machine & Self-Referential `Ralph Loop` (`oh-my-opencode` parity).
//!
//! Tracks active plan progress, cross-turn / cross-session continuation counts, verification
//! gate status, and the `<promise>DONE</promise>` completion contract for `ralph:` / `ulw-loop:`
//! autonomous execution loops.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Declarative loop directive prefixes recognized at turn start.
pub struct LoopDirectiveRule {
    pub prefix: &'static str,
    pub mode: &'static str,
    pub enables_ralph_promise: bool,
}

pub const LOOP_DIRECTIVE_RULES: &[LoopDirectiveRule] = &[
    LoopDirectiveRule {
        prefix: "ralph-loop:",
        mode: "ralph",
        enables_ralph_promise: true,
    },
    LoopDirectiveRule {
        prefix: "ralph:",
        mode: "ralph",
        enables_ralph_promise: true,
    },
    LoopDirectiveRule {
        prefix: "ulw-loop:",
        mode: "ralph",
        enables_ralph_promise: true,
    },
    LoopDirectiveRule {
        prefix: "ultrawork:",
        mode: "ultrawork",
        enables_ralph_promise: false,
    },
    LoopDirectiveRule {
        prefix: "ulw:",
        mode: "ultrawork",
        enables_ralph_promise: false,
    },
];

/// Persistent Boulder & Ralph Loop state stored at `.minicode/boulder.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoulderState {
    pub active_plan_path: String,
    #[serde(default)]
    pub plan_title: Option<String>,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    #[serde(default)]
    pub active_task: Option<String>,
    pub continuation_count: usize,
    pub max_continuations: usize,
    #[serde(default)]
    pub ralph_loop_active: bool,
    #[serde(default)]
    pub promise_done_detected: bool,
    #[serde(default)]
    pub verified: bool,
    pub started_at: String,
    pub updated_at: String,
}

impl Default for BoulderState {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            active_plan_path: ".minicode/plan/task_plan.md".to_string(),
            plan_title: None,
            total_tasks: 0,
            completed_tasks: 0,
            active_task: None,
            continuation_count: 0,
            max_continuations: 3,
            ralph_loop_active: false,
            promise_done_detected: false,
            verified: false,
            started_at: now.clone(),
            updated_at: now,
        }
    }
}

impl BoulderState {
    /// Canonical path to `.minicode/boulder.json`.
    #[must_use]
    pub fn path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".minicode/boulder.json")
    }

    /// Loads `.minicode/boulder.json` from disk, or returns `Default` if absent.
    #[must_use]
    pub fn load(workspace_root: &Path) -> Self {
        let path = Self::path(workspace_root);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default()
    }

    /// Saves this `BoulderState` to `.minicode/boulder.json`.
    pub fn save(&self, workspace_root: &Path) -> std::io::Result<()> {
        let path = Self::path(workspace_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        std::fs::write(path, json)
    }

    /// Removes `.minicode/boulder.json` when a plan is archived.
    pub fn clear(workspace_root: &Path) {
        let _ = std::fs::remove_file(Self::path(workspace_root));
    }

    /// Declaratively matches a leading loop directive (`ralph:`, `ulw-loop:`, `ultrawork:`, `ulw:`)
    /// without regexes, returning the matched rule and stripped prompt body.
    #[must_use]
    pub fn match_directive(prompt: &str) -> Option<(&'static LoopDirectiveRule, &str)> {
        let trimmed = prompt.trim();
        let lower = trimmed.to_ascii_lowercase();
        for rule in LOOP_DIRECTIVE_RULES {
            if lower.starts_with(rule.prefix) {
                let rest = trimmed[rule.prefix.len()..].trim();
                if !rest.is_empty() {
                    return Some((rule, rest));
                }
            }
        }
        None
    }

    /// Checks whether assistant output contains the `<promise>DONE</promise>` completion token.
    #[must_use]
    pub fn contains_completion_promise(output: &str) -> bool {
        output
            .to_ascii_uppercase()
            .contains("<PROMISE>DONE</PROMISE>")
    }

    /// Synchronizes `.minicode/boulder.json` with `WorkingMemory` and `.minicode/plan/last_verify.json`.
    pub fn sync_from_workspace(
        workspace_root: &Path,
        ralph_loop_active: bool,
        max_continuations: usize,
    ) -> Self {
        let mut state = Self::load(workspace_root);
        let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
        if let Some(summary) = wm.read_active_plan_summary() {
            state.plan_title = Some(summary.phase_label);
            state.total_tasks = summary.total_tasks;
            state.completed_tasks = summary.completed_tasks;
            state.active_task = summary.active_task;
        } else {
            state.total_tasks = 0;
            state.completed_tasks = 0;
            state.active_task = None;
        }
        if ralph_loop_active {
            state.ralph_loop_active = true;
        }
        state.max_continuations = max_continuations.max(state.max_continuations);
        state.verified = workspace_root
            .join(".minicode/plan/last_verify.json")
            .is_file();
        state.updated_at = chrono::Utc::now().to_rfc3339();
        if state.total_tasks == 0 && !state.ralph_loop_active {
            Self::clear(workspace_root);
        } else {
            let _ = state.save(workspace_root);
        }
        state
    }

    /// Increments the persistent continuation counter and records whether `<promise>DONE</promise>` was emitted.
    pub fn record_continuation_step(workspace_root: &Path, assistant_output: &str) -> Self {
        let mut state = Self::load(workspace_root);
        state.continuation_count = state.continuation_count.saturating_add(1);
        if Self::contains_completion_promise(assistant_output) {
            state.promise_done_detected = true;
        }
        state.updated_at = chrono::Utc::now().to_rfc3339();
        let _ = state.save(workspace_root);
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boulder_state_persistence_and_ralph_directives() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path();

        let (rule, stripped) =
            BoulderState::match_directive("ralph: Finish all 4 phases and verify")
                .expect("ralph directive");
        assert_eq!(rule.mode, "ralph");
        assert!(rule.enables_ralph_promise);
        assert_eq!(stripped, "Finish all 4 phases and verify");

        let (ulw_rule, ulw_stripped) =
            BoulderState::match_directive("ulw-loop: Ship auth").expect("ulw-loop directive");
        assert!(ulw_rule.enables_ralph_promise);
        assert_eq!(ulw_stripped, "Ship auth");

        let synced = BoulderState::sync_from_workspace(ws, true, 6);
        assert!(synced.ralph_loop_active);
        assert_eq!(synced.max_continuations, 6);
        assert!(BoulderState::path(ws).is_file());

        let step1 = BoulderState::record_continuation_step(ws, "Still working on task 1...");
        assert_eq!(step1.continuation_count, 1);
        assert!(!step1.promise_done_detected);

        let step2 = BoulderState::record_continuation_step(
            ws,
            "All checks passed. <promise>DONE</promise>",
        );
        assert_eq!(step2.continuation_count, 2);
        assert!(step2.promise_done_detected);

        BoulderState::clear(ws);
        assert!(!BoulderState::path(ws).is_file());

        // Syncing when no active plan exists and ralph_loop_active is false should NOT recreate boulder.json
        let empty_sync = BoulderState::sync_from_workspace(ws, false, 3);
        assert_eq!(empty_sync.total_tasks, 0);
        assert!(!BoulderState::path(ws).is_file());
    }
}
