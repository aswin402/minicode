use crate::error::{Result, ToolError};
use crate::tools::minikit::scaffolder::MiniKitScaffolder;
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::Path;

/// Result of evaluating workspace files against the canonical stack template.
#[derive(Debug, Clone)]
pub struct StackDiffResult {
    pub stack_name: String,
    pub missing_files: Vec<String>,
    pub modified_files: Vec<(String, String)>,
    pub ignored_files: Vec<String>,
    pub intact_files_count: usize,
    pub total_files_count: usize,
    pub restored_files: Vec<String>,
}

impl StackDiffResult {
    /// Formats the diff result into a human-readable or agent-actionable report.
    pub fn format_report(&self) -> String {
        let mut out = format!(
            "🏗️ **Stack Architecture Integrity Report: `{}`**\n\
             • Total Files: {} | Intact: {} | Modified: {} | Missing: {} | Ignored: {}\n\n",
            self.stack_name,
            self.total_files_count,
            self.intact_files_count,
            self.modified_files.len(),
            self.missing_files.len(),
            self.ignored_files.len()
        );

        if !self.ignored_files.is_empty() {
            out.push_str("🛡️ **Ignored Files (.minikitignore):**\n");
            for ign in &self.ignored_files {
                out.push_str(&format!("  • `{}` (customization protected)\n", ign));
            }
            out.push('\n');
        }

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

/// Helper for loading and evaluating .minikitignore / .onpkgignore file patterns.
#[derive(Debug, Clone, Default)]
pub struct MiniKitIgnore {
    pub patterns: Vec<String>,
}

impl MiniKitIgnore {
    pub fn load_from_workspace(workspace_root: &Path) -> Self {
        let mut patterns = Vec::new();
        for ignore_name in &[
            crate::constants::MINIKIT_IGNORE_FILE,
            crate::constants::ONPKG_IGNORE_FILE,
        ] {
            let path = workspace_root.join(ignore_name);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && !trimmed.starts_with('#') {
                            patterns.push(trimmed.trim_start_matches("./").to_string());
                        }
                    }
                    break;
                }
            }
        }
        Self { patterns }
    }

    pub fn is_ignored(&self, relative_path: &str) -> bool {
        let norm = relative_path.trim_start_matches("./").replace('\\', "/");
        for pat in &self.patterns {
            let pat_norm = pat.trim_start_matches("./").replace('\\', "/");
            if norm == pat_norm {
                return true;
            }
            if pat_norm.ends_with("/*") {
                let prefix = pat_norm.trim_end_matches("/*");
                if norm.starts_with(prefix) {
                    return true;
                }
            } else if pat_norm.ends_with('/') {
                if norm.starts_with(&pat_norm) {
                    return true;
                }
            } else if pat_norm.starts_with("*.") {
                let ext = pat_norm.trim_start_matches('*');
                if norm.ends_with(ext) {
                    return true;
                }
            }
        }
        false
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
            let manifest_path = match super::resolve_manifest_path(workspace_root) {
                Some(p) => p,
                None => {
                    return Err(ToolError::InvalidArguments {
                        name: "kit_stack_diff".to_string(),
                        reason: "No stack name provided and minikit.json / minicode.json / onpkg.json not found in workspace."
                            .to_string(),
                    }
                    .into());
                }
            };

            let content = fs::read_to_string(&manifest_path).map_err(|e| ToolError::FileOp {
                path: manifest_path.display().to_string(),
                source: e,
            })?;
            let val: serde_json::Value =
                serde_json::from_str(&content).map_err(|e| ToolError::InvalidArguments {
                    name: "kit_stack_diff".to_string(),
                    reason: format!(
                        "Failed to parse manifest ({}): {}",
                        manifest_path.display(),
                        e
                    ),
                })?;

            val.get("stack")
                .and_then(|s| s.as_str())
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "kit_stack_diff".to_string(),
                    reason: format!(
                        "Field 'stack' missing from manifest ({})",
                        manifest_path.display()
                    ),
                })?
                .to_string()
        }
    };

    let stack = MiniKitScaffolder::find_stack_in_workspace(workspace_root, &stack_name)
        .ok_or_else(|| {
            let available: Vec<String> = MiniKitScaffolder::get_all_stacks()
                .into_iter()
                .map(|s| s.name)
                .collect();
            ToolError::InvalidArguments {
                name: "kit_stack_diff".to_string(),
                reason: format!(
                    "Stack `{}` not found in catalog. Available stacks: {}",
                    stack_name,
                    available.join(", ")
                ),
            }
        })?;

    let ignore = MiniKitIgnore::load_from_workspace(workspace_root);
    let mut missing_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut restored_files = Vec::new();
    let mut ignored_files = Vec::new();
    let mut intact_files_count = 0;
    let total_files_count = stack.files.len();

    for file in &stack.files {
        if ignore.is_ignored(&file.path) {
            ignored_files.push(file.path.clone());
            continue;
        }

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

        let meta = match fs::metadata(&target_path) {
            Ok(m) => m,
            Err(_) => {
                missing_files.push(file.path.clone());
                continue;
            }
        };

        // Check binary file
        if let Some(expected_bin) = &file.binary_content {
            if meta.len() != expected_bin.len() as u64 {
                modified_files.push((
                    file.path.clone(),
                    "(Binary file differs from template: size mismatch)".to_string(),
                ));
            } else if let Ok(current_bytes) = fs::read(&target_path) {
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

        // Check text file (fast length check + content diff)
        let expected_len = file.content.len() as u64;
        if meta.len() != expected_len {
            if let Ok(current_text) = fs::read_to_string(&target_path) {
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
            } else {
                missing_files.push(file.path.clone());
            }
        } else if let Ok(current_text) = fs::read_to_string(&target_path) {
            if current_text == file.content {
                intact_files_count += 1;
            } else {
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
        ignored_files,
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

    #[test]
    fn test_minikitignore_protects_custom_files_from_drift_and_overwrite() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        // 1. Scaffold hono-api files into temp workspace
        diff_stack(ws, Some("hono-api"), true).unwrap();
        assert!(ws.join("src/server.ts").exists());

        // 2. Modify src/server.ts with custom user logic
        fs::write(
            ws.join("src/server.ts"),
            "// Custom modified server code\nconsole.log('custom');\n",
        )
        .unwrap();

        // 3. Without .minikitignore, diff detects modification
        let diff_unignored = diff_stack(ws, Some("hono-api"), false).unwrap();
        assert_eq!(diff_unignored.modified_files.len(), 1);
        assert_eq!(diff_unignored.ignored_files.len(), 0);

        // 4. Create .minikitignore ignoring src/server.ts
        fs::write(
            ws.join(".minikitignore"),
            "# Ignore custom server implementation\nsrc/server.ts\n",
        )
        .unwrap();

        // 5. With .minikitignore, it is marked as ignored and NOT in modified_files
        let diff_ignored = diff_stack(ws, Some("hono-api"), false).unwrap();
        assert_eq!(diff_ignored.modified_files.len(), 0);
        assert_eq!(diff_ignored.ignored_files.len(), 1);
        assert_eq!(diff_ignored.ignored_files[0], "src/server.ts");

        // 6. Running with apply = true does NOT touch the custom file
        diff_stack(ws, Some("hono-api"), true).unwrap();
        let content_after = fs::read_to_string(ws.join("src/server.ts")).unwrap();
        assert!(content_after.contains("Custom modified server code"));

        // 7. Verify report output includes shield and ignored section
        let report = diff_ignored.format_report();
        assert!(report.contains("🛡️ **Ignored Files (.minikitignore):**"));
        assert!(report.contains("`src/server.ts` (customization protected)"));
    }

    #[test]
    fn test_minikitignore_glob_patterns() {
        let ignore = MiniKitIgnore {
            patterns: vec![
                "README.md".to_string(),
                "docs/*".to_string(),
                "src/custom/".to_string(),
                "*.local.json".to_string(),
            ],
        };

        assert!(ignore.is_ignored("README.md"));
        assert!(ignore.is_ignored("./README.md"));
        assert!(ignore.is_ignored("docs/guide.md"));
        assert!(ignore.is_ignored("src/custom/handler.ts"));
        assert!(ignore.is_ignored("config.local.json"));

        assert!(!ignore.is_ignored("src/server.ts"));
        assert!(!ignore.is_ignored("package.json"));
    }
}
