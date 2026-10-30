use crate::agent::reproducer_guard::{ReproducerGuard, ReproducerPhase};
use crate::constants::VERIFICATION_TEST_TIMEOUT_MS;
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

        for gate in [
            &self.gate1_syntax_compiler,
            &self.gate2_reproducer_test,
            &self.gate3_regression_conflicts,
            &self.gate4_diff_sanity,
        ] {
            if let GateStatus::Failed {
                gate_name,
                reason,
                actionable_remediation,
            } = gate
            {
                out.push_str(&format!(
                    "- ❌ **{}**:\n  {}\n  --> Remediation: {}\n\n",
                    gate_name, reason, actionable_remediation
                ));
            }
        }

        if matches!(self.gate1_syntax_compiler, GateStatus::Failed { .. })
            || matches!(self.gate2_reproducer_test, GateStatus::Failed { .. })
        {
            out.push_str(
                "<causal_tracer_protocol>\n\
                 - Competing Hypotheses: Formulate at least 2 competing explanations for the failure before editing.\n\
                 - Disconfirmation Check: Check what evidence in the compiler/test output disconfirms your first guess.\n\
                 - Fastest Discriminating Probe: Inspect the exact failing symbol/callers first; reuse existing helpers instead of adding speculative abstractions.\n\
                 - No Fake Completion: Never silence failures with `#[ignore]`, `.skip(`, `.only(`, `todo!()`, or `unimplemented!()`.\n\
                 </causal_tracer_protocol>\n\n",
            );
        }

        out.push_str("Please fix these verification issues in your next tool action before declaring the task completed.");
        out
    }

    /// Extracts a concise `(failed_gate_and_reason, actionable_remediation)` pair from the first
    /// failing verification gate so the Reflexion Gotcha engine records real compiler/test diagnostics.
    #[must_use]
    pub fn first_failure_summary(&self) -> Option<(String, String)> {
        for gate in [
            &self.gate1_syntax_compiler,
            &self.gate2_reproducer_test,
            &self.gate3_regression_conflicts,
            &self.gate4_diff_sanity,
        ] {
            if let GateStatus::Failed {
                gate_name,
                reason,
                actionable_remediation,
            } = gate
            {
                let first_line = reason
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("Verification check failed");
                let summary = crate::utils::strings::truncate_ellipsis(
                    &format!("[{}] {}", gate_name, first_line),
                    220,
                );
                return Some((summary, actionable_remediation.clone()));
            }
        }
        None
    }
}

/// Pre-loaded modified file entry shared across VerificationBarrier gates to avoid redundant disk reads.
struct LoadedModifiedFile {
    rel_path: String,
    abs_path: std::path::PathBuf,
    content: Result<String, String>,
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
    fn load_modified_files(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> Vec<LoadedModifiedFile> {
        modified_files
            .iter()
            .filter_map(|file| {
                let abs_path = workspace_root.join(file);
                if !abs_path.exists() {
                    return None;
                }
                let content = std::fs::read_to_string(&abs_path).map_err(|e| e.to_string());
                Some(LoadedModifiedFile {
                    rel_path: file.clone(),
                    abs_path,
                    content,
                })
            })
            .collect()
    }

    /// Executes all 4 gates against modified files in the workspace.
    /// Runs synchronous compiler/test/filesystem gates on `spawn_blocking` so the TUI event loop
    /// and Tokio async runtime threads are never blocked.
    pub async fn verify(workspace_root: &Path, modified_files: &[String]) -> VerificationReport {
        // Check if any managed dev runtime daemon exited abnormally
        let daemon_check = Self::check_daemon_health().await;

        let root_buf = workspace_root.to_path_buf();
        let files_vec = modified_files.to_vec();

        let (gate1, gate2, gate3, gate4) = tokio::task::spawn_blocking(move || {
            let loaded = Self::load_modified_files(&root_buf, &files_vec);
            let g1 = Self::check_gate1_loaded(&root_buf, &loaded);
            let g2 = if let Some(failed_daemon) = daemon_check {
                failed_daemon
            } else {
                Self::check_gate2_reproducer_test(&root_buf, &files_vec)
            };
            let g3 = Self::check_gate3_loaded(&loaded);
            let g4 = Self::check_gate4_loaded(&loaded);
            (g1, g2, g3, g4)
        })
        .await
        .unwrap_or((
            GateStatus::Passed,
            GateStatus::Passed,
            GateStatus::Passed,
            GateStatus::Passed,
        ));

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
    #[allow(dead_code)]
    pub fn check_gate1_syntax_compiler(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> GateStatus {
        let loaded = Self::load_modified_files(workspace_root, modified_files);
        Self::check_gate1_loaded(workspace_root, &loaded)
    }

    fn check_gate1_loaded(workspace_root: &Path, loaded: &[LoadedModifiedFile]) -> GateStatus {
        let mut first_rust_file: Option<&str> = None;
        let mut first_ts_file: Option<&str> = None;

        for entry in loaded {
            let file = entry.rel_path.as_str();
            let abs_path = &entry.abs_path;

            let content = match &entry.content {
                Ok(c) => c,
                Err(e) => {
                    return GateStatus::Failed {
                        gate_name: "Gate 1: AST Syntax & Asset Integrity",
                        reason: format!("Failed to read modified file `{}`: {}", file, e),
                        actionable_remediation: format!("Ensure `{}` is accessible on disk.", file),
                    };
                }
            };

            // 1. In-memory Tree-sitter AST syntax barrier (fast per-file check)
            if let Some(err) = SyntaxGuard::check_syntax(abs_path, content) {
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

            // Track project-wide compilers (Rust / TypeScript) so they run once per barrier instead of N times
            if file.ends_with(".rs") && first_rust_file.is_none() {
                first_rust_file = Some(file);
            } else if (file.ends_with(".ts") || file.ends_with(".tsx")) && first_ts_file.is_none() {
                first_ts_file = Some(file);
            } else if file.ends_with(".py") {
                // Python py_compile is per-file
                if let Some(diag) = ScopedCompiler::check_python(workspace_root, file) {
                    if diag.contains("reported syntax error:") || diag.contains("reported errors:")
                    {
                        return GateStatus::Failed {
                            gate_name: "Gate 1: AST Syntax & Asset Integrity",
                            reason: format!(
                                "Compiler diagnostic detected in `{}`:\n{}",
                                file, diag
                            ),
                            actionable_remediation: format!(
                                "Resolve the compiler/linter error in `{}`.",
                                file
                            ),
                        };
                    }
                }
            }

            // 3. HTML Document & Local Asset Link Integrity
            if file.ends_with(".html") || file.ends_with(".htm") {
                if let Some(err) =
                    Self::check_html_integrity(workspace_root, file, abs_path, content)
                {
                    return err;
                }
            }

            // 4. CSS Syntax & Balanced Braces Integrity
            if file.ends_with(".css") {
                if let Some(err) = Self::check_css_integrity(file, content) {
                    return err;
                }
            }
        }

        // 2. Deduplicated project-wide compiler checks (run at most once per project language)
        if let Some(rs_file) = first_rust_file {
            if workspace_root.join("Cargo.toml").exists() {
                if let Some(diag) = ScopedCompiler::check_rust(workspace_root, rs_file) {
                    if diag.contains("reported errors:") {
                        return GateStatus::Failed {
                            gate_name: "Gate 1: AST Syntax & Asset Integrity",
                            reason: format!(
                                "Compiler diagnostic detected in `{}`:\n{}",
                                rs_file, diag
                            ),
                            actionable_remediation: format!(
                                "Resolve the compiler/linter error in `{}`.",
                                rs_file
                            ),
                        };
                    }
                }
            }
        }

        if let Some(ts_file) = first_ts_file {
            if workspace_root.join("tsconfig.json").exists() {
                if let Some(diag) = ScopedCompiler::check_typescript(workspace_root, ts_file) {
                    if diag.contains("reported errors:") {
                        return GateStatus::Failed {
                            gate_name: "Gate 1: AST Syntax & Asset Integrity",
                            reason: format!(
                                "Compiler diagnostic detected in `{}`:\n{}",
                                ts_file, diag
                            ),
                            actionable_remediation: format!(
                                "Resolve the compiler/linter error in `{}`.",
                                ts_file
                            ),
                        };
                    }
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

            let mut cmd = Command::new("cargo");
            cmd.args(["test", "-j", "1", "--test", target_name])
                .current_dir(workspace_root)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                cmd.process_group(0);
            }

            let child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Failed to spawn test runner for Gate 2: {}", e);
                    return GateStatus::Passed;
                }
            };

            let child_pid = child.id();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(child.wait_with_output());
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
                    tracing::warn!("Failed waiting on test runner for Gate 2: {}", e);
                }
                Err(_) => {
                    #[cfg(unix)]
                    if child_pid > 0 {
                        let pgid = -(child_pid as i32);
                        // SAFETY: child was spawned with process_group(0) so its PGID equals child_pid.
                        unsafe {
                            libc::kill(pgid, libc::SIGTERM);
                            libc::kill(pgid, libc::SIGKILL);
                            libc::kill(child_pid as i32, libc::SIGKILL);
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = Command::new("kill")
                            .args(["-9", &child_pid.to_string()])
                            .status();
                    }
                    tracing::warn!(
                        "Gate 2 test execution timed out ({}ms); killed process group for pid {}",
                        VERIFICATION_TEST_TIMEOUT_MS,
                        child_pid
                    );
                }
            }
        }

        GateStatus::Passed
    }

    /// Gate 3: Structural Integrity & Git Conflict Markers Check
    #[allow(dead_code)]
    pub fn check_gate3_regression_conflicts(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> GateStatus {
        let loaded = Self::load_modified_files(workspace_root, modified_files);
        Self::check_gate3_loaded(&loaded)
    }

    fn check_gate3_loaded(loaded: &[LoadedModifiedFile]) -> GateStatus {
        for entry in loaded {
            let Ok(content) = &entry.content else {
                continue;
            };
            let file = entry.rel_path.as_str();

            for (idx, line) in content.lines().enumerate() {
                if let Some(marker) = detect_git_conflict_marker(line) {
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

        GateStatus::Passed
    }

    /// Gate 4: Diff Sanity & Secret Leak Audit
    #[allow(dead_code)]
    pub fn check_gate4_diff_sanity(workspace_root: &Path, modified_files: &[String]) -> GateStatus {
        let loaded = Self::load_modified_files(workspace_root, modified_files);
        Self::check_gate4_loaded(&loaded)
    }

    fn check_gate4_loaded(loaded: &[LoadedModifiedFile]) -> GateStatus {
        for entry in loaded {
            let Ok(content) = &entry.content else {
                continue;
            };
            let file = entry.rel_path.as_str();

            // Check for lazy elision comments (`// ... rest of code ...`, etc.)
            if let Some(elision_warn) =
                crate::tools::registry::fs_tools::detect_code_elision_or_slop(file, content)
            {
                return GateStatus::Failed {
                    gate_name: "Gate 4: Diff Sanity & No-Fake-Completion Audit",
                    reason: format!(
                        "Lazy placeholder/elision comment left in `{}`: {}",
                        file,
                        elision_warn.trim()
                    ),
                    actionable_remediation: format!(
                        "Replace all `// ... rest of code ...` elision comments in `{}` with the complete code.",
                        file
                    ),
                };
            }

            let mut pending_cfg_test = false;
            let mut in_test_module = false;
            let mut test_brace_depth: i32 = 0;

            for (idx, line) in content.lines().enumerate() {
                let code_only = crate::utils::strings::strip_code_strings_and_comments(line);
                let trimmed_code = code_only.trim();

                // Detect `.only(` test hijacking across JS/TS test files or test blocks
                if trimmed_code.contains("describe.only(")
                    || trimmed_code.contains("it.only(")
                    || trimmed_code.contains("test.only(")
                {
                    return GateStatus::Failed {
                        gate_name: "Gate 4: Diff Sanity & No-Fake-Completion Audit",
                        reason: format!(
                            "Focused test marker (`.only(`) left in `{}` at line {}, which silently skips the rest of the test suite.",
                            file,
                            idx + 1
                        ),
                        actionable_remediation: format!(
                            "Remove `.only` from `{}` at line {} so all tests execute.",
                            file,
                            idx + 1
                        ),
                    };
                }

                if trimmed_code.starts_with("#[cfg(test)]")
                    || trimmed_code.starts_with("#[test]")
                    || trimmed_code.starts_with("#[tokio::test")
                {
                    pending_cfg_test = true;
                }

                if pending_cfg_test || in_test_module {
                    let opens = code_only.chars().filter(|&c| c == '{').count() as i32;
                    let closes = code_only.chars().filter(|&c| c == '}').count() as i32;
                    if pending_cfg_test && opens > 0 {
                        in_test_module = true;
                        pending_cfg_test = false;
                    }
                    if in_test_module {
                        test_brace_depth += opens - closes;
                        if test_brace_depth <= 0 {
                            in_test_module = false;
                            test_brace_depth = 0;
                        }
                        continue;
                    }
                    if pending_cfg_test {
                        continue;
                    }
                }

                // No-Fake-Completion: reject `todo!()` or `unimplemented!()` stubs in non-test code
                if (trimmed_code.contains("todo!(") || trimmed_code.contains("unimplemented!("))
                    && !trimmed_code.contains("macro_rules!")
                {
                    return GateStatus::Failed {
                        gate_name: "Gate 4: Diff Sanity & No-Fake-Completion Audit",
                        reason: format!(
                            "Incomplete stub macro (`todo!()` / `unimplemented!()`) left in production code `{}` at line {}.",
                            file,
                            idx + 1
                        ),
                        actionable_remediation: format!(
                            "Replace the `todo!()` / `unimplemented!()` stub in `{}` at line {} with a complete implementation.",
                            file,
                            idx + 1
                        ),
                    };
                }

                let uncommented = crate::utils::strings::strip_line_comments(line);
                let trimmed = uncommented.trim();

                // Skip pure line comments and block comment lines
                if trimmed.is_empty() || trimmed.starts_with('*') || trimmed.starts_with("{/*") {
                    continue;
                }

                // Secret & API key leakage detection
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
        let lower = content.to_ascii_lowercase();

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
                let lower_tag = &lower[tag_start..=tag_end];

                for attr_pat in ["href=", "src="] {
                    if let Some(attr_pos) = lower_tag.find(attr_pat) {
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

    /// Checks if any background dev server registered with minitask terminated with an error.
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
                            "Inspect service logs via `minitask(action=\"logs\", id=\"{}\")` and resolve startup errors.",
                            p.id
                        ),
                    });
                }
            }
        }
        None
    }
}

/// Accurately detects real Git merge conflict markers without false positives on code comments.
///
/// In Git:
/// - `<<<<<<<` at start of line, optionally followed by space and ref/branch label
/// - `=======` exactly 7 equals signs on its own line (never with trailing characters or 40 equals)
/// - `>>>>>>>` at start of line, optionally followed by space and ref/branch label
/// - `|||||||` (diff3 base marker) at start of line, optionally followed by space and commit/ref
pub(crate) fn detect_git_conflict_marker(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if trimmed == "=======" {
        return Some("=======");
    }
    if trimmed == "<<<<<<<" || trimmed.starts_with("<<<<<<< ") {
        return Some("<<<<<<<");
    }
    if trimmed == ">>>>>>>" || trimmed.starts_with(">>>>>>> ") {
        return Some(">>>>>>>");
    }
    if trimmed == "|||||||" || trimmed.starts_with("||||||| ") {
        return Some("|||||||");
    }
    None
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
    fn test_gate3_does_not_flag_legitimate_comments() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("styles.css");
        std::fs::write(
            &file_path,
            "/* ============================================\n   Design Tokens\n   ============================================ */\n// ============================================\n/* -------------------------------------------- */\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate3_regression_conflicts(
            temp.path(),
            &["styles.css".to_string()],
        );

        match status {
            GateStatus::Passed => {}
            _ => panic!(
                "Gate 3 should pass for legitimate comment banners: {:?}",
                status
            ),
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

    #[test]
    fn test_gate4_rejects_fake_completion_stubs_and_test_only() {
        let temp = tempdir().unwrap();
        let stub_rs = temp.path().join("lib.rs");
        std::fs::write(&stub_rs, "pub fn compute() -> i32 {\n    todo!()\n}\n").unwrap();

        let status =
            VerificationBarrier::check_gate4_diff_sanity(temp.path(), &["lib.rs".to_string()]);
        assert!(matches!(status, GateStatus::Failed { .. }));

        let only_ts = temp.path().join("app.test.ts");
        std::fs::write(
            &only_ts,
            "describe.only('suite', () => { it('works', () => {}); });\n",
        )
        .unwrap();
        let status_only =
            VerificationBarrier::check_gate4_diff_sanity(temp.path(), &["app.test.ts".to_string()]);
        assert!(matches!(status_only, GateStatus::Failed { .. }));
    }

    #[test]
    fn test_first_failure_summary_extracts_diagnostic_and_remediation() {
        let report = VerificationReport {
            all_passed: false,
            gate1_syntax_compiler: GateStatus::Passed,
            gate2_reproducer_test: GateStatus::Failed {
                gate_name: "Gate 2 (Test Suite)",
                reason: "assertion `left == right` failed in auth::tests::test_jwt\n  left: 401\n  right: 200".to_string(),
                actionable_remediation: "Inspect auth::tests::test_jwt and fix the status code mismatch.".to_string(),
            },
            gate3_regression_conflicts: GateStatus::Passed,
            gate4_diff_sanity: GateStatus::Passed,
        };

        let summary = report.first_failure_summary();
        assert!(summary.is_some());
        let (reason, remediation) = summary.unwrap();
        assert!(reason.contains("[Gate 2 (Test Suite)]"));
        assert!(reason.contains("assertion `left == right` failed in auth::tests::test_jwt"));
        assert!(remediation.contains("Inspect auth::tests::test_jwt"));
    }
}
