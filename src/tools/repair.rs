use crate::constants::REPAIR_VERIFY_TIMEOUT_SECS;
use crate::error::{Result, ToolError};
use crate::sandbox::env::build_sanitized_command;
use crate::sandbox::path::validate_path_in_workspace;
use crate::tools::compiler::ScopedCompiler;
use crate::tools::fs::{patch_file, write_file};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairResult {
    pub success: bool,
    pub file_path: String,
    pub patch_message: String,
    pub verification_cmd: Option<String>,
    pub verification_passed: bool,
    pub verification_output: Option<String>,
    pub rolled_back: bool,
    pub compiler_feedback: Option<String>,
}

impl RepairResult {
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        if self.success {
            out.push_str("### 🛠️ Surgical Repair Succeeded\n\n");
            out.push_str(&format!("- **Target File:** `{}`\n", self.file_path));
            out.push_str(&format!("- **Patch Status:** {}\n", self.patch_message));
            if let Some(ref cmd) = self.verification_cmd {
                out.push_str(&format!("- **Verification:** Passed (`{}`)\n", cmd));
            } else if let Some(ref feedback) = self.compiler_feedback {
                out.push_str(&format!("- **Compiler Status:** {}\n", feedback.trim()));
            } else {
                out.push_str("- **Verification:** Clean (no compilation diagnostics)\n");
            }
            if let Some(ref out_text) = self.verification_output {
                if !out_text.trim().is_empty() {
                    out.push_str(&format!("\n```\n{}\n```\n", out_text.trim()));
                }
            }
        } else {
            out.push_str("### ❌ Surgical Repair Failed\n\n");
            out.push_str(&format!("- **Target File:** `{}`\n", self.file_path));
            if self.rolled_back {
                out.push_str("- **Safety Action:** ⏪ Automatically rolled back file to pristine pre-patch state.\n");
            }
            out.push_str(&format!("- **Patch Message:** {}\n", self.patch_message));
            if let Some(ref cmd) = self.verification_cmd {
                out.push_str(&format!("- **Failed Verification Command:** `{}`\n", cmd));
            }
            if let Some(ref v_out) = self.verification_output {
                out.push_str("\n#### 🚨 Verification Failure Output:\n```\n");
                out.push_str(v_out.trim());
                out.push_str("\n```\n");
            }
            if let Some(ref feedback) = self.compiler_feedback {
                out.push_str("\n#### ⚠️ Compiler Diagnostics:\n```\n");
                out.push_str(feedback.trim());
                out.push_str("\n```\n");
            }
            out.push_str("\n#### 💡 Prescriptive Next Actions:\n");
            out.push_str("1. Review the error details and compiler diagnostics above.\n");
            out.push_str(
                "2. Inspect the symbol definitions or run `locate_fault` for fresh line numbers.\n",
            );
            out.push_str("3. Correct the patch logic and reissue `repair_patch`.\n");
        }
        out
    }
}

pub struct SurgicalRepairEngine;

impl SurgicalRepairEngine {
    /// Executes a surgical patch with atomic pre-flight snapshot, resilient search-and-replace,
    /// pre-flight verification gate, and automated rollback if tests or compilation break.
    pub async fn execute_surgical_repair(
        workspace_root: &Path,
        relative_path: &str,
        search_block: &str,
        replace_block: &str,
        verification_cmd: Option<&str>,
    ) -> Result<RepairResult> {
        let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;
        if !target_path.is_file() {
            return Err(ToolError::FileOp {
                path: relative_path.to_string(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"),
            }
            .into());
        }

        let original_content =
            std::fs::read_to_string(&target_path).map_err(|e| ToolError::FileOp {
                path: relative_path.to_string(),
                source: e,
            })?;

        // 1. Apply patch using 5-tier resilient matcher
        let patch_msg = match patch_file(workspace_root, relative_path, search_block, replace_block)
        {
            Ok(msg) => msg,
            Err(e) => {
                return Ok(RepairResult {
                    success: false,
                    file_path: relative_path.to_string(),
                    patch_message: format!("Patch failed to apply: {}", e),
                    verification_cmd: verification_cmd.map(|s| s.to_string()),
                    verification_passed: false,
                    verification_output: None,
                    rolled_back: false,
                    compiler_feedback: None,
                });
            }
        };

        // 2. Verification Gate
        if let Some(cmd_str) = verification_cmd {
            let (passed, output) = Self::run_verification_cmd(workspace_root, cmd_str).await;
            if !passed {
                // Automated rollback
                let rollback_res = write_file(workspace_root, relative_path, &original_content);
                let rolled_back = rollback_res.is_ok();

                return Ok(RepairResult {
                    success: false,
                    file_path: relative_path.to_string(),
                    patch_message: patch_msg,
                    verification_cmd: Some(cmd_str.to_string()),
                    verification_passed: false,
                    verification_output: Some(output),
                    rolled_back,
                    compiler_feedback: None,
                });
            }

            Ok(RepairResult {
                success: true,
                file_path: relative_path.to_string(),
                patch_message: patch_msg,
                verification_cmd: Some(cmd_str.to_string()),
                verification_passed: true,
                verification_output: Some(output),
                rolled_back: false,
                compiler_feedback: None,
            })
        } else {
            // Default: run scoped compiler check
            let compiler_feedback = ScopedCompiler::run_scoped_check(workspace_root, relative_path);

            let is_error = compiler_feedback
                .as_ref()
                .map(|fb| {
                    fb.contains("[Compiler Feedback]") || fb.contains("⚠️") || fb.contains("error[")
                })
                .unwrap_or(false);

            if is_error {
                // Automated rollback
                let rollback_res = write_file(workspace_root, relative_path, &original_content);
                let rolled_back = rollback_res.is_ok();

                return Ok(RepairResult {
                    success: false,
                    file_path: relative_path.to_string(),
                    patch_message: patch_msg,
                    verification_cmd: None,
                    verification_passed: false,
                    verification_output: None,
                    rolled_back,
                    compiler_feedback,
                });
            }

            Ok(RepairResult {
                success: true,
                file_path: relative_path.to_string(),
                patch_message: patch_msg,
                verification_cmd: None,
                verification_passed: true,
                verification_output: None,
                rolled_back: false,
                compiler_feedback,
            })
        }
    }

    async fn run_verification_cmd(workspace_root: &Path, command_str: &str) -> (bool, String) {
        let mut std_cmd = build_sanitized_command("sh", workspace_root);
        std_cmd.arg("-c").arg(command_str);

        #[cfg(target_os = "linux")]
        {
            let ws = workspace_root.to_path_buf();
            unsafe {
                use std::os::unix::process::CommandExt;
                std_cmd.pre_exec(move || {
                    crate::sandbox::landlock::apply_landlock_sandbox(&ws, true).map_err(|e| {
                        std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            format!("Landlock sandbox failed: {}", e),
                        )
                    })
                });
            }
        }

        let mut tokio_cmd = tokio::process::Command::from(std_cmd);
        tokio_cmd.kill_on_drop(true);
        tokio_cmd.stdout(std::process::Stdio::piped());
        tokio_cmd.stderr(std::process::Stdio::piped());

        #[cfg(unix)]
        {
            tokio_cmd.process_group(0);
        }

        let mut child = match tokio_cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return (
                    false,
                    format!("Failed to spawn verification process: {}", e),
                )
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let max_read_bytes = (crate::constants::EXEC_MAX_OUTPUT_BYTES as u64) + 1;

        let read_stdout = async {
            if let Some(mut out) = stdout {
                use tokio::io::AsyncReadExt;
                let mut limited = (&mut out).take(max_read_bytes);
                let _ = limited.read_to_end(&mut stdout_buf).await;
                let _ = tokio::io::copy(&mut out, &mut tokio::io::sink()).await;
            }
        };

        let read_stderr = async {
            if let Some(mut err) = stderr {
                use tokio::io::AsyncReadExt;
                let mut limited = (&mut err).take(max_read_bytes);
                let _ = limited.read_to_end(&mut stderr_buf).await;
                let _ = tokio::io::copy(&mut err, &mut tokio::io::sink()).await;
            }
        };

        let run_fut = async {
            tokio::join!(read_stdout, read_stderr);
            child.wait().await
        };

        let timeout = Duration::from_secs(REPAIR_VERIFY_TIMEOUT_SECS);
        let status = match tokio::time::timeout(timeout, run_fut).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return (false, format!("Process wait error: {}", e)),
            Err(_) => {
                #[cfg(unix)]
                if let Some(pid) = child.id() {
                    unsafe {
                        libc::kill(-(pid as i32), libc::SIGTERM);
                    }
                    tokio::time::sleep(Duration::from_millis(
                        crate::constants::PROCESS_KILL_GRACE_PERIOD_MS,
                    ))
                    .await;
                    unsafe {
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                }
                let _ = child.kill().await;
                return (
                    false,
                    format!(
                        "Verification command timed out after {} seconds.",
                        timeout.as_secs()
                    ),
                );
            }
        };

        let stdout_str = String::from_utf8_lossy(&stdout_buf);
        let stderr_str = String::from_utf8_lossy(&stderr_buf);

        let mut combined = String::new();
        if !stdout_str.is_empty() {
            combined.push_str(&stdout_str);
        }
        if !stderr_str.is_empty() {
            if !combined.is_empty() && !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str("[stderr]: ");
            combined.push_str(&stderr_str);
        }

        if combined.len() > crate::constants::EXEC_MAX_OUTPUT_BYTES {
            let valid_end = combined.floor_char_boundary(crate::constants::EXEC_MAX_OUTPUT_BYTES);
            let truncated = &combined[..valid_end];
            combined = format!(
                "{}\n\n[... Output truncated: exceeded max limit ...]",
                truncated
            );
        }

        (status.success(), combined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_repair_patch_success() {
        let temp = TempDir::new().unwrap();
        let file_path = "src/calc.rs";
        let initial_code = "pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n";
        let _ = write_file(temp.path(), file_path, initial_code).unwrap();

        let search = "    a - b";
        let replace = "    a + b";
        let res = SurgicalRepairEngine::execute_surgical_repair(
            temp.path(),
            file_path,
            search,
            replace,
            Some("echo 'test passed'"),
        )
        .await
        .unwrap();

        assert!(res.success);
        assert!(!res.rolled_back);
        assert!(res.verification_passed);

        let content = std::fs::read_to_string(temp.path().join(file_path)).unwrap();
        assert!(content.contains("a + b"));
    }

    #[tokio::test]
    async fn test_repair_patch_rollback_on_failure() {
        let temp = TempDir::new().unwrap();
        let file_path = "src/calc.rs";
        let initial_code = "pub fn mul(a: i32, b: i32) -> i32 {\n    a * b\n}\n";
        let _ = write_file(temp.path(), file_path, initial_code).unwrap();

        let search = "    a * b";
        let replace = "    a * b * 0";
        let res = SurgicalRepairEngine::execute_surgical_repair(
            temp.path(),
            file_path,
            search,
            replace,
            Some("sh -c 'exit 1'"),
        )
        .await
        .unwrap();

        assert!(!res.success);
        assert!(res.rolled_back);
        assert!(!res.verification_passed);

        // Content must be restored to initial code
        let content = std::fs::read_to_string(temp.path().join(file_path)).unwrap();
        assert_eq!(content, initial_code);
    }

    #[tokio::test]
    async fn test_repair_patch_not_found() {
        let temp = TempDir::new().unwrap();
        let file_path = "src/calc.rs";
        let initial_code = "pub fn div(a: i32, b: i32) -> i32 {\n    a / b\n}\n";
        let _ = write_file(temp.path(), file_path, initial_code).unwrap();

        let res = SurgicalRepairEngine::execute_surgical_repair(
            temp.path(),
            file_path,
            "nonexistent code",
            "replacement",
            Some("echo ok"),
        )
        .await
        .unwrap();

        assert!(!res.success);
        assert!(!res.rolled_back);
        assert!(res.patch_message.contains("Patch failed to apply"));
    }
}
