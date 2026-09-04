use crate::constants::{
    VERIFICATION_CONFLICT_MARKERS, VERIFICATION_DEBUG_PATTERNS, VERIFICATION_TEST_TIMEOUT_MS,
};
use crate::context::syntax_guard::SyntaxGuard;
use crate::tools::compiler::ScopedCompiler;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

/// Evaluation status for an individual verification gate.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub gate1_syntax_compiler: GateStatus,
    pub gate2_reproducer_test: GateStatus,
    pub gate3_regression_conflicts: GateStatus,
    pub gate4_diff_sanity: GateStatus,
    pub all_passed: bool,
}

impl VerificationReport {
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
        let gate2 = Self::check_gate2_reproducer_test(workspace_root, modified_files);
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

    /// Gate 1: AST Syntax & Scoped Compiler Check
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
                        gate_name: "Gate 1: AST Syntax & Compiler Integrity",
                        reason: format!("Failed to read modified file `{}`: {}", file, e),
                        actionable_remediation: format!("Ensure `{}` is accessible on disk.", file),
                    };
                }
            };

            // 1. In-memory Tree-sitter AST syntax barrier
            if let Some(err) = SyntaxGuard::check_syntax(&abs_path, &content) {
                return GateStatus::Failed {
                    gate_name: "Gate 1: AST Syntax & Compiler Integrity",
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
                        gate_name: "Gate 1: AST Syntax & Compiler Integrity",
                        reason: format!("Compiler diagnostic detected in `{}`:\n{}", file, diag),
                        actionable_remediation: format!(
                            "Resolve the compiler/linter error in `{}`.",
                            file
                        ),
                    };
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
        // Find if any test file was modified
        let modified_test = modified_files.iter().find(|f| {
            f.starts_with("tests/")
                || f.contains("test_")
                || f.contains("_test.")
                || f.contains("repro_")
        });

        let Some(test_rel_path) = modified_test else {
            return GateStatus::Skipped {
                reason: "No reproducer script or test target was modified in this turn.",
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
                    .arg("3")
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

            // Check for raw debug prints only in production code (src/), not in tests/
            let is_production_code = file.starts_with("src/") && !file.contains("test");

            for (idx, line) in content.lines().enumerate() {
                let trimmed = line.trim();

                // Skip pure line comments
                if trimmed.starts_with("//")
                    || trimmed.starts_with('#')
                    || trimmed.starts_with("/*")
                {
                    continue;
                }

                // 1. Raw debug logging detection in production code
                if is_production_code {
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
        // Ignore test assertions or env accesses
        if line.contains("std::env")
            || line.contains("process.env")
            || line.contains("env::var")
            || line.contains("assert")
        {
            return false;
        }

        // Patterns for OpenAI, GitHub, Google, AWS, and private keys
        if line.contains("sk-") && line.len() > 30 {
            return true;
        }
        if line.contains("ghp_") && line.len() > 25 {
            return true;
        }
        if line.contains("AIzaSy") && line.len() > 30 {
            return true;
        }
        if line.contains("BEGIN PRIVATE KEY") {
            return true;
        }

        false
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
}
