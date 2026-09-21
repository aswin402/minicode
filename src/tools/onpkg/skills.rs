use crate::error::{Result, ToolError};
use crate::tools::onpkg::builtin_skills::{find_builtin_skill, get_all_builtin_skills};
use crate::tools::onpkg::sync::OnpkgSyncEngine;
use std::fs;
use std::path::{Path, PathBuf};

/// Native skills manager for listing, showing, and installing domain skills into the workspace.
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

    /// Lists all installed skills and available built-in skills.
    pub fn list_skills(workspace_root: &Path) -> String {
        let search_dirs = Self::get_skill_paths(workspace_root);
        let mut installed_skills: Vec<(String, String)> = Vec::new();

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
                        installed_skills.push((name, snippet));
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
                            installed_skills.push((name, snippet));
                        }
                    }
                }
            }
        }

        installed_skills.sort_by(|a, b| a.0.cmp(&b.0));
        installed_skills.dedup_by(|a, b| a.0 == b.0);

        let mut res = String::new();

        if !installed_skills.is_empty() {
            res.push_str(&format!(
                "🧠 **Installed Agent Skills** ({} active):\n\n",
                installed_skills.len()
            ));
            for (name, desc) in installed_skills {
                res.push_str(&format!("  • **`{}`**: {}\n", name, desc));
            }
            res.push('\n');
        } else {
            res.push_str("ℹ No custom skills currently installed in `.minicode/skills/`.\n\n");
        }

        // Available built-in catalog
        let builtins = get_all_builtin_skills();
        res.push_str(&format!(
            "📦 **Built-in Skill Library** ({} available):\n\n",
            builtins.len()
        ));
        for s in builtins {
            res.push_str(&format!("  • **`{:<16}`** — {}\n", s.name, s.description));
        }
        res.push_str("\n💡 Run `minicode skill install <name>` or use `onpkg_skill_install` to install into workspace.\n");

        res
    }

    /// Shows full contents and instructions of a skill (from workspace or built-in library).
    pub fn show_skill(workspace_root: &Path, skill_name: &str) -> Result<String> {
        let clean = skill_name.trim();

        // 1. Check workspace
        let local_path = workspace_root
            .join(".minicode")
            .join("skills")
            .join(clean)
            .join("SKILL.md");
        if local_path.exists() {
            let content = fs::read_to_string(&local_path).map_err(|e| ToolError::FileOp {
                path: local_path.display().to_string(),
                source: e,
            })?;
            return Ok(format!(
                "📖 **Skill `{}`** (Workspace Local: `{}`):\n\n{}",
                clean,
                local_path.display(),
                content
            ));
        }

        // 2. Check built-in library
        if let Some(builtin) = find_builtin_skill(clean) {
            return Ok(format!(
                "📖 **Skill `{}`** (Built-in Library):\n*{}*\n\n{}",
                builtin.name, builtin.description, builtin.content
            ));
        }

        // 3. Check global directories
        if let Some(home) = dirs::home_dir() {
            let global_path = home
                .join(".config")
                .join("minicode")
                .join("skills")
                .join(clean)
                .join("SKILL.md");
            if global_path.exists() {
                let content = fs::read_to_string(&global_path).map_err(|e| ToolError::FileOp {
                    path: global_path.display().to_string(),
                    source: e,
                })?;
                return Ok(format!(
                    "📖 **Skill `{}`** (Global Config: `{}`):\n\n{}",
                    clean,
                    global_path.display(),
                    content
                ));
            }
        }

        Err(ToolError::InvalidArguments {
            name: "onpkg_skill_show".to_string(),
            reason: format!(
                "Skill `{}` not found. Run `minicode skill list` to see all available skills.",
                skill_name
            ),
        }
        .into())
    }

    /// Installs a skill package into the workspace `.minicode/skills/<name>/SKILL.md`.
    pub fn install_skill(workspace_root: &Path, skill_name: &str) -> Result<String> {
        let clean_name = skill_name.trim();
        if clean_name.is_empty()
            || clean_name.contains('/')
            || clean_name.contains('\\')
            || clean_name.contains("..")
        {
            return Err(ToolError::InvalidArguments {
                name: "onpkg_skill_install".to_string(),
                reason: format!(
                    "Invalid skill name '{}': must not contain path separators or traversal",
                    skill_name
                ),
            }
            .into());
        }
        let dest_dir = workspace_root
            .join(".minicode")
            .join("skills")
            .join(clean_name);
        fs::create_dir_all(&dest_dir).map_err(|e| ToolError::FileOp {
            path: dest_dir.display().to_string(),
            source: e,
        })?;

        let dest_file = dest_dir.join("SKILL.md");

        // 1. Built-in library (Highest priority: battle-tested rich skills)
        if let Some(builtin) = find_builtin_skill(clean_name) {
            fs::write(&dest_file, builtin.content).map_err(|e| ToolError::FileOp {
                path: dest_file.display().to_string(),
                source: e,
            })?;
            Self::add_to_manifest_active_skills(workspace_root, clean_name).ok();
            OnpkgSyncEngine::sync(workspace_root).ok();
            return Ok(format!(
                "✔ Successfully installed built-in skill `{}` into `.minicode/skills/{}/SKILL.md`\nDescription: {}\nManifest & OKF knowledge docs synchronized.",
                builtin.name, clean_name, builtin.description
            ));
        }

        // 2. If skill exists in global ~/.onpkg/skills or ~/.config/minicode/skills, copy it
        if let Some(home) = dirs::home_dir() {
            let global_candidates = [
                home.join(".config")
                    .join("minicode")
                    .join("skills")
                    .join(format!("{}.md", clean_name)),
                home.join(".onpkg")
                    .join("skills")
                    .join(format!("{}.md", clean_name)),
                home.join(".config")
                    .join("minicode")
                    .join("skills")
                    .join(clean_name)
                    .join("SKILL.md"),
            ];

            for c in global_candidates {
                if c.exists() {
                    let content = fs::read_to_string(&c).unwrap_or_default();
                    fs::write(&dest_file, &content).map_err(|e| ToolError::FileOp {
                        path: dest_file.display().to_string(),
                        source: e,
                    })?;
                    Self::add_to_manifest_active_skills(workspace_root, clean_name).ok();
                    OnpkgSyncEngine::sync(workspace_root).ok();
                    return Ok(format!(
                        "✔ Successfully installed skill `{}` from `{}` into `.minicode/skills/{}/SKILL.md`",
                        clean_name, c.display(), clean_name
                    ));
                }
            }
        }

        // 3. Fallback: Generate baseline skill contract template
        let default_skill = format!(
            "# {}\n\n## Description\nSpecialized instructions and coding conventions for {}.\n\n## Rules & Guidelines\n1. Follow idiomatic {} patterns.\n2. Verify changes with unit and integration tests.\n",
            clean_name, clean_name, clean_name
        );

        fs::write(&dest_file, default_skill).map_err(|e| ToolError::FileOp {
            path: dest_file.display().to_string(),
            source: e,
        })?;
        Self::add_to_manifest_active_skills(workspace_root, clean_name).ok();
        OnpkgSyncEngine::sync(workspace_root).ok();

        Ok(format!(
            "✔ Successfully created and installed custom skill `{}` at `.minicode/skills/{}/SKILL.md`",
            clean_name, clean_name
        ))
    }

    fn add_to_manifest_active_skills(workspace_root: &Path, skill_name: &str) -> Result<()> {
        let manifest_path = workspace_root.join(crate::constants::ONPKG_MANIFEST_FILE);
        if !manifest_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&manifest_path).map_err(|e| ToolError::FileOp {
            path: manifest_path.display().to_string(),
            source: e,
        })?;

        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(skills) = val.get_mut("active_skills").and_then(|s| s.as_array_mut()) {
                let name_val = serde_json::Value::String(skill_name.to_string());
                if !skills.contains(&name_val) {
                    skills.push(name_val);
                    if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                        fs::write(&manifest_path, pretty).ok();
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_builtin_skills_catalog() {
        let skills = get_all_builtin_skills();
        assert_eq!(skills.len(), 14);
        assert!(find_builtin_skill("react").is_some());
        assert!(find_builtin_skill("next").is_some());
        assert!(find_builtin_skill("fastapi").is_some());
        assert!(find_builtin_skill("flutter").is_some());
        assert!(find_builtin_skill("rust").is_some());
        assert!(find_builtin_skill("frontend-design").is_some());
        assert!(find_builtin_skill("ui-ux-pro-max").is_some());
    }

    #[test]
    fn test_install_builtin_skill() {
        let temp = TempDir::new().unwrap();
        let res = OnpkgSkillsManager::install_skill(temp.path(), "react").unwrap();
        assert!(res.contains("Successfully installed built-in skill `react`"));

        let installed_file = temp.path().join(".minicode/skills/react/SKILL.md");
        assert!(installed_file.exists());
        let content = fs::read_to_string(installed_file).unwrap();
        assert!(content.contains("React AI Agent Skill"));
    }
}
