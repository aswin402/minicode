//! MiniVault storage, multi-tier discovery, and CRUD engine.

use crate::vault::builtin::get_all_builtin_skills;
use crate::vault::models::{
    format_skill_markdown, parse_skill_markdown, sanitize_skill_name, Result, SkillFrontmatter,
    SkillScope, VaultError, VaultSkill,
};
use std::fs;
use std::path::{Path, PathBuf};

/// Multi-tier skills vault manager for minicode.
#[derive(Debug, Clone)]
pub struct VaultStore {
    workspace_root: PathBuf,
}

impl VaultStore {
    /// Creates a new `VaultStore` for the given workspace.
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
        }
    }

    /// Primary project skills directory: `.minicode/skills`
    pub fn project_skills_dir(&self) -> PathBuf {
        self.workspace_root.join(".minicode").join("skills")
    }

    /// Secondary project documentation skills directory: `minikit_docs/skills`
    pub fn project_docs_skills_dir(&self) -> PathBuf {
        self.workspace_root
            .join(crate::constants::MINIKIT_DOCS_DIR)
            .join("skills")
    }

    /// Global user skills directory: `~/.config/minicode/vault/skills`
    pub fn global_skills_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|home| {
            home.join(".config")
                .join(crate::constants::CONFIG_DIR_NAME)
                .join("vault")
                .join("skills")
        })
    }

    /// Legacy global skills directory: `~/.config/minicode/skills`
    pub fn legacy_global_skills_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|home| {
            home.join(".config")
                .join(crate::constants::CONFIG_DIR_NAME)
                .join("skills")
        })
    }

    /// Discovers all skills across all 3 tiers (Built-in, Global, Project).
    /// Project skills override Global skills, which override Built-in skills of the same name.
    pub fn list_all_skills(&self) -> Vec<VaultSkill> {
        let mut skill_map: std::collections::BTreeMap<String, VaultSkill> =
            std::collections::BTreeMap::new();

        // 1. Built-in Core Skills (Tier 1)
        for skill in get_all_builtin_skills() {
            skill_map.insert(skill.name.to_lowercase(), skill);
        }

        // 2. Global User Skills (Tier 2)
        if let Some(global_dir) = Self::global_skills_dir() {
            self.scan_directory_into_map(&global_dir, SkillScope::Global, &mut skill_map);
        }
        if let Some(legacy_dir) = Self::legacy_global_skills_dir() {
            self.scan_directory_into_map(&legacy_dir, SkillScope::Global, &mut skill_map);
        }

        // 3. Project Skills (Tier 3 - highest precedence)
        let project_dir = self.project_skills_dir();
        self.scan_directory_into_map(&project_dir, SkillScope::Project, &mut skill_map);

        let docs_dir = self.project_docs_skills_dir();
        self.scan_directory_into_map(&docs_dir, SkillScope::Project, &mut skill_map);

        // Mark active status based on manifest (minikit.json / onpkg.json) or presence in project
        let manifest_active = self.get_manifest_active_skills();

        let mut skills: Vec<VaultSkill> = skill_map
            .into_values()
            .map(|mut s| {
                if s.scope == SkillScope::Project
                    || manifest_active
                        .iter()
                        .any(|m| m.eq_ignore_ascii_case(&s.name))
                {
                    s.is_active_in_project = true;
                }
                s
            })
            .collect();

        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    /// Scans a directory for `.md` files or `<name>/SKILL.md` folders and inserts them into map.
    fn scan_directory_into_map(
        &self,
        dir: &Path,
        scope: SkillScope,
        map: &mut std::collections::BTreeMap<String, VaultSkill>,
    ) {
        if !dir.exists() || !dir.is_dir() {
            return;
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if let Ok(content) = fs::read_to_string(&path) {
                    let (fm, instructions) = parse_skill_markdown(&content, &name);
                    let skill = VaultSkill {
                        name: fm.name.clone(),
                        description: fm.description.clone(),
                        scope,
                        frontmatter: fm,
                        instructions,
                        raw_content: content,
                        path: Some(path),
                        is_active_in_project: scope == SkillScope::Project,
                    };
                    map.insert(skill.name.to_lowercase(), skill);
                }
            } else if path.is_dir() {
                let skill_file = path.join("SKILL.md");
                let alt_file = path.join("skill.md");
                let target_file = if skill_file.exists() {
                    Some(skill_file)
                } else if alt_file.exists() {
                    Some(alt_file)
                } else {
                    None
                };

                if let Some(target) = target_file {
                    let folder_name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if let Ok(content) = fs::read_to_string(&target) {
                        let (fm, instructions) = parse_skill_markdown(&content, &folder_name);
                        let skill = VaultSkill {
                            name: fm.name.clone(),
                            description: fm.description.clone(),
                            scope,
                            frontmatter: fm,
                            instructions,
                            raw_content: content,
                            path: Some(target),
                            is_active_in_project: scope == SkillScope::Project,
                        };
                        map.insert(skill.name.to_lowercase(), skill);
                    }
                }
            }
        }
    }

    /// Finds a skill by name across the hierarchy (Project -> Global -> Built-in).
    pub fn get_skill(&self, name: &str) -> Result<VaultSkill> {
        let clean = sanitize_skill_name(name);
        let all = self.list_all_skills();

        all.into_iter()
            .find(|s| s.name.eq_ignore_ascii_case(&clean))
            .ok_or_else(|| VaultError::NotFound(name.to_string()))
    }

    /// Lists skills filtered by optional scope.
    pub fn list_by_scope(&self, scope: Option<SkillScope>) -> Vec<VaultSkill> {
        let all = self.list_all_skills();
        match scope {
            Some(s) => all.into_iter().filter(|k| k.scope == s).collect(),
            None => all,
        }
    }

    /// Searches skills by keyword, matching name, triggers, description, and globs.
    pub fn search_skills(&self, query: &str) -> Vec<VaultSkill> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.list_all_skills();
        }

        let mut all = self.list_all_skills();

        all.retain(|s| {
            s.name.to_lowercase().contains(&q)
                || s.description.to_lowercase().contains(&q)
                || s.frontmatter
                    .triggers
                    .iter()
                    .any(|t| t.to_lowercase().contains(&q))
                || s.frontmatter
                    .globs
                    .iter()
                    .any(|g| g.to_lowercase().contains(&q))
        });

        // Sort by exact name match first, then trigger match, then description
        all.sort_by(|a, b| {
            let a_exact = a.name.to_lowercase() == q;
            let b_exact = b.name.to_lowercase() == q;
            if a_exact != b_exact {
                return b_exact.cmp(&a_exact);
            }
            let a_name_starts = a.name.to_lowercase().starts_with(&q);
            let b_name_starts = b.name.to_lowercase().starts_with(&q);
            if a_name_starts != b_name_starts {
                return b_name_starts.cmp(&a_name_starts);
            }
            a.name.cmp(&b.name)
        });

        all
    }

    /// Creates a new skill in the target scope (`Project` or `Global`).
    pub fn create_skill(
        &self,
        scope: SkillScope,
        name: &str,
        description: &str,
        instructions: &str,
        triggers: Vec<String>,
        globs: Vec<String>,
    ) -> Result<VaultSkill> {
        let clean_name = sanitize_skill_name(name);
        if clean_name.is_empty() {
            return Err(VaultError::InvalidSkill(
                "Skill name cannot be empty".to_string(),
            ));
        }

        let target_dir = match scope {
            SkillScope::Project => self.project_skills_dir().join(&clean_name),
            SkillScope::Global => {
                let dir = Self::global_skills_dir().ok_or_else(|| {
                    VaultError::InvalidSkill(
                        "Cannot resolve user home directory for global vault".to_string(),
                    )
                })?;
                dir.join(&clean_name)
            }
            SkillScope::Builtin => {
                return Err(VaultError::InvalidSkill(
                    "Cannot create custom skills in immutable Builtin scope".to_string(),
                ));
            }
        };

        fs::create_dir_all(&target_dir).map_err(|e| VaultError::Io {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        let fm = SkillFrontmatter {
            name: clean_name.clone(),
            description: description.trim().to_string(),
            version: "1.0.0".to_string(),
            author: Some("minicode".to_string()),
            globs,
            triggers,
            always_apply: false,
            allowed_tools: Vec::new(),
        };

        let content = format_skill_markdown(&fm, instructions);
        let target_file = target_dir.join("SKILL.md");

        fs::write(&target_file, &content).map_err(|e| VaultError::Io {
            path: target_file.display().to_string(),
            source: e,
        })?;

        if scope == SkillScope::Project {
            let _ = self.add_to_manifest_active_skills(&clean_name);
        }

        Ok(VaultSkill {
            name: clean_name,
            description: fm.description.clone(),
            scope,
            frontmatter: fm,
            instructions: instructions.trim().to_string(),
            raw_content: content,
            path: Some(target_file),
            is_active_in_project: scope == SkillScope::Project,
        })
    }

    /// Loads a skill from Built-in or Global vault into the active Project (`.minicode/skills/<name>/SKILL.md`).
    pub fn load_to_project(&self, name: &str) -> Result<VaultSkill> {
        let clean_name = sanitize_skill_name(name);
        let source_skill = self.get_skill(&clean_name)?;

        // If it's already a project-level skill, make sure it is marked active in manifest
        if source_skill.scope == SkillScope::Project {
            let _ = self.add_to_manifest_active_skills(&clean_name);
            let mut s = source_skill;
            s.is_active_in_project = true;
            return Ok(s);
        }

        // Install into .minicode/skills/<name>/SKILL.md
        let target_dir = self.project_skills_dir().join(&clean_name);
        fs::create_dir_all(&target_dir).map_err(|e| VaultError::Io {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        let target_file = target_dir.join("SKILL.md");
        fs::write(&target_file, &source_skill.raw_content).map_err(|e| VaultError::Io {
            path: target_file.display().to_string(),
            source: e,
        })?;

        let _ = self.add_to_manifest_active_skills(&clean_name);

        Ok(VaultSkill {
            name: source_skill.name,
            description: source_skill.description,
            scope: SkillScope::Project,
            frontmatter: source_skill.frontmatter,
            instructions: source_skill.instructions,
            raw_content: source_skill.raw_content,
            path: Some(target_file),
            is_active_in_project: true,
        })
    }

    /// Unloads/removes a skill from the active Project workspace.
    pub fn unload_from_project(&self, name: &str) -> Result<()> {
        let clean = sanitize_skill_name(name);

        let target_dir = self.project_skills_dir().join(&clean);
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir).map_err(|e| VaultError::Io {
                path: target_dir.display().to_string(),
                source: e,
            })?;
        }

        let target_file = self.project_skills_dir().join(format!("{}.md", clean));
        if target_file.exists() {
            let _ = fs::remove_file(&target_file);
        }

        let docs_file = self.project_docs_skills_dir().join(format!("{}.md", clean));
        if docs_file.exists() {
            let _ = fs::remove_file(&docs_file);
        }

        let _ = self.remove_from_manifest_active_skills(&clean);
        Ok(())
    }

    /// Updates the instructions or content of an existing skill.
    pub fn update_skill(
        &self,
        scope: SkillScope,
        name: &str,
        new_instructions: &str,
    ) -> Result<VaultSkill> {
        let clean = sanitize_skill_name(name);
        let existing = self.get_skill(&clean)?;

        let target_path = match &existing.path {
            Some(p) => p.clone(),
            None => {
                // If it's builtin, update creates a project copy
                return self.create_skill(
                    scope,
                    &existing.name,
                    &existing.description,
                    new_instructions,
                    existing.frontmatter.triggers,
                    existing.frontmatter.globs,
                );
            }
        };

        let new_content = format_skill_markdown(&existing.frontmatter, new_instructions);
        fs::write(&target_path, &new_content).map_err(|e| VaultError::Io {
            path: target_path.display().to_string(),
            source: e,
        })?;

        Ok(VaultSkill {
            name: existing.name,
            description: existing.description,
            scope,
            frontmatter: existing.frontmatter,
            instructions: new_instructions.trim().to_string(),
            raw_content: new_content,
            path: Some(target_path),
            is_active_in_project: scope == SkillScope::Project,
        })
    }

    /// Deletes a skill completely from the specified scope.
    pub fn delete_skill(&self, scope: SkillScope, name: &str) -> Result<()> {
        let clean = sanitize_skill_name(name);

        match scope {
            SkillScope::Project => self.unload_from_project(&clean),
            SkillScope::Global => {
                let dir = Self::global_skills_dir().ok_or_else(|| {
                    VaultError::InvalidSkill("Cannot resolve global vault dir".to_string())
                })?;
                let target_dir = dir.join(&clean);
                if target_dir.exists() {
                    fs::remove_dir_all(&target_dir).map_err(|e| VaultError::Io {
                        path: target_dir.display().to_string(),
                        source: e,
                    })?;
                }
                let target_file = dir.join(format!("{}.md", clean));
                if target_file.exists() {
                    let _ = fs::remove_file(&target_file);
                }
                Ok(())
            }
            SkillScope::Builtin => Err(VaultError::InvalidSkill(
                "Cannot delete immutable built-in skills".to_string(),
            )),
        }
    }

    /// Fetches a skill definition from an HTTP URL and imports it into the target scope.
    pub async fn import_from_url(
        &self,
        url: &str,
        target_scope: SkillScope,
        name_override: Option<&str>,
    ) -> Result<VaultSkill> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| VaultError::Network {
                url: url.to_string(),
                reason: e.to_string(),
            })?;

        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| VaultError::Network {
                url: url.to_string(),
                reason: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(VaultError::Network {
                url: url.to_string(),
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let body = resp.text().await.map_err(|e| VaultError::Network {
            url: url.to_string(),
            reason: e.to_string(),
        })?;

        let fallback_name = name_override.unwrap_or_else(|| {
            url.split('/')
                .next_back()
                .unwrap_or("imported-skill")
                .trim_end_matches(".md")
        });

        let (mut fm, instructions) = parse_skill_markdown(&body, fallback_name);
        if let Some(override_name) = name_override {
            fm.name = sanitize_skill_name(override_name);
        }

        self.create_skill(
            target_scope,
            &fm.name,
            &fm.description,
            &instructions,
            fm.triggers,
            fm.globs,
        )
    }

    /// Resolves `active_skills` list from `minikit.json` or `onpkg.json`.
    pub fn get_manifest_active_skills(&self) -> Vec<String> {
        let mut list = Vec::new();
        for fname in &["minikit.json", "onpkg.json"] {
            let manifest_path = self.workspace_root.join(fname);
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(arr) = val.get("active_skills").and_then(|a| a.as_array()) {
                        for s in arr {
                            if let Some(name) = s.as_str() {
                                let trimmed = name.trim().to_lowercase();
                                if !trimmed.is_empty() && !list.contains(&trimmed) {
                                    list.push(trimmed);
                                }
                            }
                        }
                    }
                }
            }
        }
        list
    }

    /// Adds a skill to `active_skills` in the workspace manifest.
    fn add_to_manifest_active_skills(&self, skill_name: &str) -> Result<()> {
        let manifest_file = if self.workspace_root.join("minikit.json").exists() {
            Some(self.workspace_root.join("minikit.json"))
        } else if self.workspace_root.join("onpkg.json").exists() {
            Some(self.workspace_root.join("onpkg.json"))
        } else {
            None
        };

        if let Some(path) = manifest_file {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(arr) = val.get_mut("active_skills").and_then(|a| a.as_array_mut()) {
                        let name_val = serde_json::Value::String(skill_name.to_lowercase());
                        if !arr.contains(&name_val) {
                            arr.push(name_val);
                            if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                                let _ = fs::write(&path, pretty);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Removes a skill from `active_skills` in the workspace manifest.
    fn remove_from_manifest_active_skills(&self, skill_name: &str) -> Result<()> {
        let manifest_file = if self.workspace_root.join("minikit.json").exists() {
            Some(self.workspace_root.join("minikit.json"))
        } else if self.workspace_root.join("onpkg.json").exists() {
            Some(self.workspace_root.join("onpkg.json"))
        } else {
            None
        };

        if let Some(path) = manifest_file {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(arr) = val.get_mut("active_skills").and_then(|a| a.as_array_mut()) {
                        arr.retain(|s| {
                            s.as_str().map(|n| n.to_lowercase()) != Some(skill_name.to_lowercase())
                        });
                        if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                            let _ = fs::write(&path, pretty);
                        }
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
    use tempfile::tempdir;

    #[test]
    fn test_vault_store_crud_lifecycle() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let store = VaultStore::new(ws);

        // 1. List contains built-in skills
        let list = store.list_all_skills();
        assert!(list.len() >= 15);
        assert!(list.iter().any(|s| s.name == "tailwind-v4"));

        // 2. Load built-in skill to project
        let loaded = store.load_to_project("tailwind-v4").unwrap();
        assert_eq!(loaded.scope, SkillScope::Project);
        assert!(loaded.is_active_in_project);
        assert!(store.project_skills_dir().join("tailwind-v4").exists());

        // 3. Create custom project skill
        let custom = store
            .create_skill(
                SkillScope::Project,
                "custom-graphql",
                "Project specific GraphQL guidelines",
                "Always use Apollo Client with cache-and-network.",
                vec!["graphql".to_string()],
                vec!["*.graphql".to_string()],
            )
            .unwrap();

        assert_eq!(custom.name, "custom-graphql");
        assert_eq!(custom.scope, SkillScope::Project);

        // 4. Search
        let search_res = store.search_skills("graphql");
        assert_eq!(search_res.len(), 1);
        assert_eq!(search_res[0].name, "custom-graphql");

        // 5. Update
        let updated = store
            .update_skill(
                SkillScope::Project,
                "custom-graphql",
                "Updated: Use URQL instead of Apollo.",
            )
            .unwrap();
        assert!(updated.instructions.contains("Use URQL instead of Apollo."));

        // 6. Unload from project
        store.unload_from_project("custom-graphql").unwrap();
        assert!(store.get_skill("custom-graphql").is_err());
    }
}
