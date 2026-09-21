use crate::error::{Result, ToolError};
use crate::tools::onpkg::scaffolder::OnpkgScaffolder;
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::Path;

/// Result of evaluating workspace files against the canonical stack template.
#[derive(Debug, Clone)]
pub struct StackDiffResult {
    pub stack_name: String,
    pub missing_files: Vec<String>,
    pub modified_files: Vec<(String, String)>,
    pub intact_files_count: usize,
    pub total_files_count: usize,
    pub restored_files: Vec<String>,
}

impl StackDiffResult {
    /// Formats the diff result into a human-readable or agent-actionable report.
    pub fn format_report(&self) -> String {
        let mut out = format!(
            "🏗️ **Stack Architecture Integrity Report: `{}`**\n\
             • Total Files: {} | Intact: {} | Modified: {} | Missing: {}\n\n",
            self.stack_name,
            self.total_files_count,
            self.intact_files_count,
            self.modified_files.len(),
            self.missing_files.len()
        );

        if !self.restored_files.is_empty() {
            out.push_str("✨ **Restored Missing Files (--apply active):**\n");
            for r in &self.restored_files {
                out.push_str(&format!("  ✔ Re-scaffolded: `{}`\n", r));
            }
            out.push('\n');
        }

        if !self.missing_files.is_empty() {
            out.push_str("⚠️ **Missing Architecture Files:**\n");
            for m in &self.missing_files {
                out.push_str(&format!(
                    "  ✗ `{}` — missing from workspace. Run with `--apply` to self-heal.\n",
                    m
                ));
            }
            out.push('\n');
        }

        if !self.modified_files.is_empty() {
            out.push_str("📝 **Modified Files (Changes from Template):**\n");
            for (path, diff) in &self.modified_files {
                out.push_str(&format!("  • **`{}`**:\n```diff\n{}\n```\n", path, diff));
            }
        }

        if self.missing_files.is_empty() && self.modified_files.is_empty() {
            out.push_str(
                "✨ **Workspace is 100% synchronized with stack template!** Zero drift detected.\n",
            );
        }

        out
    }
}

/// Evaluates project drift against its onpkg stack template and optionally heals missing files.
pub fn diff_stack(
    workspace_root: &Path,
    stack_name_opt: Option<&str>,
    apply: bool,
) -> Result<StackDiffResult> {
    let stack_name = match stack_name_opt {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => {
            let manifest_path = workspace_root.join(crate::constants::ONPKG_MANIFEST_FILE);
            if !manifest_path.exists() {
                return Err(ToolError::InvalidArguments {
                    name: "onpkg_stack_diff".to_string(),
                    reason: "No stack name provided and onpkg.json not found in workspace."
                        .to_string(),
                }
                .into());
            }

            let content = fs::read_to_string(&manifest_path).map_err(|e| ToolError::FileOp {
                path: manifest_path.display().to_string(),
                source: e,
            })?;
            let val: serde_json::Value =
                serde_json::from_str(&content).map_err(|e| ToolError::InvalidArguments {
                    name: "onpkg_stack_diff".to_string(),
                    reason: format!("Failed to parse onpkg.json: {}", e),
                })?;

            val.get("stack")
                .and_then(|s| s.as_str())
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "onpkg_stack_diff".to_string(),
                    reason: "Field 'stack' missing from onpkg.json".to_string(),
                })?
                .to_string()
        }
    };

    let stack = OnpkgScaffolder::find_stack(&stack_name).ok_or_else(|| {
        let available: Vec<String> = OnpkgScaffolder::get_all_stacks()
            .into_iter()
            .map(|s| s.name)
            .collect();
        ToolError::InvalidArguments {
            name: "onpkg_stack_diff".to_string(),
            reason: format!(
                "Stack `{}` not found in catalog. Available stacks: {}",
                stack_name,
                available.join(", ")
            ),
        }
    })?;

    let mut missing_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut restored_files = Vec::new();
    let mut intact_files_count = 0;
    let total_files_count = stack.files.len();

    for file in &stack.files {
        let target_path = workspace_root.join(&file.path);

        if !target_path.exists() {
            missing_files.push(file.path.clone());
            if apply {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                if let Some(bin) = &file.binary_content {
                    if fs::write(&target_path, bin).is_ok() {
                        restored_files.push(file.path.clone());
                    }
                } else if fs::write(&target_path, &file.content).is_ok() {
                    restored_files.push(file.path.clone());
                }
            }
            continue;
        }

        // Check binary file
        if let Some(expected_bin) = &file.binary_content {
            if let Ok(current_bytes) = fs::read(&target_path) {
                if current_bytes != *expected_bin {
                    modified_files.push((
                        file.path.clone(),
                        "(Binary file differs from template)".to_string(),
                    ));
                } else {
                    intact_files_count += 1;
                }
            }
            continue;
        }

        // Check text file
        if let Ok(current_text) = fs::read_to_string(&target_path) {
            if current_text == file.content {
                intact_files_count += 1;
            } else {
                // Generate unified diff
                let text_diff = TextDiff::from_lines(&file.content, &current_text);
                let mut diff_str = String::new();
                for change in text_diff.iter_all_changes().take(30) {
                    let sign = match change.tag() {
                        ChangeTag::Delete => "-",
                        ChangeTag::Insert => "+",
                        ChangeTag::Equal => " ",
                    };
                    diff_str.push_str(&format!("{}{}", sign, change));
                }
                modified_files.push((file.path.clone(), diff_str.trim_end().to_string()));
            }
        } else {
            missing_files.push(file.path.clone());
        }
    }

    Ok(StackDiffResult {
        stack_name,
        missing_files,
        modified_files,
        intact_files_count,
        total_files_count,
        restored_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_diff_stack_detects_missing_and_apply() {
        let temp = TempDir::new().unwrap();

        // 1. Initially missing all files
        let res = diff_stack(temp.path(), Some("hono-api"), false).unwrap();
        assert!(!res.missing_files.is_empty());
        assert_eq!(res.restored_files.len(), 0);

        // 2. With apply = true, missing files are restored
        let res_apply = diff_stack(temp.path(), Some("hono-api"), true).unwrap();
        assert_eq!(res_apply.restored_files.len(), 2);
        assert!(temp.path().join("src/server.ts").exists());

        // 3. Re-running without apply now shows intact
        let res_after = diff_stack(temp.path(), Some("hono-api"), false).unwrap();
        assert_eq!(res_after.intact_files_count, 2);
        assert_eq!(res_after.missing_files.len(), 0);
    }
}
