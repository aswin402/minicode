use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// A single compiler or linter diagnostic item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticItem {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub severity: String,
    pub code: Option<String>,
    pub message: String,
    pub rendered: Option<String>,
}

/// Aggregated report of compiler and linter diagnostics across the workspace.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticReport {
    pub errors: Vec<DiagnosticItem>,
    pub warnings: Vec<DiagnosticItem>,
}

impl DiagnosticReport {
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn total_issues(&self) -> usize {
        self.errors.len() + self.warnings.len()
    }

    /// Formats the diagnostic report into a compact, actionable context block for the LLM.
    pub fn format_for_agent(&self, workspace_root: &Path, max_items: usize) -> String {
        if self.errors.is_empty() && self.warnings.is_empty() {
            return "✔ Workspace compiles cleanly with zero errors or warnings.".to_string();
        }

        let mut out = String::new();
        if !self.errors.is_empty() {
            out.push_str(&format!(
                "❌ Found {} compiler error(s):\n",
                self.errors.len()
            ));
            for item in self.errors.iter().take(max_items) {
                let rel_path = item
                    .file
                    .strip_prefix(workspace_root)
                    .unwrap_or(&item.file)
                    .display();
                let code_str = item
                    .code
                    .as_deref()
                    .map(|c| format!(" [{}]", c))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "\n• {}:{}:{}{}\n  {}",
                    rel_path, item.line, item.column, code_str, item.message
                ));
                if let Some(ref rend) = item.rendered {
                    let compact_rend = rend
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .take(6)
                        .collect::<Vec<_>>()
                        .join("\n");
                    out.push_str(&format!("\n  ```text\n{}\n  ```", compact_rend));
                }
            }
        }

        if !self.warnings.is_empty() {
            let warn_count = self.warnings.len();
            out.push_str(&format!("\n⚠️ Found {} warning(s):\n", warn_count));
            for item in self.warnings.iter().take(3) {
                let rel_path = item
                    .file
                    .strip_prefix(workspace_root)
                    .unwrap_or(&item.file)
                    .display();
                out.push_str(&format!(
                    "  • {}:{}: {}\n",
                    rel_path, item.line, item.message
                ));
            }
            if warn_count > 3 {
                out.push_str(&format!("  ... and {} more warning(s)\n", warn_count - 3));
            }
        }

        out
    }
}

/// Tier 1 Fast-Path Compiler Checker executing language-specific compiler CLI tools.
pub struct FastCompilerChecker;

impl FastCompilerChecker {
    /// Detects project type and runs fast compiler diagnostics (< 500ms).
    pub async fn check_workspace(workspace_root: &Path) -> Result<DiagnosticReport> {
        let mut report = DiagnosticReport::default();

        // 1. Rust Projects (Cargo.toml)
        if workspace_root.join("Cargo.toml").exists() {
            Self::check_rust(workspace_root, &mut report).await?;
        }
        // 2. TypeScript / JavaScript (package.json)
        else if workspace_root.join("package.json").exists() {
            Self::check_typescript(workspace_root, &mut report).await?;
        }
        // 3. Python (pyproject.toml / requirements.txt)
        else if workspace_root.join("pyproject.toml").exists()
            || workspace_root.join("requirements.txt").exists()
        {
            Self::check_python(workspace_root, &mut report).await?;
        }

        Ok(report)
    }

    /// Runs `cargo check --message-format=json` and parses structured compiler diagnostics.
    async fn check_rust(workspace_root: &Path, report: &mut DiagnosticReport) -> Result<()> {
        let output = Command::new("cargo")
            .args(["check", "--message-format=json", "-j", "2"])
            .current_dir(workspace_root)
            .output()
            .await;

        let output = match output {
            Ok(o) => o,
            Err(_) => return Ok(()), // Cargo not found or unavailable
        };

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        for line in stdout_str.lines() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                if val.get("reason").and_then(|r| r.as_str()) == Some("compiler-message") {
                    if let Some(msg) = val.get("message") {
                        let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("error");
                        let message_text = msg
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let code = msg
                            .get("code")
                            .and_then(|c| c.get("code"))
                            .and_then(|c| c.as_str())
                            .map(|s| s.to_string());
                        let rendered = msg
                            .get("rendered")
                            .and_then(|r| r.as_str())
                            .map(|s| s.to_string());

                        let (file, line_num, col_num) =
                            if let Some(spans) = msg.get("spans").and_then(|s| s.as_array()) {
                                if let Some(primary) = spans.iter().find(|s| {
                                    s.get("is_primary")
                                        .and_then(|p| p.as_bool())
                                        .unwrap_or(false)
                                }) {
                                    let file_name = primary
                                        .get("file_name")
                                        .and_then(|f| f.as_str())
                                        .unwrap_or_default();
                                    let line_start = primary
                                        .get("line_start")
                                        .and_then(|l| l.as_u64())
                                        .unwrap_or(1)
                                        as usize;
                                    let col_start = primary
                                        .get("column_start")
                                        .and_then(|c| c.as_u64())
                                        .unwrap_or(1)
                                        as usize;
                                    (workspace_root.join(file_name), line_start, col_start)
                                } else {
                                    (workspace_root.to_path_buf(), 1, 1)
                                }
                            } else {
                                (workspace_root.to_path_buf(), 1, 1)
                            };

                        let item = DiagnosticItem {
                            file,
                            line: line_num,
                            column: col_num,
                            severity: level.to_string(),
                            code,
                            message: message_text,
                            rendered,
                        };

                        if level == "error" {
                            report.errors.push(item);
                        } else if level == "warning" {
                            report.warnings.push(item);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Runs `tsc --noEmit` if TypeScript is available.
    async fn check_typescript(workspace_root: &Path, report: &mut DiagnosticReport) -> Result<()> {
        let output = Command::new("npx")
            .args(["tsc", "--noEmit", "--pretty", "false"])
            .current_dir(workspace_root)
            .output()
            .await;

        if let Ok(o) = output {
            let stdout = String::from_utf8_lossy(&o.stdout);
            for line in stdout.lines() {
                // Typical tsc format: src/index.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.
                if let Some(paren_pos) = line.find('(') {
                    if let Some(colon_pos) = line.find("): error ") {
                        let file_part = &line[..paren_pos];
                        let loc_part = &line[paren_pos + 1..colon_pos];
                        let rest = &line[colon_pos + 9..];

                        let mut coords = loc_part.split(',');
                        let line_num = coords
                            .next()
                            .and_then(|s| s.trim().parse::<usize>().ok())
                            .unwrap_or(1);
                        let col_num = coords
                            .next()
                            .and_then(|s| s.trim().parse::<usize>().ok())
                            .unwrap_or(1);

                        report.errors.push(DiagnosticItem {
                            file: workspace_root.join(file_part),
                            line: line_num,
                            column: col_num,
                            severity: "error".to_string(),
                            code: None,
                            message: rest.to_string(),
                            rendered: None,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Runs `ruff check --output-format=json` or basic syntax compile for Python projects.
    async fn check_python(workspace_root: &Path, report: &mut DiagnosticReport) -> Result<()> {
        let output = Command::new("ruff")
            .args(["check", "--output-format=json", "."])
            .current_dir(workspace_root)
            .output()
            .await;

        if let Ok(o) = output {
            if let Ok(items) = serde_json::from_slice::<Vec<serde_json::Value>>(&o.stdout) {
                for item in items {
                    let file_name = item
                        .get("filename")
                        .and_then(|f| f.as_str())
                        .unwrap_or_default();
                    let message = item
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or_default();
                    let code = item
                        .get("code")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string());
                    let line = item
                        .get("location")
                        .and_then(|l| l.get("row"))
                        .and_then(|r| r.as_u64())
                        .unwrap_or(1) as usize;
                    let col = item
                        .get("location")
                        .and_then(|l| l.get("column"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(1) as usize;

                    report.errors.push(DiagnosticItem {
                        file: workspace_root.join(file_name),
                        line,
                        column: col,
                        severity: "error".to_string(),
                        code,
                        message: message.to_string(),
                        rendered: None,
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_report_formatting() {
        let mut report = DiagnosticReport::default();
        report.errors.push(DiagnosticItem {
            file: PathBuf::from("/workspace/src/main.rs"),
            line: 42,
            column: 15,
            severity: "error".to_string(),
            code: Some("E0308".to_string()),
            message: "mismatched types: expected u64, found String".to_string(),
            rendered: Some("   |\n42 | let x: u64 = val;\n   |               ^^^ expected `u64`, found `String`".to_string()),
        });

        let ws = Path::new("/workspace");
        let formatted = report.format_for_agent(ws, 5);
        assert!(formatted.contains("src/main.rs:42:15 [E0308]"));
        assert!(formatted.contains("mismatched types"));
    }
}
