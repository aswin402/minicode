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
            workspace_root
                .join(crate::constants::MINIKIT_DOCS_DIR)
                .join("skills"),
            workspace_root.join(crate::constants::MINIKIT_DOCS_DIR),
            workspace_root
                .join(crate::constants::ONPKG_DOCS_DIR)
                .join("skills"),
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
        let store = crate::vault::store::VaultStore::new(workspace_root);
        let all = store.list_all_skills();

        let mut installed = Vec::new();
        let mut available = Vec::new();

        for s in all {
            if s.is_active_in_project {
                installed.push(s);
            } else {
                available.push(s);
            }
        }

        let mut res = String::new();

        if !installed.is_empty() {
            res.push_str(&format!(
                "🧠 **Installed Agent Skills** ({} active in workspace):\n\n",
                installed.len()
            ));
            for s in &installed {
                res.push_str(&format!("  • **`{}`**: {}\n", s.name, s.description));
            }
            res.push('\n');
        } else {
            res.push_str("ℹ No custom skills currently installed in `.minicode/skills/`.\n\n");
        }

        res.push_str(&format!(
            "📦 **Available Skill Library** ({} available in MiniVault):\n\n",
            available.len()
        ));
        for s in available {
            res.push_str(&format!("  • **`{:<16}`** — {}\n", s.name, s.description));
        }
        res.push_str("\n💡 Run `minicode kit skill install <name>` or use `kit_skill_install` to install into workspace.\n");

        res
    }

    /// Shows full contents and instructions of a skill (from workspace or built-in library).
    pub fn show_skill(workspace_root: &Path, skill_name: &str) -> Result<String> {
        let clean = skill_name.trim();
        let store = crate::vault::store::VaultStore::new(workspace_root);
        match store.get_skill(clean) {
            Ok(skill) => {
                let scope_label = match skill.scope {
                    crate::vault::models::SkillScope::Project => "Workspace Project Local",
                    crate::vault::models::SkillScope::Global => "User Global Vault",
                    crate::vault::models::SkillScope::Builtin => "Built-in Production Library",
                };
                let path_str = skill
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "builtin".to_string());
                Ok(format!(
                    "📖 **Skill `{}`** ({}: `{}`):\n*{}*\n\n{}",
                    skill.name, scope_label, path_str, skill.description, skill.instructions
                ))
            }
            Err(_) => Err(ToolError::InvalidArguments {
                name: "kit_skill_show".to_string(),
                reason: format!(
                    "Skill `{}` not found. Run `minicode skill list` to see all available skills.",
                    skill_name
                ),
            }
            .into()),
        }
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

        let store = crate::vault::store::VaultStore::new(workspace_root);
        match store.load_to_project(clean_name) {
            Ok(skill) => {
                MiniKitSyncEngine::sync(workspace_root).ok();
                Ok(format!(
                    "✔ Successfully installed built-in skill `{}` into `.minicode/skills/{}/SKILL.md`\nDescription: {}\nManifest & OKF knowledge docs synchronized.",
                    skill.name, skill.name, skill.description
                ))
            }
            Err(e) => Err(ToolError::ExecutionFailed(format!(
                "kit_skill_install failed for '{}': {}",
                skill_name, e
            ))
            .into()),
        }
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

        let store = crate::vault::store::VaultStore::new(workspace_root);
        match store.unload_from_project(&clean) {
            Ok(_) => {
                MiniKitSyncEngine::sync(workspace_root).ok();
                Ok(format!(
                    "✔ Successfully removed skill `{}` from workspace and active manifest.",
                    clean
                ))
            }
            Err(e) => Err(ToolError::ExecutionFailed(format!(
                "kit_skill_remove failed for '{}': {}",
                skill_name, e
            ))
            .into()),
        }
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
                        let fname = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        if super::CORE_DOC_FILES
                            .iter()
                            .any(|c| c.eq_ignore_ascii_case(&fname))
                            || fname.eq_ignore_ascii_case("index.md")
                            || fname.eq_ignore_ascii_case("log.md")
                        {
                            continue;
                        }

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

    /// Dynamically resolves all skills matching a stack template based on its files and package dependencies.
    /// Purely data-driven: matches skill globs and package dependencies without hardcoded branching.
    pub fn resolve_skills_for_stack(
        workspace_root: &Path,
        stack_files: &[String],
        stack_packages: &[String],
    ) -> Vec<String> {
        let store = crate::vault::store::VaultStore::new(workspace_root);
        let mut tokens: std::collections::HashSet<String> = store.detect_workspace_package_tokens();
        for pkg in stack_packages {
            let lower = pkg.to_lowercase();
            tokens.insert(lower.clone());
            let clean = lower.trim_start_matches('@');
            if let Some((scope, name)) = clean.split_once('/') {
                tokens.insert(scope.to_string());
                tokens.insert(name.to_string());
                for part in name.split('-') {
                    if !part.is_empty() {
                        tokens.insert(part.to_string());
                    }
                }
            } else {
                for part in clean.split('-') {
                    if !part.is_empty() {
                        tokens.insert(part.to_string());
                    }
                }
            }
        }

        let mut matched = Vec::new();
        for skill in store.list_all_skills() {
            // Only true procedural/behavioral Agent Skills (`Workflow`) belong in `active_skills` and `.minicode/skills/`.
            // Package reference documentation (`Reference`) belongs in `minikit_docs/packages/`.
            if skill.kind() != crate::vault::models::SkillKind::Workflow {
                continue;
            }
            if store.matches_skill_to_stack(&skill, &tokens) {
                matched.push(skill.name.clone());
                continue;
            }
            let has_specific_globs = !skill.frontmatter.globs.is_empty()
                && !skill
                    .frontmatter
                    .globs
                    .iter()
                    .any(|g| g == "**/*" || g == "*");
            if has_specific_globs && !stack_files.is_empty() {
                for f in stack_files {
                    if skill.matches_file(workspace_root, Path::new(f)) {
                        matched.push(skill.name.clone());
                        break;
                    }
                }
            }
        }
        matched.sort();
        matched.dedup();
        matched
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
    #[allow(dead_code)]
    pub fn format_progressive_prompt_context(
        workspace_root: &Path,
        active_files: &[PathBuf],
    ) -> String {
        Self::format_progressive_prompt_context_with_prompt(workspace_root, active_files, None)
    }

    /// Formats progressive agent prompt context with optional prompt intent matching (Hermes-style autonomous skill activation).
    #[allow(dead_code)]
    pub fn format_progressive_prompt_context_with_prompt(
        workspace_root: &Path,
        active_files: &[PathBuf],
        prompt: Option<&str>,
    ) -> String {
        let store = crate::vault::store::VaultStore::new(workspace_root);
        store.format_prompt_skills_context(prompt, None, active_files)
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
        assert!(content.contains("React"));
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

        // Tier 1 check: lists available skills in vault
        assert!(prompt_context.contains("Available Domain Skills in Vault"));
        assert!(prompt_context.contains("rust-tokio") || prompt_context.contains("rust"));

        // Tier 2 check: auto-activated react because App.tsx matches react globs
        assert!(prompt_context.contains("Active Technology & Methodology Skills"));
        assert!(prompt_context.contains("Skill `react`"));
    }

    #[test]
    fn test_resolve_skills_for_stack_only_returns_workflow_skills() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        let stack_files = vec![
            "package.json".to_string(),
            "vite.config.ts".to_string(),
            "src/App.tsx".to_string(),
            "src/main.tsx".to_string(),
        ];
        let stack_packages = vec![
            "react".to_string(),
            "react-dom".to_string(),
            "vite".to_string(),
            "tailwindcss".to_string(),
        ];

        let resolved =
            MiniKitSkillsManager::resolve_skills_for_stack(ws, &stack_files, &stack_packages);

        // Must include behavioral frontend/UI workflow skills
        assert!(resolved.contains(&"frontend-design".to_string()));
        assert!(resolved.contains(&"ui-ux-pro-max".to_string()));

        // Must NOT include package reference docs
        assert!(!resolved.contains(&"react".to_string()));
        assert!(!resolved.contains(&"vite".to_string()));
        assert!(!resolved.contains(&"hono".to_string()));
        assert!(!resolved.contains(&"tailwind-v4".to_string()));
        assert!(!resolved.contains(&"express".to_string()));
        assert!(!resolved.contains(&"flutter".to_string()));
    }
}
