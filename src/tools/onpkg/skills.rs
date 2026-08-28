use crate::error::{Result, ToolError};
use std::fs;
use std::path::{Path, PathBuf};

/// Native skills manager for listing and installing domain skills into the workspace.
pub struct OnpkgSkillsManager;

impl OnpkgSkillsManager {
    /// Returns the standard search paths for skills.
    pub fn get_skill_paths(workspace_root: &Path) -> Vec<PathBuf> {
        let mut paths = vec![
            workspace_root.join(".minicode").join("skills"),
            workspace_root.join("onpkg_docs"),
        ];

        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".config").join("minicode").join("skills"));
            paths.push(home.join(".onpkg").join("skills"));
        }

        paths
    }

    /// Lists all discovered agent skills across local workspace and global config directories.
    pub fn list_skills(workspace_root: &Path) -> String {
        let search_dirs = Self::get_skill_paths(workspace_root);
        let mut found_skills: Vec<(String, String)> = Vec::new();

        for dir in &search_dirs {
            if !dir.exists() || !dir.is_dir() {
                continue;
            }

            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                        let name = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let snippet = fs::read_to_string(&path)
                            .unwrap_or_default()
                            .lines()
                            .find(|l| l.starts_with('#') || !l.trim().is_empty())
                            .unwrap_or("Domain skill")
                            .trim_start_matches('#')
                            .trim()
                            .to_string();
                        found_skills.push((name, snippet));
                    } else if path.is_dir() {
                        let skill_file = path.join("SKILL.md");
                        if skill_file.exists() {
                            let name = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            let snippet = fs::read_to_string(&skill_file)
                                .unwrap_or_default()
                                .lines()
                                .find(|l| l.starts_with('#') || !l.trim().is_empty())
                                .unwrap_or("Domain skill")
                                .trim_start_matches('#')
                                .trim()
                                .to_string();
                            found_skills.push((name, snippet));
                        }
                    }
                }
            }
        }

        found_skills.sort_by(|a, b| a.0.cmp(&b.0));
        found_skills.dedup_by(|a, b| a.0 == b.0);

        if found_skills.is_empty() {
            return "ℹ No active skills found in `.minicode/skills/` or `onpkg_docs/`.\nUse `onpkg_skill_install` to install skills.".to_string();
        }

        let mut res = format!(
            "🧠 **Active AI Agent Skills** ({} installed):\n\n",
            found_skills.len()
        );
        for (name, desc) in found_skills {
            res.push_str(&format!("• **`{}`**: {}\n", name, desc));
        }

        res
    }

    /// Installs a skill package into the workspace `.minicode/skills/<name>/SKILL.md`.
    pub fn install_skill(workspace_root: &Path, skill_name: &str) -> Result<String> {
        let dest_dir = workspace_root
            .join(".minicode")
            .join("skills")
            .join(skill_name);
        fs::create_dir_all(&dest_dir).map_err(|e| ToolError::FileOp {
            path: dest_dir.display().to_string(),
            source: e,
        })?;

        let dest_file = dest_dir.join("SKILL.md");

        // If skill exists in global ~/.onpkg/skills or ~/.config/minicode/skills, copy it
        if let Some(home) = dirs::home_dir() {
            let global_candidates = [
                home.join(".config")
                    .join("minicode")
                    .join("skills")
                    .join(format!("{}.md", skill_name)),
                home.join(".onpkg")
                    .join("skills")
                    .join(format!("{}.md", skill_name)),
                home.join(".config")
                    .join("minicode")
                    .join("skills")
                    .join(skill_name)
                    .join("SKILL.md"),
            ];

            for c in global_candidates {
                if c.exists() {
                    let content = fs::read_to_string(&c).unwrap_or_default();
                    fs::write(&dest_file, &content).map_err(|e| ToolError::FileOp {
                        path: dest_file.display().to_string(),
                        source: e,
                    })?;
                    return Ok(format!("✔ Successfully installed skill `{}` from `{}` into `.minicode/skills/{}/SKILL.md`", skill_name, c.display(), skill_name));
                }
            }
        }

        // Generate baseline skill contract template
        let default_skill = format!(
            "# {}\n\n## Description\nSpecialized instructions and coding conventions for {}.\n\n## Rules & Guidelines\n1. Follow idiomatic {} patterns.\n2. Verify changes with unit and integration tests.\n",
            skill_name, skill_name, skill_name
        );

        fs::write(&dest_file, default_skill).map_err(|e| ToolError::FileOp {
            path: dest_file.display().to_string(),
            source: e,
        })?;

        Ok(format!(
            "✔ Successfully created and installed skill `{}` at `.minicode/skills/{}/SKILL.md`",
            skill_name, skill_name
        ))
    }
}
