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

struct CompilerCheckRule {
    extensions: &'static [&'static str],
    required_marker: Option<&'static str>,
    runner: fn(&Path, &str) -> Option<String>,
}

const COMPILER_CHECK_RULES: &[CompilerCheckRule] = &[
    CompilerCheckRule {
        extensions: &[".rs"],
        required_marker: Some("Cargo.toml"),
        runner: ScopedCompiler::check_rust,
    },
    CompilerCheckRule {
        extensions: &[".py"],
        required_marker: None,
        runner: ScopedCompiler::check_python,
    },
    CompilerCheckRule {
        extensions: &[".ts", ".tsx"],
        required_marker: Some("tsconfig.json"),
        runner: ScopedCompiler::check_typescript,
    },
    CompilerCheckRule {
        extensions: &[".go"],
        required_marker: Some("go.mod"),
        runner: ScopedCompiler::check_go,
    },
];

impl ScopedCompiler {
    /// Detects and executes a scoped compiler or linter check for the modified file/project
    /// using the declarative `COMPILER_CHECK_RULES` table (Rust, Python, TypeScript, Go).
    pub fn run_scoped_check(workspace_root: &Path, relative_path: &str) -> Option<String> {
        let lower = relative_path.to_ascii_lowercase();
        let rule = COMPILER_CHECK_RULES.iter().find(|r| {
            r.extensions.iter().any(|ext| lower.ends_with(ext))
                && r.required_marker
                    .is_none_or(|marker| workspace_root.join(marker).exists())
        })?;
        (rule.runner)(workspace_root, relative_path)
    }

    /// Unified post-edit diagnostic hook combining scoped compiler checks (`cargo check`, `tsc`,
    /// `py_compile`, `go vet`) with ultra-fast (<15ms) single-file native AST `minidoctor` guards.
    /// Returns a formatted diagnostic block (prefixed with `\n\n`) or an empty string if clean/n/a.
    pub fn run_post_edit_diagnostics(
        workspace_root: &Path,
        relative_path: &str,
        content: &str,
    ) -> String {
        let mut out = String::new();
        if let Some(feedback) = Self::run_scoped_check(workspace_root, relative_path) {
            out.push_str("\n\n");
            out.push_str(&feedback);
        }

        // Smart Auto-Guard: ultra-fast (<15ms) single-file native AST Critical/Error micro-guard
        let doctor_hits: Vec<minidoctor::Diagnostic> =
            minidoctor::check_single_file(workspace_root, relative_path, content)
                .into_iter()
                .filter(|d| {
                    matches!(
                        d.severity,
                        minidoctor::Severity::Critical | minidoctor::Severity::Error
                    )
                })
                .take(4)
                .collect();
        if !doctor_hits.is_empty() {
            out.push_str("\n\n[MiniDoctor Auto-Guard]:");
            for d in doctor_hits {
                out.push_str(&format!(
                    "\n  - [{}] `{}` at {}:{} — {} (Fix: {})",
                    d.severity.badge(),
                    d.rule,
                    d.file,
                    d.line,
                    d.message,
                    d.suggestion
                ));
            }
        }
        out
    }

    /// Runs a scoped `cargo check -j 1 --message-format=short` inside the Rust project.
    pub fn check_rust(workspace_root: &Path, relative_path: &str) -> Option<String> {
        let mut cmd = Command::new("cargo");
        cmd.args(["check", "-j", "1", "--message-format=short"]);
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

    /// Runs `tsc --noEmit --pretty false` (preferring local `node_modules/.bin/tsc` over `npx`) to check TypeScript compilation.
    pub fn check_typescript(workspace_root: &Path, relative_path: &str) -> Option<String> {
        let local_tsc = if cfg!(windows) {
            workspace_root.join("node_modules/.bin/tsc.cmd")
        } else {
            workspace_root.join("node_modules/.bin/tsc")
        };
        let mut cmd = if local_tsc.is_file() {
            let mut c = Command::new(&local_tsc);
            c.args(["--noEmit", "--pretty", "false"]);
            c
        } else {
            let mut c = Command::new("npx");
            c.args(["tsc", "--noEmit", "--pretty", "false"]);
            c
        };
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
            let has_missing_export = errors
                .iter()
                .any(|e| e.contains("TS2305") || e.contains("TS2724"));
            let missing_export_tip = if has_missing_export {
                "\n💡 Missing Export Tip: If an imported icon or symbol is not exported by the installed package (e.g. brand/social icons or renamed symbols), either:\n  1. Create a clean custom inline `<svg viewBox=\"0 0 24 24\" ...>` component directly in code,\n  2. Use an existing exported icon/symbol from the package, or\n  3. Install/use a dedicated icon library (such as `@phosphor-icons/react`, `@tabler/icons-react`, `@radix-ui/react-icons`, or `simple-icons`)."
            } else {
                ""
            };
            Some(format!(
                "[Compiler Feedback]:\n⚠️ TypeScript compiler reported errors:\n{}{}",
                errors
                    .iter()
                    .take(MAX_COMPILER_DIAGNOSTICS)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n"),
                missing_export_tip
            ))
        }
    }

    /// Runs `go vet` on the package containing `relative_path` inside a Go module.
    pub fn check_go(workspace_root: &Path, relative_path: &str) -> Option<String> {
        let pkg_target = Path::new(relative_path)
            .parent()
            .and_then(|p| p.to_str())
            .filter(|s| !s.is_empty() && *s != ".")
            .map(|dir| format!("./{dir}"))
            .unwrap_or_else(|| "./...".to_string());

        let mut cmd = Command::new("go");
        cmd.args(["vet", &pkg_target]);
        cmd.current_dir(workspace_root);

        let output = Self::run_with_timeout(cmd, Duration::from_millis(AUTO_LINT_TIMEOUT_MS))?;
        if output.status.success() {
            return Some("[Compiler Status]: ✓ go vet passed cleanly (0 errors).".to_string());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{stdout}\n{stderr}");
        let errs: Vec<&str> = combined
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .take(MAX_COMPILER_DIAGNOSTICS)
            .collect();
        if errs.is_empty() {
            return None;
        }
        Some(format!(
            "[Compiler Feedback]:\n⚠️ go vet reported errors:\n{}",
            errs.join("\n")
        ))
    }

    /// Helper to execute a command with a timeout without blocking indefinitely or leaking orphan processes.
    fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Option<std::process::Output> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let (tx, rx) = mpsc::channel();
        let child = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .ok()?;
        let child_pid = child.id();

        std::thread::spawn(move || {
            let res = child.wait_with_output();
            let _ = tx.send(res);
        });

        match rx.recv_timeout(timeout) {
            Ok(Ok(output)) => Some(output),
            _ => {
                #[cfg(unix)]
                if child_pid > 0 {
                    let pid_i32 = child_pid as i32;
                    unsafe {
                        let _ = libc::kill(-pid_i32, libc::SIGKILL);
                        let _ = libc::kill(pid_i32, libc::SIGKILL);
                    }
                }
                None
            }
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
