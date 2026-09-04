use crate::constants::{AUTO_LINT_TIMEOUT_MS, MAX_COMPILER_DIAGNOSTICS, MAX_COMPILER_ERROR_LINES};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

/// Scoped background compiler and linter diagnostic runner.
///
/// Automatically inspects edited files and project state to provide instantaneous
/// compiler/linter feedback (Rust cargo check, Python py_compile, TypeScript tsc)
/// directly inside tool responses, enabling immediate self-correction.
pub struct ScopedCompiler;

impl ScopedCompiler {
    /// Detects and executes a scoped compiler or linter check for the modified file/project.
    ///
    /// Respects strict resource constraints (max 3 concurrent jobs) and timeouts.
    /// Returns formatted diagnostic feedback for the LLM if applicable.
    pub fn run_scoped_check(workspace_root: &Path, relative_path: &str) -> Option<String> {
        let is_rust = relative_path.ends_with(".rs") && workspace_root.join("Cargo.toml").exists();
        let is_python = relative_path.ends_with(".py");
        let is_ts = (relative_path.ends_with(".ts") || relative_path.ends_with(".tsx"))
            && workspace_root.join("tsconfig.json").exists();

        if is_rust {
            Self::check_rust(workspace_root, relative_path)
        } else if is_python {
            Self::check_python(workspace_root, relative_path)
        } else if is_ts {
            Self::check_typescript(workspace_root, relative_path)
        } else {
            None
        }
    }

    /// Runs a scoped `cargo check -j 3 --message-format=short` inside the Rust project.
    pub fn check_rust(workspace_root: &Path, relative_path: &str) -> Option<String> {
        let mut cmd = Command::new("cargo");
        cmd.args(["check", "-j", "3", "--message-format=short"]);
        cmd.current_dir(workspace_root);

        let output = Self::run_with_timeout(cmd, Duration::from_millis(AUTO_LINT_TIMEOUT_MS))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        // Check if clean
        if output.status.success() {
            let has_errors = combined
                .lines()
                .any(|l| l.contains("error[") || l.contains("error:"));
            if !has_errors {
                return Some(
                    "[Compiler Status]: ✓ cargo check passed cleanly (0 errors).".to_string(),
                );
            }
        }

        // Collect and prioritize errors
        let mut error_lines = Vec::new();
        let mut relevant_errors = Vec::new();
        let mut other_errors = Vec::new();

        for line in combined.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("error[") || trimmed.starts_with("error:") {
                if trimmed.contains(relative_path) {
                    relevant_errors.push(trimmed.to_string());
                } else {
                    other_errors.push(trimmed.to_string());
                }
            } else if (trimmed.starts_with("-->") || trimmed.starts_with('|'))
                && (!relevant_errors.is_empty() || !other_errors.is_empty())
            {
                if let Some(last) = relevant_errors.last_mut() {
                    if last.lines().count() < MAX_COMPILER_ERROR_LINES {
                        last.push('\n');
                        last.push_str(trimmed);
                    }
                }
            }
        }

        error_lines.extend(relevant_errors);
        if error_lines.len() < MAX_COMPILER_DIAGNOSTICS {
            let remaining = MAX_COMPILER_DIAGNOSTICS - error_lines.len();
            error_lines.extend(other_errors.into_iter().take(remaining));
        }

        if error_lines.is_empty() {
            return None;
        }

        let mut diag = String::from("[Compiler Feedback]:\n⚠️ cargo check reported errors:\n");
        for (idx, err) in error_lines
            .iter()
            .take(MAX_COMPILER_DIAGNOSTICS)
            .enumerate()
        {
            if idx > 0 {
                diag.push('\n');
            }
            diag.push_str(err);
        }

        Some(diag)
    }

    /// Runs `python3 -m py_compile <path>` to check for Python syntax errors.
    pub fn check_python(workspace_root: &Path, relative_path: &str) -> Option<String> {
        let full_path = workspace_root.join(relative_path);
        let path_str = full_path.to_str()?;
        let mut cmd = Command::new("python3");
        cmd.args(["-m", "py_compile", path_str]);
        cmd.current_dir(workspace_root);

        let output = Self::run_with_timeout(cmd, Duration::from_millis(2000))?;

        if output.status.success() {
            Some("[Linter Feedback]: ✓ py_compile passed cleanly.".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let first_lines = stderr.lines().take(4).collect::<Vec<_>>().join("\n");
            Some(format!(
                "[Linter Feedback]:\n⚠️ py_compile reported syntax error:\n{}",
                first_lines
            ))
        }
    }

    /// Runs `npx tsc --noEmit --pretty false` to check TypeScript compilation.
    pub fn check_typescript(workspace_root: &Path, relative_path: &str) -> Option<String> {
        let mut cmd = Command::new("npx");
        cmd.args(["tsc", "--noEmit", "--pretty", "false"]);
        cmd.current_dir(workspace_root);

        let output = Self::run_with_timeout(cmd, Duration::from_millis(AUTO_LINT_TIMEOUT_MS))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        if output.status.success() {
            Some("[Compiler Status]: ✓ tsc passed cleanly (0 errors).".to_string())
        } else {
            let mut relevant_errors = Vec::new();
            let mut other_errors = Vec::new();
            for line in combined.lines() {
                if line.contains("error TS") {
                    if line.contains(relative_path) {
                        relevant_errors.push(line.trim().to_string());
                    } else {
                        other_errors.push(line.trim().to_string());
                    }
                }
            }
            let mut errors = relevant_errors;
            if errors.len() < MAX_COMPILER_DIAGNOSTICS {
                let remaining = MAX_COMPILER_DIAGNOSTICS - errors.len();
                errors.extend(other_errors.into_iter().take(remaining));
            }
            if errors.is_empty() {
                return None;
            }
            Some(format!(
                "[Compiler Feedback]:\n⚠️ TypeScript compiler reported errors:\n{}",
                errors
                    .iter()
                    .take(MAX_COMPILER_DIAGNOSTICS)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        }
    }

    /// Helper to execute a command with a timeout without blocking indefinitely.
    fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Option<std::process::Output> {
        let (tx, rx) = mpsc::channel();
        let child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .ok()?;

        std::thread::spawn(move || {
            let res = child.wait_with_output();
            let _ = tx.send(res);
        });

        match rx.recv_timeout(timeout) {
            Ok(Ok(output)) => Some(output),
            _ => None, // Timed out or child process error
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_scoped_compiler_skips_non_project_dirs() {
        let temp = tempdir().unwrap();
        // Temp dir with no Cargo.toml, tsconfig.json, or python file
        let res = ScopedCompiler::run_scoped_check(temp.path(), "readme.txt");
        assert!(res.is_none());
    }

    #[test]
    fn test_scoped_compiler_rust_detection() {
        let temp = tempdir().unwrap();
        // If Cargo.toml does not exist, check_rust is not run by run_scoped_check
        let res = ScopedCompiler::run_scoped_check(temp.path(), "src/main.rs");
        assert!(res.is_none());
    }
}
