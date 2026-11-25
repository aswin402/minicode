//! Semantic Edit→Test Failure-Loop Guard.
//!
//! Catches unproductive edit→verify cycles where the agent mutates files with *different* patch
//! arguments on each attempt (bypassing exact-argument hash stuck detection) yet keeps hitting
//! the same normalized error cluster.
//!
//! Uses language-agnostic structural error-block normalization + 64-bit SimHash (`hamming_distance <= 6`)
//! across ANY shell command or verification call after file edits — zero hardcoded runner lists and
//! zero extra LLM judge calls.

use crate::context::budget::headroom::AdaptiveSizer;
use std::collections::BTreeSet;

/// Number of edit→verify cycles hitting the same semantic error cluster before emitting a redirect.
pub const LOOP_REDIRECT_CYCLES: usize = 2;

/// Number of edit→verify cycles hitting the same semantic error cluster before recommending escalation.
pub const LOOP_ESCALATE_CYCLES: usize = 4;

/// Maximum 64-bit SimHash Hamming distance for two normalized error blocks to be treated as the
/// same underlying root error despite shifted paths, line numbers, or minor phrasing drift.
const ERROR_SIMHASH_MAX_HAMMING: u32 = 6;

/// Output of the semantic edit→verify loop guard.
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

/// Tracks edit→verify failure cycles within a single turn using 64-bit SimHash error clustering.
#[derive(Debug, Clone, Default)]
pub struct EditTestLoopGuard {
    edited_since_last_check: bool,
    touched_files_since_check: BTreeSet<String>,
    last_error_simhash: Option<u64>,
    consecutive_same_error_cycles: usize,
    interventions_emitted: usize,
}

impl EditTestLoopGuard {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.edited_since_last_check = false;
        self.touched_files_since_check.clear();
        self.last_error_simhash = None;
        self.consecutive_same_error_cycles = 0;
        self.interventions_emitted = 0;
    }

    #[must_use]
    pub fn interventions_emitted(&self) -> usize {
        self.interventions_emitted
    }

    /// Observes a completed tool call and returns an optional constructive redirect when an
    /// edit→verify failure cycle repeats on the same semantic error cluster (SimHash).
    pub fn observe(
        &mut self,
        tool_name: &str,
        args: &serde_json::Value,
        success: bool,
        output: &str,
    ) -> LoopIntervention {
        // Record workspace file mutations between verification runs
        if crate::tools::meta::is_file_mutation(tool_name) && success {
            self.edited_since_last_check = true;
            if let Some(path) = args
                .get("path")
                .or_else(|| args.get("file_path"))
                .and_then(serde_json::Value::as_str)
            {
                self.touched_files_since_check.insert(path.to_string());
            }
            return LoopIntervention::None;
        }

        // Evaluate `power_verify` or ANY shell command executed after a file edit (or while
        // tracking an active failure streak), regardless of language or test runner name.
        let is_shell_exec = matches!(tool_name, "exec_cmd" | "sandbox_exec");
        let is_verify_step = tool_name == "power_verify"
            || (is_shell_exec
                && (self.edited_since_last_check || self.last_error_simhash.is_some()));

        if !is_verify_step {
            return LoopIntervention::None;
        }

        let exec_cmd_str = if is_shell_exec {
            args.get("command")
                .or_else(|| args.get("cmd"))
                .and_then(serde_json::Value::as_str)
        } else {
            None
        };

        // Ignore trivial read-only inspection shell commands (`ls`, `cat`, `git status`, `pwd`, etc.)
        // so reading files or checking git status between edits doesn't clear or trigger failure streaks.
        if let Some(cmd) = exec_cmd_str {
            if is_passive_inspection_command(cmd) {
                return LoopIntervention::None;
            }
        }

        // Detect failures masked by shell pipes (`cmd 2>&1 | tail -60`) where downstream utility exits 0.
        let effective_success = success
            && !exec_cmd_str.is_some_and(|cmd| output_indicates_masked_pipe_failure(cmd, output));

        // A passing verification run clears the failure streak cleanly
        if effective_success {
            self.edited_since_last_check = false;
            self.touched_files_since_check.clear();
            self.last_error_simhash = None;
            self.consecutive_same_error_cycles = 0;
            return LoopIntervention::None;
        }

        let Some((sig, excerpt)) = fingerprint_error_block(output) else {
            return LoopIntervention::None;
        };

        let same_cluster = self.last_error_simhash.is_some_and(|prev| {
            AdaptiveSizer::hamming_distance(prev, sig) <= ERROR_SIMHASH_MAX_HAMMING
        });

        if same_cluster && self.edited_since_last_check {
            self.consecutive_same_error_cycles += 1;
        } else if !same_cluster {
            self.last_error_simhash = Some(sig);
            self.consecutive_same_error_cycles = 1;
        }

        let files_list = if self.touched_files_since_check.is_empty() {
            "the modified files".to_string()
        } else {
            self.touched_files_since_check
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        };
        self.edited_since_last_check = false;
        self.touched_files_since_check.clear();

        if self.consecutive_same_error_cycles >= LOOP_ESCALATE_CYCLES {
            self.interventions_emitted += 1;
            let message = format!(
                "<economy_loop_guard level=\"escalate\" cycles=\"{cycles}\">\n\
                 Repeated edit→verify failure loop ({cycles} cycles on {files_list}) hitting the SAME root error cluster:\n\
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
                 Edit→verify loop detected ({cycles} cycles on {files_list}) producing the same semantic error cluster:\n\
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

/// Identifies passive inspection commands that neither compile nor execute tests, so running `ls`
/// or `git diff` between an edit and a test does not disrupt the edit→verify cycle state.
/// Unwraps trailing formatting pipes (`| tail`, `| head`, `| tee`) first so piped test/build runs
/// like `python3 -m unittest 2>&1 | tail -30` are inspected by their actual upstream binary.
fn is_passive_inspection_command(cmd: &str) -> bool {
    let unwrapped = super::unwrap_trailing_formatting_pipes(cmd);
    let primary = crate::tools::compactor::extract_primary_command(unwrapped);
    let mut parts = primary.split_whitespace();
    let Some(bin) = parts.next() else {
        return true;
    };
    let bin_name = bin.rsplit('/').next().unwrap_or(bin);
    if matches!(
        bin_name,
        "ls" | "cat"
            | "head"
            | "tail"
            | "wc"
            | "pwd"
            | "echo"
            | "printf"
            | "which"
            | "find"
            | "rg"
            | "grep"
            | "stat"
            | "file"
            | "tree"
            | "du"
            | "df"
    ) {
        return true;
    }
    if bin_name == "git" {
        return matches!(
            parts.next(),
            Some("status" | "diff" | "log" | "show" | "branch" | "rev-parse")
        );
    }
    false
}

/// Computes a 64-bit SimHash over the top normalized diagnostic lines of `output`, collapsing
/// volatile digits, hex addresses, file line/col spans, and paths so shifted line numbers or
/// minor wording changes after edits still cluster within `ERROR_SIMHASH_MAX_HAMMING`.
fn fingerprint_error_block(output: &str) -> Option<(u64, String)> {
    let mut scored_lines: Vec<(u8, &str)> = Vec::new();
    for line in output.lines().map(str::trim) {
        if line.len() < 8 {
            continue;
        }
        let score = diagnostic_signal_score(line);
        if score > 0 {
            scored_lines.push((score, line));
        }
    }

    // Fall back to the highest-information non-empty lines if no explicit diagnostic line matched
    if scored_lines.is_empty() {
        for line in output
            .lines()
            .map(str::trim)
            .filter(|l| l.len() >= 10)
            .take(6)
        {
            scored_lines.push((1, line));
        }
    }
    if scored_lines.is_empty() {
        return None;
    }

    // Preserve top diagnostic excerpt (highest score first, stable order)
    let best_excerpt_line = scored_lines
        .iter()
        .max_by_key(|(s, _)| *s)
        .map(|(_, l)| *l)
        .unwrap_or(scored_lines[0].1);

    // Build normalized token skeleton across up to 6 diagnostic lines for 64-bit SimHash
    let mut skeleton = String::new();
    for (_, line) in scored_lines.iter().take(6) {
        normalize_diagnostic_tokens(line, &mut skeleton);
        skeleton.push('\n');
    }

    let hash = AdaptiveSizer::simhash_64(&skeleton);
    let excerpt: String = best_excerpt_line.chars().take(180).collect();
    Some((hash, excerpt))
}

/// Scores how strongly a line looks like a compiler/runtime/test error (0 = neutral, 1..3 = strong).
fn diagnostic_signal_score(line: &str) -> u8 {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("error[")
        || lower.starts_with("error:")
        || lower.starts_with("fatal:")
        || lower.contains("assertionerror")
        || lower.contains("panicked at")
        || lower.contains("typeerror:")
        || lower.contains("syntaxerror:")
        || lower.contains("referenceerror:")
        || lower.contains("undefined reference")
        || (lower.contains("expected ") && lower.contains("found "))
    {
        3
    } else if lower.contains(": error:")
        || lower.contains("error ts")
        || lower.starts_with("fail:")
        || lower.starts_with("failed:")
        || lower.contains("exception:")
        || lower.contains("cannot find ")
        || lower.contains("mismatched types")
    {
        2
    } else if lower.contains("failed") || lower.contains("error") {
        1
    } else {
        0
    }
}

/// Normalizes volatile numbers, hex literals, line:col markers, and quoted paths into stable
/// tokens so SimHash compares semantic error structure rather than shifted line numbers.
fn normalize_diagnostic_tokens(line: &str, out: &mut String) {
    let mut prev_was_digit = false;
    for ch in line.chars() {
        if ch.is_ascii_digit() {
            if !prev_was_digit {
                out.push('#');
                prev_was_digit = true;
            }
        } else {
            prev_was_digit = false;
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '[' | ']' | ' ' | '-') {
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(' ');
            }
        }
    }
}

/// Returns `true` when a piped verification command exited 0 via a downstream formatter (`| tail`,
/// `| head`) while stdout/stderr clearly reports test or compiler failure with no passing summary.
fn output_indicates_masked_pipe_failure(cmd: &str, output: &str) -> bool {
    if !cmd.contains('|') {
        return false;
    }
    let has_clear_pass = output.lines().any(|line| {
        let t = line.trim();
        t == "OK" || t.starts_with("test result: ok.")
    });
    if has_clear_pass {
        return false;
    }
    output.lines().map(str::trim).any(|line| {
        diagnostic_signal_score(line) >= 2
            || line.starts_with("FAILED (")
            || line.starts_with("Traceback (most recent call last):")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_edit_test_loop_guard_catches_repeated_error_across_differing_edits() {
        let mut guard = EditTestLoopGuard::new();

        // Cycle 1: edit -> custom runner (not in any hardcoded list!) fails on line 14
        assert_eq!(
            guard.observe("patch_file", &json!({"path": "src/auth.zig"}), true, "ok"),
            LoopIntervention::None
        );
        assert_eq!(
            guard.observe(
                "exec_cmd",
                &json!({"command": "zig build test"}),
                false,
                "error[E0308]: mismatched types at src/auth.zig:14:5 expected `bool`, found `Result<bool>`"
            ),
            LoopIntervention::None
        );

        // Passive inspection command between edit and verify does not disrupt state
        assert_eq!(
            guard.observe("exec_cmd", &json!({"command": "git diff"}), true, "diff"),
            LoopIntervention::None
        );

        // Cycle 2: different edit (line shifted to 19!) -> SAME SimHash error cluster -> Redirect!
        assert_eq!(
            guard.observe("patch_file", &json!({"path": "src/auth.zig"}), true, "ok"),
            LoopIntervention::None
        );
        let second = guard.observe(
            "exec_cmd",
            &json!({"command": "zig build test"}),
            false,
            "error[E0308]: mismatched types at src/auth.zig:19:9 expected `bool`, found `Result<bool>`",
        );
        assert!(matches!(
            second,
            LoopIntervention::Redirect { cycles: 2, .. }
        ));

        // Passing test resets streak cleanly
        assert_eq!(
            guard.observe(
                "exec_cmd",
                &json!({"command": "zig build test"}),
                true,
                "ok"
            ),
            LoopIntervention::None
        );
        assert_eq!(guard.consecutive_same_error_cycles, 0);
    }

    #[test]
    fn test_loop_guard_detects_piped_test_failure_despite_exit_zero() {
        let mut guard = EditTestLoopGuard::new();
        let piped_cmd = json!({"command": "python3 -m unittest discover -s tests 2>&1 | tail -30"});
        let fail_out = "FAIL: test_tax (test_billing.TestInvoice)\nAssertionError: 99 != 100\nFAILED (failures=1)";

        guard.observe("patch_file", &json!({"path": "src/billing.py"}), true, "ok");
        assert_eq!(
            guard.observe("exec_cmd", &piped_cmd, true, fail_out),
            LoopIntervention::None
        );
        guard.observe("patch_file", &json!({"path": "src/billing.py"}), true, "ok");
        let second = guard.observe("exec_cmd", &piped_cmd, true, fail_out);
        assert!(matches!(
            second,
            LoopIntervention::Redirect { cycles: 2, .. }
        ));
    }
}
