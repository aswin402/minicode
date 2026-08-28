pub mod client;
pub mod doctor;
pub mod scaffolder;
pub mod skills;
pub mod stacks;
pub mod sync;
pub mod types;

use crate::error::Result;
#[allow(unused_imports)]
pub use client::OnpkgClient;
#[allow(unused_imports)]
pub use doctor::OnpkgDoctor;
#[allow(unused_imports)]
pub use scaffolder::OnpkgScaffolder;
#[allow(unused_imports)]
pub use skills::OnpkgSkillsManager;
#[allow(unused_imports)]
pub use stacks::{Stack, StackFile, StackHook};
use std::path::Path;
#[allow(unused_imports)]
pub use sync::OnpkgSyncEngine;
#[allow(unused_imports)]
pub use types::{OnpkgSkillInfo, OnpkgStackInfo};

/// Full-featured native operations for onpkg stack scaffolding, skill management, and project sync.
pub struct OnpkgService;

impl OnpkgService {
    /// Lists all available built-in and custom onpkg stacks natively.
    pub async fn list_stacks(workspace_root: &Path, category: Option<&str>) -> Result<String> {
        let native_stacks = OnpkgScaffolder::get_all_stacks();

        let filtered: Vec<&Stack> = match category {
            Some(cat) if !cat.is_empty() => native_stacks
                .iter()
                .filter(|s| {
                    let c = match s.runtime.as_str() {
                        "uv" | "python" => "backend",
                        "flutter" => "app",
                        _ => "frontend",
                    };
                    c.eq_ignore_ascii_case(cat) || s.runtime.eq_ignore_ascii_case(cat)
                })
                .collect(),
            _ => native_stacks.iter().collect(),
        };

        let mut res = format!(
            "📦 **Available onpkg Stacks** ({} native templates):\n\n",
            filtered.len()
        );

        for s in filtered {
            let cat = match s.runtime.as_str() {
                "uv" | "python" => "backend",
                "flutter" => "app",
                _ => "frontend",
            };

            let tech_str = if s.packages.is_empty() {
                String::new()
            } else {
                format!(
                    " [{}]",
                    s.packages
                        .iter()
                        .take(4)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };

            res.push_str(&format!(
                "• **{}** ({} files, {}){}\n  ╰─── {}\n",
                s.name,
                s.files.len(),
                cat,
                tech_str,
                s.description
            ));
        }

        // If onpkg CLI is installed, check if custom user stacks exist
        if OnpkgClient::is_installed() {
            if let Ok(out) = OnpkgClient::exec(&["stack", "list", "--json"], workspace_root) {
                if let Ok(cli_stacks) = serde_json::from_str::<Vec<OnpkgStackInfo>>(&out) {
                    let custom_stacks: Vec<&OnpkgStackInfo> = cli_stacks
                        .iter()
                        .filter(|cs| !native_stacks.iter().any(|ns| ns.name == cs.name))
                        .collect();

                    if !custom_stacks.is_empty() {
                        res.push_str(&format!(
                            "\n📁 **Custom User Stacks** ({} found in ~/.onpkg):\n\n",
                            custom_stacks.len()
                        ));
                        for cs in custom_stacks {
                            res.push_str(&format!(
                                "• **{}** ({} files, {})\n  ╰─── {}\n",
                                cs.name, cs.files_count, cs.category, cs.description
                            ));
                        }
                    }
                }
            }
        }

        Ok(res)
    }

    /// Shows detailed information and file manifest of a specific stack.
    pub async fn show_stack(workspace_root: &Path, stack_name: &str) -> Result<String> {
        if let Some(stack) = OnpkgScaffolder::find_stack(stack_name) {
            let mut out = format!(
                "📦 **Stack: `{}`**\n\
                • **Runtime / Package Manager:** `{}`\n\
                • **Description:** {}\n\
                • **Total Files:** {} files\n\
                • **Packages:** {}\n\
                • **Dev Packages:** {}\n\n\
                ### File Structure:\n",
                stack.name,
                stack.runtime,
                stack.description,
                stack.files.len(),
                if stack.packages.is_empty() {
                    "none".to_string()
                } else {
                    stack.packages.join(", ")
                },
                if stack.dev_packages.is_empty() {
                    "none".to_string()
                } else {
                    stack.dev_packages.join(", ")
                },
            );

            for f in stack.files.iter().take(25) {
                out.push_str(&format!("  ├── {}\n", f.path));
            }
            if stack.files.len() > 25 {
                out.push_str(&format!(
                    "  ╰── ... and {} more files\n",
                    stack.files.len() - 25
                ));
            }

            return Ok(out);
        }

        // Fallback to CLI if stack is a custom external template
        if OnpkgClient::is_installed() {
            let out = OnpkgClient::exec(&["stack", "show", stack_name], workspace_root)?;
            return Ok(out);
        }

        let available: Vec<String> = OnpkgScaffolder::get_all_stacks()
            .into_iter()
            .map(|s| s.name)
            .collect();
        Err(crate::error::ToolError::InvalidArguments {
            name: "onpkg_stack_show".to_string(),
            reason: format!(
                "Stack `{}` not found. Available built-in stacks: {}",
                stack_name,
                available.join(", ")
            ),
        }
        .into())
    }

    /// Scaffolds an onpkg stack template into the workspace or a target subdirectory.
    pub async fn add_stack(
        workspace_root: &Path,
        stack_name: &str,
        target_dir: Option<&str>,
        no_install: bool,
    ) -> Result<String> {
        // First try native embedded scaffolder
        if OnpkgScaffolder::find_stack(stack_name).is_some() {
            return OnpkgScaffolder::scaffold(workspace_root, stack_name, target_dir, no_install)
                .await;
        }

        // Fallback to CLI if it's a custom external stack
        if OnpkgClient::is_installed() {
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
            return Ok(format!(
                "✔ Successfully scaffolded stack `{}` via onpkg CLI:\n\n{}",
                stack_name, out
            ));
        }

        let available: Vec<String> = OnpkgScaffolder::get_all_stacks()
            .into_iter()
            .map(|s| s.name)
            .collect();
        Err(crate::error::ToolError::InvalidArguments {
            name: "onpkg_stack_add".to_string(),
            reason: format!(
                "Stack `{}` not found. Available built-in stacks: {}",
                stack_name,
                available.join(", ")
            ),
        }
        .into())
    }

    /// Lists all installed or available onpkg agent skills.
    pub async fn list_skills(workspace_root: &Path) -> Result<String> {
        Ok(OnpkgSkillsManager::list_skills(workspace_root))
    }

    /// Installs an agent skill into the project.
    pub async fn install_skill(workspace_root: &Path, skill_name: &str) -> Result<String> {
        OnpkgSkillsManager::install_skill(workspace_root, skill_name)
    }

    /// Synchronizes project dependencies, onpkg.json manifest, AGENTS.md, and onpkg_docs.
    pub async fn sync_project(workspace_root: &Path) -> Result<String> {
        OnpkgSyncEngine::sync(workspace_root)
    }

    /// Runs runtime and tool health diagnostics.
    pub async fn run_doctor(_workspace_root: &Path) -> Result<String> {
        Ok(OnpkgDoctor::diagnose())
    }
}
