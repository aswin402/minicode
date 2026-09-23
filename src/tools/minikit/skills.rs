use crate::error::{Result, ToolError};
use crate::tools::minikit::builtin_skills::{find_builtin_skill, get_all_builtin_skills};
use crate::tools::minikit::sync::MiniKitSyncEngine;
use std::fs;
use std::path::{Path, PathBuf};

/// Parsed metadata representing a progressive domain skill with glob triggers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub globs: Vec<String>,
    pub always_apply: bool,
    pub is_builtin: bool,
    pub path: Option<PathBuf>,
}

impl SkillMetadata {
    /// Parses frontmatter and metadata from a markdown skill file.
    pub fn parse_from_markdown(name: &str, content: &str, path: Option<PathBuf>) -> Self {
        let mut description = String::new();
        let mut globs = Vec::new();
        let mut always_apply = false;

        if let Some(rest) = content.strip_prefix("---") {
            if let Some(end) = rest.find("---") {
                let frontmatter = &rest[..end];
                for line in frontmatter.lines() {
                    let trimmed = line.trim();
                    if let Some(r) = trimmed.strip_prefix("description:") {
                        description = r.trim().trim_matches('"').trim_matches('\'').to_string();
                    } else if let Some(r) = trimmed.strip_prefix("always_apply:") {
                        always_apply = r.trim().parse::<bool>().unwrap_or(false);
                    } else if let Some(r) = trimmed.strip_prefix("globs:") {
                        let list_str = r.trim().trim_matches('[').trim_matches(']');
                        for item in list_str.split(',') {
                            let g = item.trim().trim_matches('"').trim_matches('\'');
                            if !g.is_empty() {
                                globs.push(g.to_string());
                            }
                        }
                    }
                }
            }
        }

        if description.is_empty() {
            description = content
                .lines()
                .find(|l| l.starts_with('#') || !l.trim().is_empty())
                .unwrap_or("Domain skill")
                .trim_start_matches('#')
                .trim()
                .to_string();
        }

        if globs.is_empty() {
            if let Some(builtin) = find_builtin_skill(name) {
                globs = builtin.globs.iter().map(|s| s.to_string()).collect();
            }
        }

        Self {
            name: name.to_string(),
            description,
            globs,
            always_apply,
            is_builtin: false,
            path,
        }
    }

    /// Evaluates whether this skill applies to the given relative or absolute file path.
    pub fn matches_path(&self, workspace_root: &Path, file_path: &Path) -> bool {
        if self.always_apply {
            return true;
        }
        if self.globs.is_empty() {
            return false;
        }

        let rel_path = if file_path.is_absolute() {
            file_path.strip_prefix(workspace_root).unwrap_or(file_path)
        } else {
            file_path
        };

        let mut builder = ignore::overrides::OverrideBuilder::new(workspace_root);
        for g in &self.globs {
            let _ = builder.add(g);
        }
        if let Ok(overrides) = builder.build() {
            overrides.matched(rel_path, false).is_whitelist()
        } else {
            false
        }
    }
}

/// Native skills manager for listing, showing, and installing domain skills into the workspace.
pub struct MiniKitSkillsManager;

#[allow(dead_code)]
pub type OnpkgSkillsManager = MiniKitSkillsManager;

impl MiniKitSkillsManager {
    /// Returns the standard search paths for skills.
    pub fn get_skill_paths(workspace_root: &Path) -> Vec<PathBuf> {
        let mut paths = vec![
            workspace_root.join(".minicode").join("skills"),
            workspace_root.join(crate::constants::MINIKIT_DOCS_DIR),
            workspace_root.join(crate::constants::ONPKG_DOCS_DIR),
        ];

        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".config").join("minicode").join("skills"));
            paths.push(home.join(".minikit").join("skills"));
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
        res.push_str("\n💡 Run `minicode kit skill install <name>` or use `kit_skill_install` to install into workspace.\n");

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
            MiniKitSyncEngine::sync(workspace_root).ok();
            return Ok(format!(
                "✔ Successfully installed built-in skill `{}` into `.minicode/skills/{}/SKILL.md`\nDescription: {}\nManifest & OKF knowledge docs synchronized.",
                builtin.name, clean_name, builtin.description
            ));
        }

        // 2. If skill exists in global ~/.minikit/skills, ~/.onpkg/skills or ~/.config/minicode/skills, copy it
        if let Some(home) = dirs::home_dir() {
            let global_candidates = [
                home.join(".config")
                    .join("minicode")
                    .join("skills")
                    .join(format!("{}.md", clean_name)),
                home.join(".minikit")
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
                home.join(".minikit")
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
                    MiniKitSyncEngine::sync(workspace_root).ok();
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
        MiniKitSyncEngine::sync(workspace_root).ok();

        Ok(format!(
            "✔ Successfully created and installed custom skill `{}` at `.minicode/skills/{}/SKILL.md`",
            clean_name, clean_name
        ))
    }

    fn add_to_manifest_active_skills(workspace_root: &Path, skill_name: &str) -> Result<()> {
        let manifest_path = match super::resolve_manifest_path(workspace_root) {
            Some(p) => p,
            None => return Ok(()),
        };

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

    /// Removes an installed custom skill from the workspace (.minicode/skills/<name>).
    pub fn remove_skill(workspace_root: &Path, skill_name: &str) -> Result<String> {
        let clean = skill_name.trim().to_lowercase();
        if clean.is_empty() || clean.contains('/') || clean.contains('\\') || clean.contains("..") {
            return Err(ToolError::InvalidArguments {
                name: "onpkg_skill_remove".to_string(),
                reason: format!(
                    "Invalid skill name '{}': must not contain path separators or traversal",
                    skill_name
                ),
            }
            .into());
        }
        let target_dir = workspace_root.join(".minicode").join("skills").join(&clean);
        let mut removed = false;

        if target_dir.exists() {
            fs::remove_dir_all(&target_dir).map_err(|e| ToolError::FileOp {
                path: target_dir.display().to_string(),
                source: e,
            })?;
            removed = true;
        }

        let target_file = workspace_root
            .join(".minicode")
            .join("skills")
            .join(format!("{}.md", clean));
        if target_file.exists() {
            fs::remove_file(&target_file).map_err(|e| ToolError::FileOp {
                path: target_file.display().to_string(),
                source: e,
            })?;
            removed = true;
        }

        for docs_dir in &[
            crate::constants::MINIKIT_DOCS_DIR,
            crate::constants::ONPKG_DOCS_DIR,
        ] {
            let doc_file = workspace_root.join(docs_dir).join(format!("{}.md", clean));
            if doc_file.exists() {
                fs::remove_file(&doc_file).ok();
                removed = true;
            }
        }

        Self::remove_from_manifest_active_skills(workspace_root, &clean).ok();
        MiniKitSyncEngine::sync(workspace_root).ok();

        if removed {
            Ok(format!(
                "✔ Successfully removed skill `{}` from workspace.",
                clean
            ))
        } else {
            Ok(format!(
                "ℹ Skill `{}` was not found installed in workspace.",
                clean
            ))
        }
    }

    fn remove_from_manifest_active_skills(workspace_root: &Path, skill_name: &str) -> Result<()> {
        let manifest_path = match super::resolve_manifest_path(workspace_root) {
            Some(p) => p,
            None => return Ok(()),
        };

        let content = fs::read_to_string(&manifest_path).map_err(|e| ToolError::FileOp {
            path: manifest_path.display().to_string(),
            source: e,
        })?;

        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(skills) = val.get_mut("active_skills").and_then(|s| s.as_array_mut()) {
                skills.retain(|s| {
                    s.as_str().map(|n| n.to_lowercase()) != Some(skill_name.to_lowercase())
                });
                if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                    fs::write(&manifest_path, pretty).ok();
                }
            }
        }

        Ok(())
    }

    /// Resolves the list of explicitly configured skill names from the workspace manifest (`minikit.json` or `onpkg.json`), if any.
    /// No hardcoded heuristics or file guessing: minicode's LLM agent decides when to consult,
    /// show, or install skills via `kit_skill_show` dynamically, or when instructed by the user.
    pub fn get_manifest_active_skills(workspace_root: &Path) -> Vec<String> {
        let mut skill_names = Vec::new();

        if let Some(manifest_path) = super::resolve_manifest_path(workspace_root) {
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(skills) = val.get("active_skills").and_then(|s| s.as_array()) {
                        for s in skills {
                            if let Some(name) = s.as_str() {
                                let trimmed = name.trim().to_lowercase();
                                if !trimmed.is_empty() && !skill_names.contains(&trimmed) {
                                    skill_names.push(trimmed);
                                }
                            }
                        }
                    }
                }
            }
        }

        skill_names
    }

    /// Returns all available skills (installed workspace skills + built-in catalog) as SkillMetadata.
    pub fn get_all_skills_metadata(workspace_root: &Path) -> Vec<SkillMetadata> {
        let mut list = Vec::new();
        let search_dirs = Self::get_skill_paths(workspace_root);

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
                        let content = fs::read_to_string(&path).unwrap_or_default();
                        let meta = SkillMetadata::parse_from_markdown(&name, &content, Some(path));
                        list.push(meta);
                    } else if path.is_dir() {
                        let skill_file = path.join("SKILL.md");
                        if skill_file.exists() {
                            let name = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            let content = fs::read_to_string(&skill_file).unwrap_or_default();
                            let meta = SkillMetadata::parse_from_markdown(
                                &name,
                                &content,
                                Some(skill_file),
                            );
                            list.push(meta);
                        }
                    }
                }
            }
        }

        // Add built-ins if not already in workspace
        let builtins = get_all_builtin_skills();
        for b in builtins {
            if !list.iter().any(|s| s.name.eq_ignore_ascii_case(b.name)) {
                list.push(SkillMetadata {
                    name: b.name.to_string(),
                    description: b.description.to_string(),
                    globs: b.globs.iter().map(|s| s.to_string()).collect(),
                    always_apply: false,
                    is_builtin: true,
                    path: None,
                });
            }
        }

        list.sort_by(|a, b| a.name.cmp(&b.name));
        list.dedup_by(|a, b| a.name == b.name);
        list
    }

    /// Matches skills whose globs or always_apply rules apply to the specified file path.
    pub fn match_skills_for_path(workspace_root: &Path, file_path: &Path) -> Vec<SkillMetadata> {
        let all = Self::get_all_skills_metadata(workspace_root);
        let manifest_active = Self::get_manifest_active_skills(workspace_root);

        all.into_iter()
            .filter(|s| {
                manifest_active
                    .iter()
                    .any(|m| m.eq_ignore_ascii_case(&s.name))
                    || s.matches_path(workspace_root, file_path)
            })
            .collect()
    }

    /// Formats a list of matching skills for a path.
    pub fn format_skill_matches(workspace_root: &Path, path_str: &str) -> String {
        let p = Path::new(path_str);
        let matches = Self::match_skills_for_path(workspace_root, p);
        if matches.is_empty() {
            format!("ℹ No skills currently matched for `{}`.", path_str)
        } else {
            let mut out = format!(
                "\n🎯 **Matched Skills for `{}`** ({} matches):\n\n",
                path_str,
                matches.len()
            );
            for m in matches {
                let source = if m.is_builtin {
                    "built-in"
                } else {
                    "installed"
                };
                let globs_str = if m.globs.is_empty() {
                    "always".to_string()
                } else {
                    m.globs.join(", ")
                };
                out.push_str(&format!(
                    "  • **`{}`** [{}] — {}\n    Globs: {}\n",
                    m.name, source, m.description, globs_str
                ));
            }
            out
        }
    }

    /// Formats progressive agent prompt context (Tier 1 lightweight directory + Tier 2 matched rules).
    pub fn format_progressive_prompt_context(
        workspace_root: &Path,
        active_files: &[PathBuf],
    ) -> String {
        let all_skills = Self::get_all_skills_metadata(workspace_root);
        if all_skills.is_empty() {
            return String::new();
        }

        let manifest_active = Self::get_manifest_active_skills(workspace_root);
        let mut out = String::from("\n# Progressive Domain Skills & Guidelines (MiniKit):\n");

        // Tier 1: Directory of available skills
        out.push_str("Available technology skills in this workspace (invoke `kit_skill_show(name)` on-demand for full rules):\n");
        for s in &all_skills {
            let globs_hint = if !s.globs.is_empty() {
                format!(" [globs: {}]", s.globs.join(", "))
            } else {
                String::new()
            };
            out.push_str(&format!(
                "  • `{}`: {}{}\n",
                s.name, s.description, globs_hint
            ));
        }

        // Tier 2: Automatically attach rules for active files or explicitly enabled skills
        let mut matched_skills = Vec::new();
        for s in &all_skills {
            let is_manifest = manifest_active
                .iter()
                .any(|m| m.eq_ignore_ascii_case(&s.name));
            let matches_file = active_files
                .iter()
                .any(|f| s.matches_path(workspace_root, f));
            if is_manifest || s.always_apply || matches_file {
                matched_skills.push(s);
            }
        }

        if !matched_skills.is_empty() {
            out.push_str("\n## Auto-Activated Skills for Current Workspace / Files:\n");
            for s in matched_skills {
                out.push_str(&format!("### Skill `{}`\n", s.name));
                let content = if let Some(p) = &s.path {
                    fs::read_to_string(p).unwrap_or_default()
                } else if let Some(b) = find_builtin_skill(&s.name) {
                    b.content.to_string()
                } else {
                    String::new()
                };

                let body = if let Some(rest) = content.strip_prefix("---") {
                    if let Some(end) = rest.find("---") {
                        rest[end + 3..].trim()
                    } else {
                        content.as_str()
                    }
                } else {
                    content.as_str()
                };

                let lines: Vec<&str> = body.lines().take(40).collect();
                out.push_str(&lines.join("\n"));
                out.push_str("\n\n");
            }
        }

        out
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

    #[test]
    fn test_get_manifest_active_skills() {
        let temp = TempDir::new().unwrap();
        let manifest = r#"{
            "name": "test-app",
            "active_skills": ["react", "tailwind"]
        }"#;
        fs::write(temp.path().join("minikit.json"), manifest).unwrap();

        let skills = OnpkgSkillsManager::get_manifest_active_skills(temp.path());
        assert_eq!(skills, vec!["react".to_string(), "tailwind".to_string()]);
    }

    #[test]
    fn test_remove_skill() {
        let temp = TempDir::new().unwrap();
        // 1. Install skill
        OnpkgSkillsManager::install_skill(temp.path(), "react").unwrap();
        assert!(temp.path().join(".minicode/skills/react/SKILL.md").exists());

        // 2. Remove skill
        let msg = OnpkgSkillsManager::remove_skill(temp.path(), "react").unwrap();
        assert!(msg.contains("Successfully removed skill `react`"));
        assert!(!temp.path().join(".minicode/skills/react").exists());
    }

    #[test]
    fn test_skill_metadata_parsing_and_globs() {
        let content = r#"---
name: custom-ui
description: "Custom UI guidelines"
globs: ["src/ui/**/*.tsx", "components/**/*.jsx"]
always_apply: false
---

# Custom UI
Follow design tokens.
"#;
        let meta = SkillMetadata::parse_from_markdown("custom-ui", content, None);
        assert_eq!(meta.name, "custom-ui");
        assert_eq!(meta.description, "Custom UI guidelines");
        assert_eq!(meta.globs, vec!["src/ui/**/*.tsx", "components/**/*.jsx"]);
        assert!(!meta.always_apply);
    }

    #[test]
    fn test_skill_glob_matching() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        let react_skill = SkillMetadata {
            name: "react".to_string(),
            description: "React guidelines".to_string(),
            globs: vec!["**/*.tsx".to_string(), "**/*.jsx".to_string()],
            always_apply: false,
            is_builtin: true,
            path: None,
        };

        assert!(react_skill.matches_path(ws, Path::new("src/components/Button.tsx")));
        assert!(react_skill.matches_path(ws, Path::new("app/index.jsx")));
        assert!(!react_skill.matches_path(ws, Path::new("src/api/handler.rs")));
    }

    #[test]
    fn test_progressive_prompt_context_generation() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        let active_files = vec![PathBuf::from("src/App.tsx")];
        let prompt_context =
            MiniKitSkillsManager::format_progressive_prompt_context(ws, &active_files);

        // Tier 1 check: lists available skills
        assert!(prompt_context.contains("Progressive Domain Skills & Guidelines"));
        assert!(prompt_context.contains("react"));
        assert!(prompt_context.contains("rust"));

        // Tier 2 check: auto-activated react because App.tsx matches react globs
        assert!(prompt_context.contains("Auto-Activated Skills for Current Workspace / Files"));
        assert!(prompt_context.contains("### Skill `react`"));
    }
}
