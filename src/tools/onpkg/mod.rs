pub mod client;
pub mod types;

use crate::error::Result;
#[allow(unused_imports)]
pub use client::OnpkgClient;
use std::path::Path;
#[allow(unused_imports)]
pub use types::{OnpkgSkillInfo, OnpkgStackInfo};

/// High-level operations for onpkg stack scaffolding, skill management, and project sync.
pub struct OnpkgService;

impl OnpkgService {
    /// Lists all available built-in and custom onpkg stacks.
    pub async fn list_stacks(workspace_root: &Path, category: Option<&str>) -> Result<String> {
        let out = OnpkgClient::exec(&["stack", "list", "--json"], workspace_root)?;

        if let Ok(stacks) = serde_json::from_str::<Vec<OnpkgStackInfo>>(&out) {
            let filtered: Vec<&OnpkgStackInfo> = match category {
                Some(cat) if !cat.is_empty() => stacks
                    .iter()
                    .filter(|s| s.category.eq_ignore_ascii_case(cat))
                    .collect(),
                _ => stacks.iter().collect(),
            };

            if filtered.is_empty() {
                return Ok(format!(
                    "ℹ No onpkg stacks found matching category `{}`.",
                    category.unwrap_or("all")
                ));
            }

            let mut res = format!(
                "📦 Available onpkg Stacks ({} templates):\n\n",
                filtered.len()
            );

            for s in filtered {
                let tech_str = if s.technologies.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", s.technologies.join(", "))
                };

                res.push_str(&format!(
                    "• **{}** ({} files, {}){}\n  ╰─── {}\n",
                    s.name, s.files_count, s.category, tech_str, s.description
                ));
            }

            return Ok(res);
        }

        // Fallback to raw text output if JSON parsing is unavailable
        let raw_out = OnpkgClient::exec(&["stack", "list"], workspace_root)?;
        Ok(raw_out)
    }

    /// Shows detailed information about a specific onpkg stack.
    pub async fn show_stack(workspace_root: &Path, stack_name: &str) -> Result<String> {
        let out = OnpkgClient::exec(&["stack", "show", stack_name], workspace_root)?;
        Ok(out)
    }

    /// Scaffolds an onpkg stack template into the workspace or a target subdirectory.
    pub async fn add_stack(
        workspace_root: &Path,
        stack_name: &str,
        target_dir: Option<&str>,
        no_install: bool,
    ) -> Result<String> {
        let mut args = vec!["stack", "add", stack_name];

        if let Some(dir) = target_dir {
            if !dir.trim().is_empty() {
                args.push("--dir");
                args.push(dir);
            }
        }

        if no_install {
            args.push("--no-install");
        }

        let out = OnpkgClient::exec(&args, workspace_root)?;
        Ok(format!(
            "✔ Successfully scaffolded stack `{}`:\n\n{}",
            stack_name, out
        ))
    }

    /// Lists all installed or available onpkg agent skills.
    pub async fn list_skills(workspace_root: &Path) -> Result<String> {
        let out = OnpkgClient::exec(&["skill", "list"], workspace_root)?;
        Ok(out)
    }

    /// Installs an agent skill from onpkg into the project.
    pub async fn install_skill(workspace_root: &Path, skill_name: &str) -> Result<String> {
        let out = OnpkgClient::exec(&["skill", "install", skill_name], workspace_root)?;
        Ok(format!(
            "✔ Successfully installed skill `{}`:\n\n{}",
            skill_name, out
        ))
    }

    /// Synchronizes project dependencies, onpkg.json manifest, AGENTS.md, and onpkg_docs.
    pub async fn sync_project(workspace_root: &Path) -> Result<String> {
        let out = OnpkgClient::exec(&["sync"], workspace_root)?;
        Ok(format!("✔ Project synchronized with onpkg:\n\n{}", out))
    }

    /// Runs runtime and tool health diagnostics.
    pub async fn run_doctor(workspace_root: &Path) -> Result<String> {
        let out = OnpkgClient::exec(&["doctor"], workspace_root)?;
        Ok(out)
    }
}
