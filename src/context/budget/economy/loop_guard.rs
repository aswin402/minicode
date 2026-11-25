//! Semantic Edit→Test Failure-Loop Guard.
//!
//! Catches unproductive edit→test cycles where the agent mutates files with *different* patch
//! arguments on each attempt (bypassing exact-argument hash stuck detection) yet keeps hitting
//! the exact same normalized compiler/test failure signature.
//!
//! Inspired by `microsoft/TokenEconomizer`'s Loop Guard (Apache-2.0), implemented deterministically
//! in pure Rust at 0 extra LLM judge calls.

use crate::constants::{FNV_OFFSET_BASIS, FNV_PRIME};
use std::collections::BTreeSet;

/// Number of edit→test cycles hitting the same error signature before emitting a constructive redirect.
pub const LOOP_REDIRECT_CYCLES: usize = 2;

/// Number of edit→test cycles hitting the same error signature before recommending escalation.
pub const LOOP_ESCALATE_CYCLES: usize = 4;

/// Output of the semantic edit→test loop guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopIntervention {
    None,
    /// Constructive single redirect injected into the failing tool output to break the cycle.
    Redirect {
        cycles: usize,
        error_excerpt: String,
        message: String,
    },
    /// Strong escalation notice when repeated redirects failed to clear the same root error.
    Escalate {
        cycles: usize,
        error_excerpt: String,
        message: String,
    },
}

/// Tracks edit→verify/test error cycles within a single turn.
#[derive(Debug, Clone, Default)]
pub struct EditTestLoopGuard {
    edited_since_last_test: bool,
    touched_files_since_test: BTreeSet<String>,
    last_error_sig: Option<u64>,
    consecutive_same_error_cycles: usize,
    interventions_emitted: usize,
}

impl EditTestLoopGuard {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.edited_since_last_test = false;
        self.touched_files_since_test.clear();
        self.last_error_sig = None;
        self.consecutive_same_error_cycles = 0;
        self.interventions_emitted = 0;
    }

    #[must_use]
    pub fn interventions_emitted(&self) -> usize {
        self.interventions_emitted
    }

    /// Observes a completed tool call and returns an optional constructive redirect when an
    /// edit→test failure cycle repeats on the same normalized error signature.
    pub fn observe(
        &mut self,
        tool_name: &str,
        args: &serde_json::Value,
        success: bool,
        output: &str,
    ) -> LoopIntervention {
        // Record file mutations between test/build runs
        if crate::tools::meta::is_file_mutation(tool_name) && success {
            self.edited_since_last_test = true;
            if let Some(path) = args
                .get("path")
                .or_else(|| args.get("file_path"))
                .and_then(serde_json::Value::as_str)
            {
                self.touched_files_since_test.insert(path.to_string());
            }
            return LoopIntervention::None;
        }

        // Only evaluate test/build execution commands and verification barrier calls
        let is_check_tool = tool_name == "power_verify"
            || (tool_name == "exec_cmd"
                && args
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(looks_like_build_or_test_command));

        if !is_check_tool {
            return LoopIntervention::None;
        }

        // A passing build/test clears the failure streak cleanly
        if success {
            self.edited_since_last_test = false;
            self.touched_files_since_test.clear();
            self.last_error_sig = None;
            self.consecutive_same_error_cycles = 0;
            return LoopIntervention::None;
        }

        let Some((sig, excerpt)) = fingerprint_error_output(output) else {
            return LoopIntervention::None;
        };

        if self.last_error_sig == Some(sig) && self.edited_since_last_test {
            self.consecutive_same_error_cycles += 1;
        } else if self.last_error_sig != Some(sig) {
            self.last_error_sig = Some(sig);
            self.consecutive_same_error_cycles = 1;
        }

        let files_list = if self.touched_files_since_test.is_empty() {
            "the modified files".to_string()
        } else {
            self.touched_files_since_test
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        };
        self.edited_since_last_test = false;
        self.touched_files_since_test.clear();

        if self.consecutive_same_error_cycles >= LOOP_ESCALATE_CYCLES {
            self.interventions_emitted += 1;
            let message = format!(
                "<economy_loop_guard level=\"escalate\" cycles=\"{cycles}\">\n\
                 Repeated edit→test failure loop ({cycles} cycles on {files_list}) hitting the SAME error:\n\
                 `{excerpt}`\n\
                 STOP incremental patching. Re-read the full upstream definition/callers (`read_file` / `locate_symbol`), reconsider the architecture, or use `ask_user` if blocked on an external constraint.\n\
                 </economy_loop_guard>",
                cycles = self.consecutive_same_error_cycles,
            );
            LoopIntervention::Escalate {
                cycles: self.consecutive_same_error_cycles,
                error_excerpt: excerpt,
                message,
            }
        } else if self.consecutive_same_error_cycles >= LOOP_REDIRECT_CYCLES {
            self.interventions_emitted += 1;
            let message = format!(
                "<economy_loop_guard level=\"redirect\" cycles=\"{cycles}\">\n\
                 Edit→test loop detected ({cycles} cycles on {files_list}) producing the same error signature:\n\
                 `{excerpt}`\n\
                 Pause before patching again: trace the root cause upstream with `read_file` or `grep_search` (verify types, imports, or test fixtures) rather than repeating local edits.\n\
                 </economy_loop_guard>",
                cycles = self.consecutive_same_error_cycles,
            );
            LoopIntervention::Redirect {
                cycles: self.consecutive_same_error_cycles,
                error_excerpt: excerpt,
                message,
            }
        } else {
            LoopIntervention::None
        }
    }
}

fn looks_like_build_or_test_command(cmd: &str) -> bool {
    let lower = cmd.to_ascii_lowercase();
    [
        "cargo test",
        "cargo check",
        "cargo clippy",
        "cargo build",
        "pytest",
        "vitest",
        "jest",
        "npm test",
        "npm run test",
        "npm run build",
        "bun test",
        "bun run test",
        "bun run build",
        "bun run lint",
        "tsc",
        "eslint",
        "go test",
        "go build",
        "node --test",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
}

/// Extracts a stable FNV-1a signature + concise root-error excerpt from compiler/test stderr/stdout,
/// normalizing volatile digits (line/column numbers, durations, hex addresses) so shifted line
/// numbers after edits still hash to the same root error.
fn fingerprint_error_output(output: &str) -> Option<(u64, String)> {
    let error_line = output
        .lines()
        .map(str::trim)
        .find(|line| {
            let l = line.to_ascii_lowercase();
            !line.is_empty()
                && (l.starts_with("error[")
                    || l.starts_with("error:")
                    || l.contains("assertionerror")
                    || l.contains("typeerror:")
                    || l.contains("referenceerror:")
                    || l.contains("syntaxerror:")
                    || l.contains("ts2")
                    || l.contains("failed:")
                    || l.contains("panicked at")
                    || l.contains("expected ") && l.contains("found "))
        })
        .or_else(|| {
            output
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && l.len() >= 10)
        })?;

    let mut normalized = String::with_capacity(error_line.len());
    let mut prev_was_digit = false;
    for ch in error_line.chars() {
        if ch.is_ascii_digit() {
            if !prev_was_digit {
                normalized.push('#');
                prev_was_digit = true;
            }
        } else {
            prev_was_digit = false;
            normalized.push(ch.to_ascii_lowercase());
        }
    }

    let mut hash = FNV_OFFSET_BASIS;
    for b in normalized.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    let excerpt: String = error_line.chars().take(180).collect();
    Some((hash, excerpt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_edit_test_loop_guard_catches_repeated_error_across_differing_edits() {
        let mut guard = EditTestLoopGuard::new();

        // Cycle 1: edit -> test fails on line 14
        assert_eq!(
            guard.observe("patch_file", &json!({"path": "src/auth.rs"}), true, "ok"),
            LoopIntervention::None
        );
        assert_eq!(
            guard.observe(
                "exec_cmd",
                &json!({"command": "cargo test"}),
                false,
                "error[E0308]: mismatched types at src/auth.rs:14:5 expected `bool`, found `Result<bool>`"
            ),
            LoopIntervention::None
        );

        // Cycle 2: different edit (line shifted to 19!) -> SAME root error -> Redirect!
        assert_eq!(
            guard.observe("patch_file", &json!({"path": "src/auth.rs"}), true, "ok"),
            LoopIntervention::None
        );
        let second = guard.observe(
            "exec_cmd",
            &json!({"command": "cargo test"}),
            false,
            "error[E0308]: mismatched types at src/auth.rs:19:9 expected `bool`, found `Result<bool>`",
        );
        assert!(matches!(
            second,
            LoopIntervention::Redirect { cycles: 2, .. }
        ));

        // Passing test resets streak cleanly
        assert_eq!(
            guard.observe("exec_cmd", &json!({"command": "cargo test"}), true, "ok"),
            LoopIntervention::None
        );
        assert_eq!(guard.consecutive_same_error_cycles, 0);
    }
}
