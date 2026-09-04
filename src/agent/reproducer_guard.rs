use crate::constants::{
    REPRODUCER_DIR_NAME, REPRODUCER_PREFIX, REPRODUCER_TIMEOUT_MS, WORKSPACE_DIR_NAME,
};
use crate::error::{MinicodeError, Result, ToolError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Lifecycle state of a standalone reproducer test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ReproducerPhase {
    /// Red phase confirmed: the reproducer failed as expected on unpatched codebase.
    RedConfirmed {
        exit_code: i32,
        failure_snippet: String,
        timestamp: u64,
    },
    /// Vacuous warning: the reproducer unexpectedly passed on unpatched codebase.
    VacuousWarning { timestamp: u64 },
    /// Compilation error: the reproducer code failed to compile.
    CompilationError {
        compiler_diagnostic: String,
        timestamp: u64,
    },
    /// Green phase verified: the reproducer passed on the patched codebase.
    GreenVerified { timestamp: u64 },
}

/// Metadata record tracking an individual standalone reproducer test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReproducerRecord {
    pub name: String,
    pub file_path: String,
    pub description: String,
    pub created_at: u64,
    pub status: ReproducerPhase,
}

/// Report summarizing the outcome of synthesizing or verifying a reproducer test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReproducerReport {
    RedConfirmed {
        name: String,
        file_path: String,
        exit_code: i32,
        failure_snippet: String,
    },
    VacuousWarning {
        name: String,
        file_path: String,
    },
    CompilationError {
        name: String,
        file_path: String,
        diagnostic: String,
    },
    GreenVerified {
        name: String,
        file_path: String,
    },
    StillFailing {
        name: String,
        file_path: String,
        exit_code: i32,
        failure_snippet: String,
    },
    WrittenOnly {
        name: String,
        file_path: String,
    },
}

impl ReproducerReport {
    /// Formats an actionable message suitable for model instruction or user output.
    #[must_use]
    pub fn format_message(&self) -> String {
        match self {
            Self::RedConfirmed {
                name,
                file_path,
                exit_code,
                failure_snippet,
            } => {
                let mut out = String::new();
                out.push_str(&format!(
                    "[RED PHASE CONFIRMED]: Standalone reproducer successfully synthesized at '{}'.\n",
                    file_path
                ));
                out.push_str(&format!(
                    "The test FAILED as expected on the unpatched codebase (exit code: {}).\n\n",
                    exit_code
                ));
                out.push_str("Failure Trace:\n");
                out.push_str("--------------------------------------------------\n");
                out.push_str(failure_snippet.trim());
                out.push_str("\n--------------------------------------------------\n\n");
                out.push_str(&format!(
                    "Next Step: Reproducer '{}' is now registered as an active regression guard.\n",
                    name
                ));
                out.push_str("Now patch the implementation in 'src/' using 'patch_file'.\n");
                out.push_str("When you conclude your turn, the Verification Barrier (Gate 2) will automatically verify this reproducer transitions from RED to GREEN.");
                out
            }
            Self::VacuousWarning { name: _, file_path } => {
                let mut out = String::new();
                out.push_str(&format!(
                    "[VACUOUS REPRODUCER WARNING]: Reproducer written to '{}', but it PASSED (exit code 0) on the current unpatched codebase!\n\n",
                    file_path
                ));
                out.push_str("A valid bug reproducer MUST fail against unpatched code to prove that the bug is real and to prevent false-positive completions.\n");
                out.push_str(&format!(
                    "Action Required: Revise the test assertions in '{}' so they trigger the bug before patching source code.",
                    file_path
                ));
                out
            }
            Self::CompilationError {
                name: _,
                file_path,
                diagnostic,
            } => {
                let mut out = String::new();
                out.push_str(&format!(
                    "[REPRODUCER COMPILATION ERROR]: Reproducer '{}' failed to compile:\n\n",
                    file_path
                ));
                out.push_str(diagnostic.trim());
                out.push_str(&format!(
                    "\n\nAction Required: Fix the syntax, module imports, or types in '{}'.",
                    file_path
                ));
                out
            }
            Self::GreenVerified { name: _, file_path } => {
                format!(
                    "[GREEN PHASE VERIFIED]: Reproducer '{}' PASSED with exit code 0!\n\
                     Red-to-Green transition is mathematically confirmed. This reproducer is now active as a permanent regression guard.",
                    file_path
                )
            }
            Self::StillFailing {
                name: _,
                file_path,
                exit_code,
                failure_snippet,
            } => {
                let mut out = String::new();
                out.push_str(&format!(
                    "[REPRODUCER STILL FAILING]: Reproducer '{}' exited with code {}:\n\n",
                    file_path, exit_code
                ));
                out.push_str(failure_snippet.trim());
                out.push_str("\n\nAction Required: Continue modifying code in 'src/' until this reproducer passes.");
                out
            }
            Self::WrittenOnly { name: _, file_path } => {
                format!(
                    "Successfully wrote reproducer test to '{}' without executing red phase.",
                    file_path
                )
            }
        }
    }
}

/// Core engine managing test-driven reproducer synthesis, Red-phase proof, and Green-phase verification.
pub struct ReproducerGuard;

impl ReproducerGuard {
    /// Normalizes an input reproducer name into a canonical test target name (e.g. `repro_empty_input`).
    #[must_use]
    pub fn normalize_target_name(raw_name: &str) -> String {
        let trimmed = raw_name
            .trim()
            .trim_end_matches(".rs")
            .trim_end_matches(".py");

        let base = if trimmed.starts_with(REPRODUCER_PREFIX) {
            trimmed.to_string()
        } else {
            format!("{}{}", REPRODUCER_PREFIX, trimmed)
        };

        // Filter to valid identifier characters
        base.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// Resolves the storage directory for active reproducer records (.minicode/reproducers).
    pub fn get_storage_dir(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(WORKSPACE_DIR_NAME)
            .join(REPRODUCER_DIR_NAME)
    }

    /// Loads an individual reproducer record from disk if present.
    pub fn load_record(workspace_root: &Path, target_name: &str) -> Option<ReproducerRecord> {
        let target = Self::normalize_target_name(target_name);
        let path = Self::get_storage_dir(workspace_root).join(format!("{}.json", target));
        if !path.exists() {
            return None;
        }

        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str::<ReproducerRecord>(&content).ok()
    }

    /// Persists a reproducer record to disk in `.minicode/reproducers/<name>.json`.
    pub fn save_record(workspace_root: &Path, record: &ReproducerRecord) -> Result<()> {
        let dir = Self::get_storage_dir(workspace_root);
        if !dir.exists() {
            fs::create_dir_all(&dir).map_err(|e| {
                MinicodeError::Tool(ToolError::CommandExec(format!(
                    "Failed to create reproducer store directory `{}`: {}",
                    dir.display(),
                    e
                )))
            })?;
        }

        let path = dir.join(format!("{}.json", record.name));
        let serialized = serde_json::to_string_pretty(record).map_err(|e| {
            MinicodeError::Tool(ToolError::CommandExec(format!(
                "Failed to serialize reproducer record: {}",
                e
            )))
        })?;

        fs::write(&path, serialized).map_err(|e| {
            MinicodeError::Tool(ToolError::CommandExec(format!(
                "Failed to write reproducer record to `{}`: {}",
                path.display(),
                e
            )))
        })?;

        Ok(())
    }

    /// Synthesizes a Rust standalone reproducer in `tests/<target>.rs` and optionally executes the Red Phase.
    pub fn synthesize_rust_reproducer(
        workspace_root: &Path,
        raw_name: &str,
        test_code: &str,
        description: &str,
        run_red_phase: bool,
    ) -> Result<ReproducerReport> {
        let target_name = Self::normalize_target_name(raw_name);
        let test_rel_path = format!("tests/{}.rs", target_name);
        let test_abs_path = workspace_root.join(&test_rel_path);

        // Ensure tests directory exists
        if let Some(parent) = test_abs_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    MinicodeError::Tool(ToolError::CommandExec(format!(
                        "Failed to create tests directory `{}`: {}",
                        parent.display(),
                        e
                    )))
                })?;
            }
        }

        // Write the reproducer code to disk
        fs::write(&test_abs_path, test_code).map_err(|e| {
            MinicodeError::Tool(ToolError::CommandExec(format!(
                "Failed to write reproducer test file `{}`: {}",
                test_abs_path.display(),
                e
            )))
        })?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if !run_red_phase {
            let record = ReproducerRecord {
                name: target_name.clone(),
                file_path: test_rel_path.clone(),
                description: description.to_string(),
                created_at: timestamp,
                status: ReproducerPhase::VacuousWarning { timestamp },
            };
            let _ = Self::save_record(workspace_root, &record);
            return Ok(ReproducerReport::WrittenOnly {
                name: target_name,
                file_path: test_rel_path,
            });
        }

        // Execute Red Phase: must fail against unpatched codebase
        let execution_result = Self::run_single_test(workspace_root, &target_name);

        match execution_result {
            Ok(output) => {
                if output.success {
                    // Vacuous: test passed on unpatched code
                    let record = ReproducerRecord {
                        name: target_name.clone(),
                        file_path: test_rel_path.clone(),
                        description: description.to_string(),
                        created_at: timestamp,
                        status: ReproducerPhase::VacuousWarning { timestamp },
                    };
                    let _ = Self::save_record(workspace_root, &record);
                    Ok(ReproducerReport::VacuousWarning {
                        name: target_name,
                        file_path: test_rel_path,
                    })
                } else if output.is_compilation_error {
                    // Failed to compile
                    let record = ReproducerRecord {
                        name: target_name.clone(),
                        file_path: test_rel_path.clone(),
                        description: description.to_string(),
                        created_at: timestamp,
                        status: ReproducerPhase::CompilationError {
                            compiler_diagnostic: output.diagnostic.clone(),
                            timestamp,
                        },
                    };
                    let _ = Self::save_record(workspace_root, &record);
                    Ok(ReproducerReport::CompilationError {
                        name: target_name,
                        file_path: test_rel_path,
                        diagnostic: output.diagnostic,
                    })
                } else {
                    // RED PHASE CONFIRMED!
                    let record = ReproducerRecord {
                        name: target_name.clone(),
                        file_path: test_rel_path.clone(),
                        description: description.to_string(),
                        created_at: timestamp,
                        status: ReproducerPhase::RedConfirmed {
                            exit_code: output.exit_code,
                            failure_snippet: output.diagnostic.clone(),
                            timestamp,
                        },
                    };
                    Self::save_record(workspace_root, &record)?;
                    Ok(ReproducerReport::RedConfirmed {
                        name: target_name,
                        file_path: test_rel_path,
                        exit_code: output.exit_code,
                        failure_snippet: output.diagnostic,
                    })
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Verifies the current state of a reproducer test target.
    pub fn verify_reproducer(workspace_root: &Path, raw_name: &str) -> Result<ReproducerReport> {
        let target_name = Self::normalize_target_name(raw_name);
        let test_rel_path = format!("tests/{}.rs", target_name);
        let test_abs_path = workspace_root.join(&test_rel_path);

        if !test_abs_path.exists() {
            return Err(MinicodeError::Tool(ToolError::CommandExec(format!(
                "Reproducer test `{}` does not exist on disk.",
                test_rel_path
            ))));
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let execution_result = Self::run_single_test(workspace_root, &target_name)?;

        if execution_result.success {
            // GREEN PHASE CONFIRMED!
            if let Some(mut record) = Self::load_record(workspace_root, &target_name) {
                record.status = ReproducerPhase::GreenVerified { timestamp };
                let _ = Self::save_record(workspace_root, &record);
            } else {
                let record = ReproducerRecord {
                    name: target_name.clone(),
                    file_path: test_rel_path.clone(),
                    description: "Ad-hoc reproducer test".to_string(),
                    created_at: timestamp,
                    status: ReproducerPhase::GreenVerified { timestamp },
                };
                let _ = Self::save_record(workspace_root, &record);
            }

            Ok(ReproducerReport::GreenVerified {
                name: target_name,
                file_path: test_rel_path,
            })
        } else {
            Ok(ReproducerReport::StillFailing {
                name: target_name,
                file_path: test_rel_path,
                exit_code: execution_result.exit_code,
                failure_snippet: execution_result.diagnostic,
            })
        }
    }

    /// Lists all active reproducer records stored in `.minicode/reproducers/`.
    pub fn list_active_reproducers(workspace_root: &Path) -> Vec<ReproducerRecord> {
        let dir = Self::get_storage_dir(workspace_root);
        if !dir.exists() {
            return Vec::new();
        }

        let mut records = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(rec) = serde_json::from_str::<ReproducerRecord>(&content) {
                            records.push(rec);
                        }
                    }
                }
            }
        }

        records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        records
    }

    /// Formats a markdown table summarizing all active reproducers.
    pub fn format_reproducer_list(records: &[ReproducerRecord]) -> String {
        if records.is_empty() {
            return "No active reproducer tests registered in this workspace.\n\
                    Use 'synthesize_reproducer' to create a test-driven reproducer before patching code."
                .to_string();
        }

        let mut out = String::new();
        out.push_str(&format!(
            "### Active TDD Bug Reproducers ({} registered)\n\n",
            records.len()
        ));
        out.push_str("| Status | Test Target | Description | Phase Details |\n");
        out.push_str("| :--- | :--- | :--- | :--- |\n");

        for rec in records {
            let (status_icon, status_label, details) = match &rec.status {
                ReproducerPhase::RedConfirmed { exit_code, .. } => (
                    "🔴",
                    "RED CONFIRMED",
                    format!("Exit code: {} (Failed on unpatched code)", exit_code),
                ),
                ReproducerPhase::GreenVerified { .. } => (
                    "🟢",
                    "GREEN VERIFIED",
                    "Passed with exit code 0 (Fix verified)".to_string(),
                ),
                ReproducerPhase::VacuousWarning { .. } => (
                    "🟡",
                    "VACUOUS WARNING",
                    "Passed on unpatched code (Does not reproduce bug)".to_string(),
                ),
                ReproducerPhase::CompilationError { .. } => (
                    "❌",
                    "COMPILE ERROR",
                    "Failed to compile test target".to_string(),
                ),
            };

            out.push_str(&format!(
                "| {} **{}** | `{}` | {} | {} |\n",
                status_icon, status_label, rec.file_path, rec.description, details
            ));
        }

        out
    }

    /// Runs a single Rust test target with `-j 3` and enforces timeout.
    fn run_single_test(workspace_root: &Path, target_name: &str) -> Result<TestExecutionOutcome> {
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

        match rx.recv_timeout(Duration::from_millis(REPRODUCER_TIMEOUT_MS)) {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(1);
                let success = output.status.success();

                let is_compilation_error = !success
                    && (stderr.contains("error[E")
                        || stderr.contains("error: could not compile")
                        || stdout.contains("error[E")
                        || stdout.contains("error: could not compile"));

                let diagnostic = if !stderr.trim().is_empty() {
                    stderr.lines().take(15).collect::<Vec<_>>().join("\n")
                } else {
                    stdout.lines().take(15).collect::<Vec<_>>().join("\n")
                };

                Ok(TestExecutionOutcome {
                    success,
                    exit_code,
                    is_compilation_error,
                    diagnostic,
                })
            }
            Ok(Err(e)) => Err(MinicodeError::Tool(ToolError::CommandExec(format!(
                "Failed to execute cargo test runner for reproducer target `{}`: {}",
                target_name, e
            )))),
            Err(_) => Err(MinicodeError::Tool(ToolError::CommandExec(format!(
                "Reproducer test `{}` execution timed out after {}ms",
                target_name, REPRODUCER_TIMEOUT_MS
            )))),
        }
    }
}

/// Internal execution outcome of running a reproducer test target.
struct TestExecutionOutcome {
    success: bool,
    exit_code: i32,
    is_compilation_error: bool,
    diagnostic: String,
}
