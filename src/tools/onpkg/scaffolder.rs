use crate::error::{Result, ToolError};
use crate::tools::onpkg::stacks::{builtin::builtin_stacks, Stack};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Native engine for scaffolding application stacks, generating manifests, and auto-installing packages.
pub struct OnpkgScaffolder;

impl OnpkgScaffolder {
    /// Returns all natively embedded built-in stacks plus any custom workspace or user stacks.
    pub fn get_all_stacks() -> Vec<Stack> {
        let mut stacks = builtin_stacks();

        let mut search_dirs = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            search_dirs.push(cwd.join(".minicode").join("stacks"));
            search_dirs.push(cwd.join(".minikit").join("stacks"));
        }
        if let Some(home) = dirs::home_dir() {
            search_dirs.push(home.join(".config").join("minicode").join("stacks"));
            search_dirs.push(home.join(".minikit").join("stacks"));
            search_dirs.push(home.join(".onpkg").join("stacks"));
        }

        for dir in search_dirs {
            if !dir.exists() || !dir.is_dir() {
                continue;
            }
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().is_some_and(|e| e == "json") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(stack) = serde_json::from_str::<Stack>(&content) {
                                if !stacks
                                    .iter()
                                    .any(|s| s.name.eq_ignore_ascii_case(&stack.name))
                                {
                                    stacks.push(stack);
                                }
                            }
                        }
                    }
                }
            }
        }

        stacks
    }

    /// Finds a stack by name across built-in and custom templates.
    pub fn find_stack(name: &str) -> Option<Stack> {
        let norm = name.trim().to_lowercase();
        Self::get_all_stacks()
            .into_iter()
            .find(|s| s.name.to_lowercase() == norm)
    }

    /// Creates a starter custom stack template JSON in `.minicode/stacks/<name>.json` (or globally in `~/.config/minicode/stacks/<name>.json`).
    pub fn create_custom_stack(
        workspace_root: &Path,
        name: &str,
        runtime: &str,
        global: bool,
    ) -> Result<PathBuf> {
        let norm = name.trim().to_lowercase();
        let target_dir = if global {
            let home = dirs::home_dir().ok_or_else(|| ToolError::InvalidArguments {
                name: "kit_stack_new".to_string(),
                reason: "Could not determine home directory".to_string(),
            })?;
            home.join(".config").join("minicode").join("stacks")
        } else {
            workspace_root.join(".minicode").join("stacks")
        };

        fs::create_dir_all(&target_dir).map_err(|e| ToolError::FileOp {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        let file_path = target_dir.join(format!("{}.json", norm));
        if file_path.exists() {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_new".to_string(),
                reason: format!(
                    "Custom stack template `{}` already exists at `{}`",
                    norm,
                    file_path.display()
                ),
            }
            .into());
        }

        let starter_stack = Stack {
            name: norm.clone(),
            runtime: runtime.trim().to_lowercase(),
            description: format!("Custom {} stack template for {}", runtime, norm),
            packages: vec![],
            dev_packages: vec![],
            transitive_packages: vec![],
            files: vec![crate::tools::onpkg::stacks::StackFile {
                path: "README.md".to_string(),
                content: format!(
                    "# {}\n\nCustom architecture stack created with minicode.\n",
                    norm
                ),
                binary_content: None,
            }],
            hooks: vec![],
        };

        let json = serde_json::to_string_pretty(&starter_stack).map_err(|e| {
            ToolError::InvalidArguments {
                name: "kit_stack_new".to_string(),
                reason: format!("Failed to serialize stack JSON: {}", e),
            }
        })?;

        fs::write(&file_path, json).map_err(|e| ToolError::FileOp {
            path: file_path.display().to_string(),
            source: e,
        })?;

        Ok(file_path)
    }

    /// Deletes a custom stack template from `.minicode/stacks/<name>.json` (or globally `~/.config/minicode/stacks/`).
    pub fn delete_custom_stack(
        workspace_root: &Path,
        stack_name: &str,
        global: bool,
    ) -> Result<String> {
        let norm = stack_name.trim().to_lowercase();
        if norm.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_remove".to_string(),
                reason: "Stack name cannot be empty".to_string(),
            }
            .into());
        }

        let file_path = if global {
            if let Some(home) = dirs::home_dir() {
                home.join(".config")
                    .join("minicode")
                    .join("stacks")
                    .join(format!("{}.json", norm))
            } else {
                return Err(ToolError::InvalidArguments {
                    name: "kit_stack_remove".to_string(),
                    reason: "Home directory could not be determined".to_string(),
                }
                .into());
            }
        } else {
            workspace_root
                .join(".minicode")
                .join("stacks")
                .join(format!("{}.json", norm))
        };

        if file_path.exists() {
            fs::remove_file(&file_path).map_err(|e| ToolError::FileOp {
                path: file_path.display().to_string(),
                source: e,
            })?;
            Ok(format!(
                "✔ Successfully removed custom stack template `{}` at `{}`",
                norm,
                file_path.display()
            ))
        } else {
            Err(ToolError::InvalidArguments {
                name: "kit_stack_remove".to_string(),
                reason: format!(
                    "Custom stack template `{}` not found at `{}`",
                    norm,
                    file_path.display()
                ),
            }
            .into())
        }
    }

    /// Finds a stack by name, prioritizing workspace-specific stacks in `.minicode/stacks`.
    pub fn find_stack_in_workspace(workspace_root: &Path, name: &str) -> Option<Stack> {
        let norm = name.trim().to_lowercase();
        for dir_name in &[".minicode", ".minikit"] {
            let candidate = workspace_root
                .join(dir_name)
                .join("stacks")
                .join(format!("{}.json", norm));
            if candidate.exists() {
                if let Ok(content) = fs::read_to_string(&candidate) {
                    if let Ok(stack) = serde_json::from_str::<Stack>(&content) {
                        return Some(stack);
                    }
                }
            }
        }
        Self::find_stack(name)
    }

    /// Scaffolds a stack into `target_dir`.
    pub async fn scaffold(
        workspace_root: &Path,
        stack_name: &str,
        target_dir_opt: Option<&str>,
        no_install: bool,
    ) -> Result<String> {
        let stack = Self::find_stack_in_workspace(workspace_root, stack_name).ok_or_else(|| {
            let available: Vec<String> =
                Self::get_all_stacks().into_iter().map(|s| s.name).collect();
            ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: format!(
                    "Stack `{}` not found. Available built-in and custom stacks: {}",
                    stack_name,
                    available.join(", ")
                ),
            }
        })?;

        let raw_dest: PathBuf = match target_dir_opt {
            Some(rel) if !rel.trim().is_empty() => {
                let p = Path::new(rel.trim());
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    workspace_root.join(p)
                }
            }
            _ => workspace_root.to_path_buf(),
        };
        let dest_dir = crate::sandbox::path::validate_path_in_workspace(workspace_root, &raw_dest)?;

        fs::create_dir_all(&dest_dir).map_err(|e| ToolError::FileOp {
            path: dest_dir.display().to_string(),
            source: e,
        })?;

        let files_count = stack.files.len();

        // 1. Write all template files
        for f in &stack.files {
            let file_path = dest_dir.join(&f.path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
                    path: parent.display().to_string(),
                    source: e,
                })?;
            }

            if let Some(bin) = &f.binary_content {
                fs::write(&file_path, bin).map_err(|e| ToolError::FileOp {
                    path: file_path.display().to_string(),
                    source: e,
                })?;
            } else {
                fs::write(&file_path, &f.content).map_err(|e| ToolError::FileOp {
                    path: file_path.display().to_string(),
                    source: e,
                })?;
            }
        }

        // 2. Generate onpkg.json manifest
        let project_name = dest_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app")
            .to_string();

        let manifest = serde_json::json!({
            "name": project_name,
            "version": "0.1.0",
            "runtime": stack.runtime,
            "package_manager": stack.runtime,
            "stack": stack.name,
            "description": stack.description,
            "packages": stack.packages,
            "dev_packages": stack.dev_packages,
            "active_skills": [stack.runtime, stack.name]
        });

        let manifest_path = dest_dir.join(crate::constants::MINIKIT_MANIFEST_FILE);
        let manifest_json = serde_json::to_string_pretty(&manifest).unwrap_or_default();
        fs::write(&manifest_path, &manifest_json).ok();
        fs::write(
            dest_dir.join(crate::constants::ONPKG_MANIFEST_FILE),
            &manifest_json,
        )
        .ok();

        // 3. Generate AGENTS.md instructions
        let agents_md = format!(
            "# {} — Agent Guidelines & Repository Instructions 🧠\n\n\
            > Scaffolded with `minicode` + `MiniKit` native stack engine.\n\n\
            ## Project Summary\n\
            - **Name:** `{}`\n\
            - **Stack:** `{}`\n\
            - **Runtime / Package Manager:** `{}`\n\
            - **Description:** {}\n\n\
            ## Architecture & Conventions\n\
            1. All project specifications and task tracking live under `minikit_docs/`.\n\
            2. Use `{}` as the package manager.\n\
            3. Follow standard {} best practices.\n",
            project_name,
            project_name,
            stack.name,
            stack.runtime,
            stack.description,
            stack.runtime,
            stack.runtime
        );
        fs::write(dest_dir.join("AGENTS.md"), agents_md).ok();

        // 4. Generate initial workflow docs under minikit_docs and onpkg_docs
        let docs_dir = dest_dir.join(crate::constants::MINIKIT_DOCS_DIR);
        super::sync::OnpkgSyncEngine::ensure_workflow_docs(
            &docs_dir,
            &project_name,
            &stack.runtime,
        );
        let onpkg_docs = dest_dir.join(crate::constants::ONPKG_DOCS_DIR);
        super::sync::OnpkgSyncEngine::ensure_workflow_docs(
            &onpkg_docs,
            &project_name,
            &stack.runtime,
        );

        // 5. Post-scaffold install hooks
        let mut install_msg = String::new();
        if !no_install {
            install_msg = Self::run_package_installer(&stack.runtime, &dest_dir);
        }

        Ok(format!(
            "✔ Successfully scaffolded stack `{}` in `{}`\n\
            • Files created: {} files\n\
            • Manifest: minikit.json, AGENTS.md, minikit_docs/\n\
            • Runtime: {}\n{}",
            stack.name,
            dest_dir.display(),
            files_count,
            stack.runtime,
            install_msg
        ))
    }

    /// Automatically runs the best package installer for the runtime.
    fn run_package_installer(runtime: &str, dest_dir: &Path) -> String {
        let (cmd, args) = match runtime {
            "bun" => ("bun", vec!["install"]),
            "uv" => ("uv", vec!["sync"]),
            "cargo" => ("cargo", vec!["check"]),
            "flutter" => ("flutter", vec!["pub", "get"]),
            "npm" => ("npm", vec!["install"]),
            "pnpm" => ("pnpm", vec!["install"]),
            "yarn" => ("yarn", vec!["install"]),
            _ => return "\nℹ Skipped auto-install (unknown runtime)".to_string(),
        };

        match Command::new(cmd).args(&args).current_dir(dest_dir).output() {
            Ok(output) if output.status.success() => {
                format!("• Package install: ✔ `{}` completed successfully.", cmd)
            }
            Ok(output) => {
                let err = String::from_utf8_lossy(&output.stderr);
                format!(
                    "• Package install: ⚠ `{} {}` exited with error: {}",
                    cmd,
                    args.join(" "),
                    err.lines().next().unwrap_or("failed")
                )
            }
            Err(_) => {
                format!(
                    "• Package install: ℹ `{}` CLI not found on system. Run `{} {}` manually.",
                    cmd,
                    cmd,
                    args.join(" ")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_custom_workspace_stack_scaffolding() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        // 1. Create a custom stack template in .minicode/stacks/my-custom.json
        let stacks_dir = ws.join(".minicode").join("stacks");
        fs::create_dir_all(&stacks_dir).unwrap();

        let custom_stack = serde_json::json!({
            "name": "my-custom",
            "runtime": "bun",
            "description": "My custom microservice template",
            "packages": ["hono"],
            "dev_packages": ["typescript"],
            "files": [
                {
                    "path": "src/index.ts",
                    "content": "import { Hono } from 'hono';\nconst app = new Hono();\nexport default app;"
                }
            ]
        });
        fs::write(
            stacks_dir.join("my-custom.json"),
            serde_json::to_string_pretty(&custom_stack).unwrap(),
        )
        .unwrap();

        // 2. Discover stack
        let found = OnpkgScaffolder::find_stack_in_workspace(ws, "my-custom");
        assert!(found.is_some());
        let stack = found.unwrap();
        assert_eq!(stack.name, "my-custom");
        assert_eq!(stack.files.len(), 1);

        // 3. Scaffold stack
        let target = ws.join("service-output");
        let res = OnpkgScaffolder::scaffold(ws, "my-custom", Some(target.to_str().unwrap()), true)
            .await
            .unwrap();
        assert!(res.contains("Successfully scaffolded stack `my-custom`"));
        assert!(target.join("src/index.ts").exists());
        assert!(target.join("minikit.json").exists());
        assert!(target.join("AGENTS.md").exists());
    }

    #[test]
    fn test_create_custom_stack() {
        let temp = TempDir::new().unwrap();
        let path =
            OnpkgScaffolder::create_custom_stack(temp.path(), "my-starter", "bun", false).unwrap();
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"name\": \"my-starter\""));
        assert!(content.contains("\"runtime\": \"bun\""));

        let found = OnpkgScaffolder::find_stack_in_workspace(temp.path(), "my-starter");
        assert!(found.is_some());
    }
}
