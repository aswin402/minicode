use crate::error::{Result, ToolError};
use crate::tools::onpkg::stacks::{builtin::builtin_stacks, Stack};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Native engine for scaffolding application stacks, generating manifests, and auto-installing packages.
pub struct OnpkgScaffolder;

impl OnpkgScaffolder {
    /// Returns all natively embedded built-in stacks.
    pub fn get_all_stacks() -> Vec<Stack> {
        builtin_stacks()
    }

    /// Finds a stack by name.
    pub fn find_stack(name: &str) -> Option<Stack> {
        let norm = name.trim().to_lowercase();
        Self::get_all_stacks()
            .into_iter()
            .find(|s| s.name.to_lowercase() == norm)
    }

    /// Scaffolds a stack into `target_dir`.
    pub async fn scaffold(
        workspace_root: &Path,
        stack_name: &str,
        target_dir_opt: Option<&str>,
        no_install: bool,
    ) -> Result<String> {
        let stack = Self::find_stack(stack_name).ok_or_else(|| {
            let available: Vec<String> =
                Self::get_all_stacks().into_iter().map(|s| s.name).collect();
            ToolError::InvalidArguments {
                name: "onpkg_stack_add".to_string(),
                reason: format!(
                    "Stack `{}` not found. Available built-in stacks: {}",
                    stack_name,
                    available.join(", ")
                ),
            }
        })?;

        let dest_dir: PathBuf = match target_dir_opt {
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

        let manifest_path = dest_dir.join("onpkg.json");
        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest).unwrap_or_default(),
        )
        .ok();

        // 3. Generate AGENTS.md instructions
        let agents_md = format!(
            "# {} — Agent Guidelines & Repository Instructions 🧠\n\n\
            > Scaffolded with `minicode` + `onpkg` native stack engine.\n\n\
            ## Project Summary\n\
            - **Name:** `{}`\n\
            - **Stack:** `{}`\n\
            - **Runtime / Package Manager:** `{}`\n\
            - **Description:** {}\n\n\
            ## Architecture & Conventions\n\
            1. All project specifications and task tracking live under `onpkg_docs/`.\n\
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

        // 4. Generate onpkg_docs/ spec documents
        let docs_dir = dest_dir.join("onpkg_docs");
        fs::create_dir_all(&docs_dir).ok();

        fs::write(
            docs_dir.join("prd.md"),
            format!(
                "# Product Requirements Document — {}\n\n## Overview\n{}\n",
                project_name, stack.description
            ),
        )
        .ok();

        fs::write(
            docs_dir.join("design.md"),
            format!(
                "# Design Specification — {}\n\n## Architecture\nStack: `{}` with runtime `{}`.\n",
                project_name, stack.name, stack.runtime
            ),
        )
        .ok();

        fs::write(
            docs_dir.join("implementation.md"),
            format!("# Implementation Plan — {}\n\n## Milestones\n1. Initial scaffold and dependency check.\n", project_name),
        ).ok();

        fs::write(
            docs_dir.join("todo.md"),
            "# Project Tasks\n\n- [x] Initial stack scaffolding with onpkg engine\n- [ ] Configure core application features\n",
        ).ok();

        // 5. Post-scaffold install hooks
        let mut install_msg = String::new();
        if !no_install {
            install_msg = Self::run_package_installer(&stack.runtime, &dest_dir);
        }

        Ok(format!(
            "✔ Successfully scaffolded stack `{}` in `{}`\n\
            • Files created: {} files\n\
            • Manifest: onpkg.json, AGENTS.md, onpkg_docs/\n\
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
