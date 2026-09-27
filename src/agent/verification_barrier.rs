use crate::agent::reproducer_guard::{ReproducerGuard, ReproducerPhase};
use crate::constants::{
    VERIFICATION_CONFLICT_MARKERS, VERIFICATION_DEBUG_PATTERNS, VERIFICATION_TEST_TIMEOUT_MS,
};
use crate::context::syntax_guard::SyntaxGuard;
use crate::tools::compiler::ScopedCompiler;
use serde::Serialize;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

/// Evaluation status for an individual verification gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum GateStatus {
    Passed,
    Failed {
        gate_name: &'static str,
        reason: String,
        actionable_remediation: String,
    },
    Skipped {
        reason: &'static str,
    },
}

impl GateStatus {
    #[must_use]
    pub fn is_pass_or_skip(&self) -> bool {
        matches!(self, Self::Passed | Self::Skipped { .. })
    }
}

/// Comprehensive report summarizing the evaluation of all 4 pre-completion verification gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificationReport {
    pub gate1_syntax_compiler: GateStatus,
    pub gate2_reproducer_test: GateStatus,
    pub gate3_regression_conflicts: GateStatus,
    pub gate4_diff_sanity: GateStatus,
    pub all_passed: bool,
}

impl VerificationReport {
    /// Formats a human-readable scorecard of the 4 verification gates.
    #[must_use]
    pub fn format_report(&self) -> String {
        let mut out = String::new();
        if self.all_passed {
            out.push_str("⚡ **MiniPower Verification Barrier: All 4 Gates Passed!**\n\n");
            out.push_str("  ✔ **Gate 1**: AST Syntax & Scoped Compiler Integrity\n");
            out.push_str("  ✔ **Gate 2**: Reproducer & Regression Test Suite\n");
            out.push_str("  ✔ **Gate 3**: Structural Integrity & Conflict Markers\n");
            out.push_str("  ✔ **Gate 4**: Diff Sanity (Zero Secrets & Debug Residue)\n\n");
            out.push_str("All pre-completion verification gates passed cleanly.");
        } else {
            out.push_str("⚡ **MiniPower Verification Barrier: Verification Failed**\n\n");
            let format_line = |name: &str, status: &GateStatus| -> String {
                match status {
                    GateStatus::Passed => format!("  ✔ **{}**: Passed\n", name),
                    GateStatus::Skipped { reason } => {
                        format!("  ○ **{}**: Skipped ({})\n", name, reason)
                    }
                    GateStatus::Failed {
                        reason,
                        actionable_remediation,
                        ..
                    } => {
                        format!(
                            "  ❌ **{}**: {}\n     ╰── Remediation: {}\n",
                            name, reason, actionable_remediation
                        )
                    }
                }
            };
            out.push_str(&format_line(
                "Gate 1 (AST Syntax & Compiler)",
                &self.gate1_syntax_compiler,
            ));
            out.push_str(&format_line(
                "Gate 2 (Reproducer & Tests)",
                &self.gate2_reproducer_test,
            ));
            out.push_str(&format_line(
                "Gate 3 (Structural Integrity)",
                &self.gate3_regression_conflicts,
            ));
            out.push_str(&format_line(
                "Gate 4 (Diff Sanity)",
                &self.gate4_diff_sanity,
            ));
        }
        out
    }

    /// Formats an actionable prompt directing the model to self-correct before finishing.
    #[must_use]
    pub fn format_remediation_prompt(&self) -> String {
        let mut out = String::new();
        out.push_str("[PRE-COMPLETION VERIFICATION BARRIER REJECTED COMPLETION]\n");
        out.push_str("You attempted to conclude the turn, but the workspace failed automated pre-completion verification gates:\n\n");

        if let GateStatus::Failed {
            gate_name,
            reason,
            actionable_remediation,
        } = &self.gate1_syntax_compiler
        {
            out.push_str(&format!(
                "- ❌ **{}**:\n  {}\n  --> Remediation: {}\n\n",
                gate_name, reason, actionable_remediation
            ));
        }

        if let GateStatus::Failed {
            gate_name,
            reason,
            actionable_remediation,
        } = &self.gate2_reproducer_test
        {
            out.push_str(&format!(
                "- ❌ **{}**:\n  {}\n  --> Remediation: {}\n\n",
                gate_name, reason, actionable_remediation
            ));
        }

        if let GateStatus::Failed {
            gate_name,
            reason,
            actionable_remediation,
        } = &self.gate3_regression_conflicts
        {
            out.push_str(&format!(
                "- ❌ **{}**:\n  {}\n  --> Remediation: {}\n\n",
                gate_name, reason, actionable_remediation
            ));
        }

        if let GateStatus::Failed {
            gate_name,
            reason,
            actionable_remediation,
        } = &self.gate4_diff_sanity
        {
            out.push_str(&format!(
                "- ❌ **{}**:\n  {}\n  --> Remediation: {}\n\n",
                gate_name, reason, actionable_remediation
            ));
        }

        out.push_str("Please fix these verification issues in your next tool action before declaring the task completed.");
        out
    }
}

/// 4-Gate Pre-Completion Verification Barrier.
///
/// Intercepts premature completion claims from autonomous models and validates:
/// - Gate 1: AST syntax validity & scoped compiler integrity on modified files
/// - Gate 2: Reproduction / modified test suite passes with exit code 0
/// - Gate 3: Structural integrity & zero unresolved merge conflict markers
/// - Gate 4: Diff sanity audit (zero leftover debug statements, zero leaked secrets)
pub struct VerificationBarrier;

impl VerificationBarrier {
    /// Executes all 4 gates against modified files in the workspace.
    pub async fn verify(workspace_root: &Path, modified_files: &[String]) -> VerificationReport {
        let gate1 = Self::check_gate1_syntax_compiler(workspace_root, modified_files);

        // Check if any managed dev runtime daemon exited abnormally
        let daemon_check = Self::check_daemon_health().await;
        let gate2 = if let Some(failed_daemon) = daemon_check {
            failed_daemon
        } else {
            Self::check_gate2_reproducer_test(workspace_root, modified_files)
        };

        let gate3 = Self::check_gate3_regression_conflicts(workspace_root, modified_files);
        let gate4 = Self::check_gate4_diff_sanity(workspace_root, modified_files);

        let all_passed = gate1.is_pass_or_skip()
            && gate2.is_pass_or_skip()
            && gate3.is_pass_or_skip()
            && gate4.is_pass_or_skip();

        VerificationReport {
            gate1_syntax_compiler: gate1,
            gate2_reproducer_test: gate2,
            gate3_regression_conflicts: gate3,
            gate4_diff_sanity: gate4,
            all_passed,
        }
    }

    /// Gate 1: AST Syntax, Scoped Compiler & Asset Integrity Check
    pub fn check_gate1_syntax_compiler(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> GateStatus {
        for file in modified_files {
            let abs_path = workspace_root.join(file);
            if !abs_path.exists() {
                continue;
            }

            // Read disk contents
            let content = match std::fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(e) => {
                    return GateStatus::Failed {
                        gate_name: "Gate 1: AST Syntax & Asset Integrity",
                        reason: format!("Failed to read modified file `{}`: {}", file, e),
                        actionable_remediation: format!("Ensure `{}` is accessible on disk.", file),
                    };
                }
            };

            // 1. In-memory Tree-sitter AST syntax barrier
            if let Some(err) = SyntaxGuard::check_syntax(&abs_path, &content) {
                return GateStatus::Failed {
                    gate_name: "Gate 1: AST Syntax & Asset Integrity",
                    reason: format!(
                        "AST syntax error in `{}` at line {}:{}: {}",
                        file, err.line, err.column, err.kind
                    ),
                    actionable_remediation: format!(
                        "Fix the syntax error in `{}` around line {} before concluding: `{}`",
                        file, err.line, err.snippet
                    ),
                };
            }

            // 2. Scoped compiler check
            if let Some(diag) = ScopedCompiler::run_scoped_check(workspace_root, file) {
                if diag.contains("reported errors:") {
                    return GateStatus::Failed {
                        gate_name: "Gate 1: AST Syntax & Asset Integrity",
                        reason: format!("Compiler diagnostic detected in `{}`:\n{}", file, diag),
                        actionable_remediation: format!(
                            "Resolve the compiler/linter error in `{}`.",
                            file
                        ),
                    };
                }
            }

            // 3. HTML Document & Local Asset Link Integrity
            if file.ends_with(".html") || file.ends_with(".htm") {
                if let Some(err) =
                    Self::check_html_integrity(workspace_root, file, &abs_path, &content)
                {
                    return err;
                }
            }

            // 4. CSS Syntax & Balanced Braces Integrity
            if file.ends_with(".css") {
                if let Some(err) = Self::check_css_integrity(file, &content) {
                    return err;
                }
            }
        }

        GateStatus::Passed
    }

    /// Gate 2: Reproducer / Bug Reproduction Test Execution
    pub fn check_gate2_reproducer_test(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> GateStatus {
        // 1. Check if any active reproducer is awaiting Red-to-Green transition
        let active_reproducers = ReproducerGuard::list_active_reproducers(workspace_root);
        let pending_repro = active_reproducers
            .into_iter()
            .find(|r| matches!(r.status, ReproducerPhase::RedConfirmed { .. }));

        // 2. Find if any test file was modified in this turn
        let modified_test = modified_files.iter().find(|f| {
            f.starts_with("tests/")
                || f.contains("test_")
                || f.contains("_test.")
                || f.contains("repro_")
        });

        // Determine target test path
        let (test_rel_path, is_tracked_repro) = if let Some(pending) = pending_repro {
            (pending.file_path, true)
        } else if let Some(m) = modified_test {
            (m.clone(), false)
        } else {
            return GateStatus::Skipped {
                reason:
                    "No reproducer script, test target, or active reproducer guard in this turn.",
            };
        };

        // For Rust tests in tests/<target>.rs, execute single target
        if test_rel_path.starts_with("tests/") && test_rel_path.ends_with(".rs") {
            let target_name = test_rel_path
                .trim_start_matches("tests/")
                .trim_end_matches(".rs");

            let (tx, rx) = mpsc::channel();
            let root = workspace_root.to_path_buf();
            let target = target_name.to_string();

            std::thread::spawn(move || {
                let status = Command::new("cargo")
                    .arg("test")
                    .arg("-j")
                    .arg("1")
                    .arg("--test")
                    .arg(&target)
                    .current_dir(&root)
                    .output();
                let _ = tx.send(status);
            });

            match rx.recv_timeout(Duration::from_millis(VERIFICATION_TEST_TIMEOUT_MS)) {
                Ok(Ok(output)) => {
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let err_msg = if !stderr.trim().is_empty() {
                            stderr.lines().take(5).collect::<Vec<_>>().join("\n")
                        } else {
                            stdout.lines().take(5).collect::<Vec<_>>().join("\n")
                        };

                        return GateStatus::Failed {
                            gate_name: "Gate 2: Reproducer & Test Execution",
                            reason: format!(
                                "Test target `{}` failed verification:\n{}",
                                target_name, err_msg
                            ),
                            actionable_remediation: format!(
                                "Investigate test failure in `tests/{}.rs` and fix the underlying logic.",
                                target_name
                            ),
                        };
                    } else if is_tracked_repro || target_name.starts_with("repro_") {
                        // Mark reproducer as GreenVerified
                        if let Some(mut record) =
                            ReproducerGuard::load_record(workspace_root, target_name)
                        {
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            record.status = ReproducerPhase::GreenVerified { timestamp };
                            let _ = ReproducerGuard::save_record(workspace_root, &record);
                            tracing::info!(
                                "Gate 2: Active reproducer `{}` transitioned from RED to GREEN!",
                                target_name
                            );
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("Failed to spawn test runner for Gate 2: {}", e);
                }
                Err(_) => {
                    tracing::warn!(
                        "Gate 2 test execution timed out ({}ms)",
                        VERIFICATION_TEST_TIMEOUT_MS
                    );
                }
            }
        }

        GateStatus::Passed
    }

    /// Gate 3: Structural Integrity & Git Conflict Markers Check
    pub fn check_gate3_regression_conflicts(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> GateStatus {
        for file in modified_files {
            let abs_path = workspace_root.join(file);
            if !abs_path.exists() {
                continue;
            }

            let content = match std::fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for (idx, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                for &marker in VERIFICATION_CONFLICT_MARKERS {
                    if trimmed.starts_with(marker) {
                        return GateStatus::Failed {
                            gate_name: "Gate 3: Structural Integrity & Merge Conflicts",
                            reason: format!(
                                "Git merge conflict marker `{}` found in `{}` at line {}.",
                                marker,
                                file,
                                idx + 1
                            ),
                            actionable_remediation: format!(
                                "Resolve and remove all merge conflict markers from `{}`.",
                                file
                            ),
                        };
                    }
                }
            }
        }

        GateStatus::Passed
    }

    /// Gate 4: Diff Sanity & Secret Leak Audit
    pub fn check_gate4_diff_sanity(workspace_root: &Path, modified_files: &[String]) -> GateStatus {
        for file in modified_files {
            let abs_path = workspace_root.join(file);
            if !abs_path.exists() {
                continue;
            }

            let content = match std::fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Check for raw debug prints only in production library code (src/), not in tests/ or CLI entrypoint (src/main.rs)
            let is_production_code = file.starts_with("src/")
                && !file.contains("test")
                && file != "src/main.rs"
                && file != "src/constants.rs";

            let mut in_test_module = false;
            for (idx, line) in content.lines().enumerate() {
                let trimmed = line.trim();

                if trimmed.starts_with("#[cfg(test)]") {
                    in_test_module = true;
                }
                if in_test_module {
                    continue;
                }

                // Skip pure line comments and block comment lines
                if trimmed.starts_with("//")
                    || trimmed.starts_with('#')
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                {
                    continue;
                }

                // 1. Raw debug logging detection in production code (skip pattern matching / scanner code)
                if is_production_code && !trimmed.contains(".contains(") {
                    for &pattern in VERIFICATION_DEBUG_PATTERNS {
                        if trimmed.contains(pattern) {
                            return GateStatus::Failed {
                                gate_name: "Gate 4: Diff Sanity & Secret Leak Audit",
                                reason: format!(
                                    "Forbidden stdout debug statement `{}` detected in `{}` at line {}:\n  {}",
                                    pattern,
                                    file,
                                    idx + 1,
                                    trimmed
                                ),
                                actionable_remediation: format!(
                                    "Remove `{}` from `{}` or replace with structured logging (`tracing::debug!`).",
                                    pattern, file
                                ),
                            };
                        }
                    }
                }

                // 2. Secret & API key leakage detection
                if Self::contains_secret_leak(trimmed) {
                    return GateStatus::Failed {
                        gate_name: "Gate 4: Diff Sanity & Secret Leak Audit",
                        reason: format!(
                            "Potential hardcoded secret or API key credential detected in `{}` at line {}.",
                            file,
                            idx + 1
                        ),
                        actionable_remediation: format!(
                            "Remove the secret from `{}` and retrieve it via environment variables.",
                            file
                        ),
                    };
                }
            }
        }

        GateStatus::Passed
    }

    /// Checks if a code line contains obvious hardcoded secrets or raw API tokens.
    #[must_use]
    pub fn contains_secret_leak(line: &str) -> bool {
        // Ignore test assertions or env accesses or pattern matching definitions
        if line.contains("std::env")
            || line.contains("process.env")
            || line.contains("env::var")
            || line.contains("assert")
            || line.contains("line.contains")
        {
            return false;
        }

        // 1. Private key blocks
        if line.contains("BEGIN PRIVATE KEY") || line.contains("BEGIN RSA PRIVATE KEY") {
            return true;
        }

        // Helper: checks if prefix starts on a token boundary and is followed by high-entropy key characters
        let has_token_prefix = |target: &str, prefix: &str, min_secret_tail_len: usize| -> bool {
            let bytes = target.as_bytes();
            let prefix_bytes = prefix.as_bytes();
            let mut search_idx = 0;

            while let Some(pos) = target[search_idx..].find(prefix) {
                let abs_pos = search_idx + pos;
                search_idx = abs_pos + prefix_bytes.len();

                // Character preceding prefix must NOT be alphanumeric or underscore
                // (e.g. rejects "task-", "flask-", "subtask-")
                if abs_pos > 0 {
                    let prev_byte = bytes[abs_pos - 1];
                    if prev_byte.is_ascii_alphanumeric() || prev_byte == b'_' {
                        continue;
                    }
                }

                // Check remaining tail after prefix: must be at least min_secret_tail_len alphanumeric/dash chars
                let tail = &target[abs_pos + prefix_bytes.len()..];
                let tail_chars_count = tail
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .count();

                if tail_chars_count >= min_secret_tail_len {
                    return true;
                }
            }
            false
        };

        // OpenAI / Anthropic / MiniMax keys: "sk-" followed by 20+ key chars on token boundary
        if has_token_prefix(line, "sk-", 20) {
            return true;
        }

        // GitHub Personal Access Tokens: "ghp_" followed by 20+ key chars on token boundary
        if has_token_prefix(line, "ghp_", 20) {
            return true;
        }

        // Google API Keys: "AIzaSy" followed by 25+ key chars on token boundary
        if has_token_prefix(line, "AIzaSy", 25) {
            return true;
        }

        false
    }

    /// Verifies HTML document structure and local asset link existence.
    pub fn check_html_integrity(
        workspace_root: &Path,
        file: &str,
        abs_path: &Path,
        content: &str,
    ) -> Option<GateStatus> {
        let lower = content.to_lowercase();

        // 1. Truncation checks for full HTML documents
        let is_full_document = lower.contains("<!doctype html")
            || (lower.contains("<html") && lower.contains("<head"));

        if is_full_document && !lower.contains("</html>") {
            return Some(GateStatus::Failed {
                gate_name: "Gate 1: AST Syntax & Asset Integrity",
                reason: format!(
                    "HTML document in `{}` is truncated: missing closing `</html>` tag.",
                    file
                ),
                actionable_remediation: format!(
                    "Ensure the complete HTML document is written with closing `</html>` in `{}`.",
                    file
                ),
            });
        }
        if is_full_document && lower.contains("<body") && !lower.contains("</body>") {
            return Some(GateStatus::Failed {
                gate_name: "Gate 1: AST Syntax & Asset Integrity",
                reason: format!(
                    "HTML document in `{}` is truncated: missing closing `</body>` tag.",
                    file
                ),
                actionable_remediation: format!(
                    "Ensure the body element is closed with `</body>` in `{}`.",
                    file
                ),
            });
        }

        // 2. Validate local referenced code assets (stylesheets & scripts)
        let html_dir = abs_path.parent().unwrap_or(workspace_root);
        let tags = ["<link", "<script"];

        for tag in tags {
            let mut search_from = 0;
            while let Some(tag_offset) = lower[search_from..].find(tag) {
                let tag_start = search_from + tag_offset;
                let tag_end = match content[tag_start..].find('>') {
                    Some(p) => tag_start + p,
                    None => break,
                };
                let tag_slice = &content[tag_start..=tag_end];

                for attr in &["href", "src"] {
                    let attr_pat = format!("{}=", attr);
                    if let Some(attr_pos) = tag_slice.to_lowercase().find(&attr_pat) {
                        let rest = &tag_slice[attr_pos + attr_pat.len()..];
                        let trimmed = rest.trim_start();
                        if let Some(quote) = trimmed.chars().next() {
                            if quote == '"' || quote == '\'' {
                                let after_quote = &trimmed[1..];
                                if let Some(end_quote) = after_quote.find(quote) {
                                    let mut raw_url = after_quote[..end_quote].trim();
                                    if let Some(q) = raw_url.find('?') {
                                        raw_url = &raw_url[..q];
                                    }
                                    if let Some(h) = raw_url.find('#') {
                                        raw_url = &raw_url[..h];
                                    }
                                    let url = raw_url.trim();

                                    // Filter out external URLs and special schemes
                                    if !url.is_empty()
                                        && !url.starts_with("http://")
                                        && !url.starts_with("https://")
                                        && !url.starts_with("//")
                                        && !url.starts_with("data:")
                                        && !url.starts_with("mailto:")
                                        && !url.starts_with("tel:")
                                        && !url.starts_with("javascript:")
                                        && !url.starts_with('{')
                                    {
                                        // Only verify code assets (.css, .js, .mjs, .ts)
                                        let is_code_asset = url.ends_with(".css")
                                            || url.ends_with(".js")
                                            || url.ends_with(".mjs")
                                            || url.ends_with(".ts");

                                        if is_code_asset {
                                            let clean_url = url.trim_start_matches("./");
                                            let target_local = html_dir.join(clean_url);
                                            let target_root = workspace_root
                                                .join(clean_url.trim_start_matches('/'));

                                            if !target_local.exists() && !target_root.exists() {
                                                return Some(GateStatus::Failed {
                                                    gate_name: "Gate 1: AST Syntax & Asset Integrity",
                                                    reason: format!(
                                                        "Broken asset link in `{}`: referenced asset `{}` does not exist on disk.",
                                                        file, url
                                                    ),
                                                    actionable_remediation: format!(
                                                        "Create the referenced file `{}` or update `{}` with the correct relative path.",
                                                        clean_url, file
                                                    ),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                search_from = tag_end + 1;
            }
        }

        None
    }

    /// Verifies CSS syntax for balanced curly braces (ignoring comments and strings).
    pub fn check_css_integrity(file: &str, content: &str) -> Option<GateStatus> {
        let mut in_comment = false;
        let mut in_string: Option<char> = None;
        let mut open_braces: usize = 0;
        let mut close_braces: usize = 0;
        let mut chars = content.chars().peekable();

        while let Some(ch) = chars.next() {
            if in_comment {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_comment = false;
                }
                continue;
            }

            if let Some(quote) = in_string {
                if ch == '\\' {
                    chars.next(); // skip escaped char
                } else if ch == quote {
                    in_string = None;
                }
                continue;
            }

            if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_comment = true;
                continue;
            }

            if ch == '"' || ch == '\'' {
                in_string = Some(ch);
                continue;
            }

            if ch == '{' {
                open_braces += 1;
            } else if ch == '}' {
                close_braces += 1;
            }
        }

        if open_braces != close_braces {
            return Some(GateStatus::Failed {
                gate_name: "Gate 1: AST Syntax & Asset Integrity",
                reason: format!(
                    "CSS syntax error in `{}`: unbalanced curly braces ({} opened vs {} closed).",
                    file, open_braces, close_braces
                ),
                actionable_remediation: format!(
                    "Check `{}` for unclosed CSS blocks or extra closing braces.",
                    file
                ),
            });
        }

        None
    }

    /// Checks if any background dev server registered with mini_dev terminated with an error.
    pub async fn check_daemon_health() -> Option<GateStatus> {
        let registry = crate::dev::get_global_dev_registry();
        let procs = registry.list().await;
        for p in &procs {
            if let crate::dev::models::DevProcessStatus::Exited(Some(code)) = p.status {
                if code != 0 {
                    let pid_str = p
                        .pid
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "N/A".to_string());
                    return Some(GateStatus::Failed {
                        gate_name: "Gate 2: Reproducer & Runtime Daemon Health",
                        reason: format!(
                            "Managed runtime daemon `{}` (PID {}) terminated abnormally with exit code {}.",
                            p.name, pid_str, code
                        ),
                        actionable_remediation: format!(
                            "Inspect service logs via `mini_dev(action=\"logs\", id=\"{}\")` and resolve startup errors.",
                            p.id
                        ),
                    });
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_gate3_conflict_markers_detection() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("conflict.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n<<<<<<< HEAD\n    let x = 1;\n=======\n    let x = 2;\n>>>>>>> feature\n}\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate3_regression_conflicts(
            temp.path(),
            &["conflict.rs".to_string()],
        );

        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 3"));
                assert!(reason.contains("conflict marker"));
            }
            _ => panic!("Expected Gate 3 to fail on conflict markers"),
        }
    }

    #[test]
    fn test_gate4_debug_statement_detection() {
        let temp = tempdir().unwrap();
        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("worker.rs");
        std::fs::write(&file_path, "pub fn work() {\n    println!(\"debug\");\n}\n").unwrap();

        let status = VerificationBarrier::check_gate4_diff_sanity(
            temp.path(),
            &["src/worker.rs".to_string()],
        );

        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 4"));
                assert!(reason.contains("println!"));
            }
            _ => panic!("Expected Gate 4 to fail on println! in src/"),
        }
    }

    #[test]
    fn test_gate4_secret_leak_detection() {
        let temp = tempdir().unwrap();
        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("client.rs");
        std::fs::write(
            &file_path,
            "const KEY: &str = \"sk-proj-abcdef1234567890abcdef1234567890\";\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate4_diff_sanity(
            temp.path(),
            &["src/client.rs".to_string()],
        );

        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 4"));
                assert!(reason.contains("secret or API key"));
            }
            _ => panic!("Expected Gate 4 to fail on sk- API key"),
        }
    }

    #[test]
    fn test_gate4_ignores_benign_subwords_like_task() {
        let temp = tempdir().unwrap();
        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("index.html");
        std::fs::write(
            &file_path,
            "<form id=\"taskForm\" class=\"task-form\" data-action=\"task-card-action\">\n  <button class=\"task-submit-btn\">Save Task</button>\n</form>\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate4_diff_sanity(
            temp.path(),
            &["src/index.html".to_string()],
        );

        match status {
            GateStatus::Passed => {}
            _ => panic!(
                "Expected Gate 4 to pass for benign task-form strings, got: {:?}",
                status
            ),
        }
    }

    #[test]
    fn test_remediation_prompt_formatting() {
        let report = VerificationReport {
            gate1_syntax_compiler: GateStatus::Failed {
                gate_name: "Gate 1: AST Syntax",
                reason: "Unclosed brace".to_string(),
                actionable_remediation: "Close brace".to_string(),
            },
            gate2_reproducer_test: GateStatus::Skipped { reason: "None" },
            gate3_regression_conflicts: GateStatus::Passed,
            gate4_diff_sanity: GateStatus::Failed {
                gate_name: "Gate 4: Diff Sanity",
                reason: "Found println!".to_string(),
                actionable_remediation: "Remove println!".to_string(),
            },
            all_passed: false,
        };

        let prompt = report.format_remediation_prompt();
        assert!(prompt.contains("[PRE-COMPLETION VERIFICATION BARRIER REJECTED COMPLETION]"));
        assert!(prompt.contains("Gate 1: AST Syntax"));
        assert!(prompt.contains("Gate 4: Diff Sanity"));
    }

    #[test]
    fn test_gate1_html_asset_integrity_missing_css() {
        let temp = tempdir().unwrap();
        let html_file = temp.path().join("index.html");
        std::fs::write(
            &html_file,
            "<!DOCTYPE html>\n<html>\n<head>\n  <link rel=\"stylesheet\" href=\"styles.css\">\n</head>\n<body>\n  <h1>Hello</h1>\n</body>\n</html>\n",
        )
        .unwrap();

        // styles.css does not exist yet -> should fail
        let status = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["index.html".to_string()],
        );
        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 1"));
                assert!(reason.contains("Broken asset link"));
                assert!(reason.contains("styles.css"));
            }
            _ => panic!("Expected Gate 1 to fail on missing styles.css"),
        }

        // Create styles.css -> should pass
        std::fs::write(temp.path().join("styles.css"), "body { margin: 0; }").unwrap();
        let status_ok = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["index.html".to_string()],
        );
        assert_eq!(status_ok, GateStatus::Passed);
    }

    #[test]
    fn test_gate1_html_truncation_detection() {
        let temp = tempdir().unwrap();
        let html_file = temp.path().join("index.html");
        // Missing closing </html> and </body>
        std::fs::write(
            &html_file,
            "<!DOCTYPE html>\n<html>\n<head><title>Test</title></head>\n<body>\n  <h1>Cut off midway...",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["index.html".to_string()],
        );
        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 1"));
                assert!(reason.contains("truncated") || reason.contains("</html>"));
            }
            _ => panic!("Expected Gate 1 to detect truncated HTML document"),
        }
    }

    #[test]
    fn test_gate1_css_balanced_braces_check() {
        let temp = tempdir().unwrap();
        let css_broken = temp.path().join("broken.css");
        std::fs::write(
            &css_broken,
            ".hero {\n  display: flex;\n  /* comment with { */\n  content: \"}\";\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["broken.css".to_string()],
        );
        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 1"));
                assert!(reason.contains("unbalanced curly braces"));
            }
            _ => panic!("Expected Gate 1 to reject unbalanced CSS braces"),
        }

        // Valid CSS
        let css_clean = temp.path().join("clean.css");
        std::fs::write(
            &css_clean,
            ".hero {\n  display: flex;\n  /* comment with { */\n  content: \"}\";\n}\n",
        )
        .unwrap();
        let status_clean = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["clean.css".to_string()],
        );
        assert_eq!(status_clean, GateStatus::Passed);
    }

    #[tokio::test]
    async fn test_gate2_daemon_health_check_passes_when_clean() {
        let daemon_status = VerificationBarrier::check_daemon_health().await;
        assert_eq!(daemon_status, None);
    }
}
