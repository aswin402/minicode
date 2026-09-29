//! MiniVault storage, multi-tier discovery, and CRUD engine.

use crate::vault::builtin::{get_all_builtin_bundles, get_all_builtin_skills};
use crate::vault::models::{
    format_skill_markdown, parse_skill_markdown, sanitize_skill_name, GlobalGotcha, LearnedSource,
    Result, SkillFrontmatter, SkillScope, SourceKind, VaultBundle, VaultError, VaultSkill,
};
use std::fs;
use std::path::{Path, PathBuf};

/// Multi-tier skills vault manager for minicode.
#[derive(Debug, Clone)]
pub struct VaultStore {
    workspace_root: PathBuf,
    custom_global_vault_dir: Option<PathBuf>,
}

impl VaultStore {
    /// Creates a new `VaultStore` for the given workspace.
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            custom_global_vault_dir: None,
        }
    }

    /// Sets an explicit custom directory for the global vault (used for testing or sandboxing).
    pub fn with_custom_global_vault(mut self, path: PathBuf) -> Self {
        self.custom_global_vault_dir = Some(path);
        self
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

    /// Global user vault base directory: `~/.config/minicode/vault` (or custom/env override)
    pub fn global_vault_dir(&self) -> Option<PathBuf> {
        if let Some(ref custom) = self.custom_global_vault_dir {
            return Some(custom.clone());
        }
        if let Ok(override_dir) = std::env::var("MINICODE_VAULT_DIR") {
            if !override_dir.trim().is_empty() {
                return Some(PathBuf::from(override_dir.trim()));
            }
        }
        dirs::home_dir().map(|home| {
            home.join(".config")
                .join(crate::constants::CONFIG_DIR_NAME)
                .join("vault")
        })
    }

    /// Global user skills directory: `~/.config/minicode/vault/skills`
    pub fn global_skills_dir(&self) -> Option<PathBuf> {
        self.global_vault_dir().map(|v| v.join("skills"))
    }

    /// Legacy global skills directory: `~/.config/minicode/skills`
    pub fn legacy_global_skills_dir(&self) -> Option<PathBuf> {
        if self.custom_global_vault_dir.is_some() {
            return None;
        }
        dirs::home_dir().map(|home| {
            home.join(".config")
                .join(crate::constants::CONFIG_DIR_NAME)
                .join("skills")
        })
    }

    /// Primary project bundles directory: `.minicode/bundles`
    pub fn project_bundles_dir(&self) -> PathBuf {
        self.workspace_root.join(".minicode").join("bundles")
    }

    /// Global user bundles directory: `~/.config/minicode/vault/bundles`
    pub fn global_bundles_dir(&self) -> Option<PathBuf> {
        self.global_vault_dir().map(|v| v.join("bundles"))
    }

    /// Global learned sources file: `~/.config/minicode/vault/sources.json`
    pub fn global_sources_file(&self) -> Option<PathBuf> {
        self.global_vault_dir().map(|d| d.join("sources.json"))
    }

    /// Global learned gotchas file: `~/.config/minicode/vault/gotchas.json`
    pub fn global_gotchas_file(&self) -> Option<PathBuf> {
        self.global_vault_dir().map(|d| d.join("gotchas.json"))
    }

    /// Loads all bookmarked learned sources from `sources.json`.
    pub fn load_sources(&self) -> Vec<LearnedSource> {
        let Some(path) = self.global_sources_file() else {
            return Vec::new();
        };
        if !path.exists() {
            return Vec::new();
        }
        let Ok(data) = fs::read_to_string(&path) else {
            return Vec::new();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    /// Saves the learned sources array to `sources.json`.
    pub fn save_sources(&self, sources: &[LearnedSource]) -> Result<()> {
        let Some(path) = self.global_sources_file() else {
            return Err(VaultError::InvalidSkill(
                "Cannot resolve global vault directory".to_string(),
            ));
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| VaultError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        let content = serde_json::to_string_pretty(sources)
            .map_err(|e| VaultError::InvalidSkill(e.to_string()))?;
        fs::write(&path, content).map_err(|e| VaultError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Ok(())
    }

    /// Adds or updates a learned source by URI.
    pub fn add_or_update_source(&self, source: LearnedSource) -> Result<()> {
        let mut sources = self.load_sources();
        if let Some(existing) = sources
            .iter_mut()
            .find(|s| s.uri.eq_ignore_ascii_case(&source.uri))
        {
            existing.title = source.title;
            existing.summary = source.summary;
            existing.tags = source.tags;
            existing.last_referenced = chrono::Utc::now().to_rfc3339();
            if source.extracted_skill_or_doc.is_some() {
                existing.extracted_skill_or_doc = source.extracted_skill_or_doc;
            }
        } else {
            sources.push(source);
        }
        self.save_sources(&sources)
    }

    /// Searches bookmarked sources.
    pub fn search_sources(&self, query: &str) -> Vec<LearnedSource> {
        let sources = self.load_sources();
        if query.trim().is_empty() {
            return sources;
        }
        sources
            .into_iter()
            .filter(|s| s.matches_query(query))
            .collect()
    }

    /// Loads universal cross-project learned gotchas from `gotchas.json`.
    pub fn load_global_gotchas(&self) -> Vec<GlobalGotcha> {
        let Some(path) = self.global_gotchas_file() else {
            return Vec::new();
        };
        if !path.exists() {
            return Vec::new();
        }
        let Ok(data) = fs::read_to_string(&path) else {
            return Vec::new();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    /// Saves universal gotchas to `gotchas.json`.
    pub fn save_global_gotchas(&self, gotchas: &[GlobalGotcha]) -> Result<()> {
        let Some(path) = self.global_gotchas_file() else {
            return Err(VaultError::InvalidSkill(
                "Cannot resolve global vault directory".to_string(),
            ));
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| VaultError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        let content = serde_json::to_string_pretty(gotchas)
            .map_err(|e| VaultError::InvalidSkill(e.to_string()))?;
        fs::write(&path, content).map_err(|e| VaultError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Ok(())
    }

    /// Adds or updates a universal gotcha. Caps at 50 to prevent unbounded bloat.
    pub fn add_or_update_global_gotcha(&self, mut gotcha: GlobalGotcha) -> Result<()> {
        let mut gotchas = self.load_global_gotchas();
        let trigger_lower = gotcha.trigger.trim().to_lowercase();
        let scope_lower = gotcha.context_scope.trim().to_lowercase();

        if let Some(existing) = gotchas.iter_mut().find(|g| {
            g.context_scope.eq_ignore_ascii_case(&scope_lower)
                && (g.trigger.eq_ignore_ascii_case(&trigger_lower)
                    || g.trigger.to_lowercase().contains(&trigger_lower)
                    || trigger_lower.contains(&g.trigger.to_lowercase()))
        }) {
            existing.failed_attempt = gotcha.failed_attempt;
            existing.verified_fix = gotcha.verified_fix;
            existing.occurrence_count = existing.occurrence_count.saturating_add(1);
        } else {
            gotcha.occurrence_count = 1;
            gotchas.push(gotcha);
        }

        // Keep at most 50 gotchas (sorted by occurrence_count desc, then created_at desc)
        if gotchas.len() > 50 {
            gotchas.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));
            gotchas.truncate(50);
        }

        self.save_global_gotchas(&gotchas)
    }

    /// Finds relevant universal gotchas matching query, framework, or context keywords.
    pub fn find_relevant_gotchas(&self, query_or_tech: &str) -> Vec<GlobalGotcha> {
        let gotchas = self.load_global_gotchas();
        if query_or_tech.trim().is_empty() {
            return gotchas.into_iter().take(5).collect();
        }
        let lower = query_or_tech.to_lowercase();
        let mut matched: Vec<_> = gotchas
            .into_iter()
            .filter(|g| g.matches_query(&lower) || lower.contains(&g.context_scope.to_lowercase()))
            .collect();
        matched.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));
        matched.truncate(5);
        matched
    }

    /// Ingests an external source (HTTP URL or local directory/repo), registers it in `sources.json`,
    /// and synthesizes a corresponding `[Doc]` or `[Skill]` in the target vault scope.
    pub async fn ingest_source(
        &self,
        uri: &str,
        title_override: Option<&str>,
        target_scope: SkillScope,
        custom_instructions: Option<&str>,
    ) -> Result<(LearnedSource, Option<VaultSkill>)> {
        let uri = uri.trim();
        let now = chrono::Utc::now().to_rfc3339();

        if uri.starts_with("http://") || uri.starts_with("https://") {
            // Web source
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .map_err(|e| VaultError::Network {
                    url: uri.to_string(),
                    reason: e.to_string(),
                })?;

            let resp = client
                .get(uri)
                .send()
                .await
                .map_err(|e| VaultError::Network {
                    url: uri.to_string(),
                    reason: e.to_string(),
                })?;

            if !resp.status().is_success() {
                return Err(VaultError::Network {
                    url: uri.to_string(),
                    reason: format!("HTTP {}", resp.status()),
                });
            }

            let body = resp.text().await.map_err(|e| VaultError::Network {
                url: uri.to_string(),
                reason: e.to_string(),
            })?;

            // Derive name and title
            let fallback_name = uri
                .split('/')
                .rfind(|s| !s.is_empty())
                .unwrap_or("web-source")
                .trim_end_matches(".html")
                .trim_end_matches(".md");
            let clean_name = sanitize_skill_name(title_override.unwrap_or(fallback_name));
            let title = title_override.unwrap_or(fallback_name).to_string();

            // Check if body is raw markdown or HTML
            let instructions = if let Some(custom) = custom_instructions {
                custom.to_string()
            } else if body.starts_with("---") || uri.ends_with(".md") {
                let (_, parsed_body) = parse_skill_markdown(&body, &clean_name);
                parsed_body
            } else {
                // Basic HTML clean-up: extract meaningful text lines
                let text_lines: Vec<_> = body
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty() && !l.starts_with('<') && !l.starts_with("//"))
                    .take(150)
                    .collect();
                if text_lines.is_empty() {
                    format!(
                        "# Documentation for {}\n\nSource: {}\n\nRefer to {}",
                        title, uri, uri
                    )
                } else {
                    format!(
                        "# Documentation for {}\n\nSource: {}\n\n{}",
                        title,
                        uri,
                        text_lines.join("\n")
                    )
                }
            };

            let skill = self.create_skill(
                target_scope,
                &clean_name,
                &format!("Ingested reference documentation for {}", title),
                Some("reference"),
                Some("reference"),
                &instructions,
                vec![clean_name.clone()],
                Vec::new(),
            )?;

            let source = LearnedSource {
                id: format!("src-{}", uuid::Uuid::new_v4().simple()),
                uri: uri.to_string(),
                title,
                kind: SourceKind::WebDoc,
                summary: format!("Documentation for {}", clean_name),
                tags: vec![clean_name.clone(), "web".to_string()],
                extracted_skill_or_doc: Some(clean_name),
                created_at: now.clone(),
                last_referenced: now,
            };

            self.add_or_update_source(source.clone())?;
            Ok((source, Some(skill)))
        } else {
            // Local path / repo template
            let path = Path::new(uri);
            if !path.exists() {
                return Err(VaultError::InvalidSkill(format!(
                    "Path '{}' does not exist",
                    uri
                )));
            }

            let fallback_name = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("local-repo");
            let clean_name = sanitize_skill_name(title_override.unwrap_or(fallback_name));
            let title = title_override.unwrap_or(fallback_name).to_string();

            let instructions = if let Some(custom) = custom_instructions {
                custom.to_string()
            } else {
                let readme_path = path.join("README.md");
                let alt_readme = path.join("readme.md");
                let readme_content = if readme_path.exists() {
                    fs::read_to_string(&readme_path).ok()
                } else if alt_readme.exists() {
                    fs::read_to_string(&alt_readme).ok()
                } else {
                    None
                };

                let readme_snippet = readme_content
                    .as_deref()
                    .map(|c| c.lines().take(60).collect::<Vec<_>>().join("\n"))
                    .unwrap_or_else(|| "No README found in repository.".to_string());

                format!(
                    "# Architectural Reference: {}\n\nLocal Source: `{}`\n\n## Overview\n{}\n",
                    title,
                    path.display(),
                    readme_snippet
                )
            };

            let skill = self.create_skill(
                target_scope,
                &clean_name,
                &format!("Reference architecture pattern from {}", title),
                Some("architecture"),
                Some("workflow"),
                &instructions,
                vec![clean_name.clone()],
                Vec::new(),
            )?;

            let source = LearnedSource {
                id: format!("src-{}", uuid::Uuid::new_v4().simple()),
                uri: uri.to_string(),
                title,
                kind: SourceKind::RepoTemplate,
                summary: format!("Local reference repository at {}", path.display()),
                tags: vec![clean_name.clone(), "repo".to_string()],
                extracted_skill_or_doc: Some(clean_name),
                created_at: now.clone(),
                last_referenced: now,
            };

            self.add_or_update_source(source.clone())?;
            Ok((source, Some(skill)))
        }
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
        if let Some(global_dir) = self.global_skills_dir() {
            self.scan_directory_into_map(&global_dir, SkillScope::Global, &mut skill_map);
        }
        if let Some(legacy_dir) = self.legacy_global_skills_dir() {
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

    /// Searches skills by keyword, matching name, triggers, description, globs, and category.
    pub fn search_skills(&self, query: &str) -> Vec<VaultSkill> {
        self.search_skills_categorized(query, None)
    }

    /// Searches skills with optional category filtering and keyword matching.
    pub fn search_skills_categorized(
        &self,
        query: &str,
        category: Option<&str>,
    ) -> Vec<VaultSkill> {
        let q = query.trim().to_lowercase();
        let cat_filter = category.map(|c| c.trim().to_lowercase());

        let mut all = self.list_all_skills();

        if let Some(ref cat) = cat_filter {
            if !cat.is_empty() {
                all.retain(|s| s.category().eq_ignore_ascii_case(cat));
            }
        }

        if q.is_empty() {
            return all;
        }

        all.retain(|s| {
            s.name.to_lowercase().contains(&q)
                || s.description.to_lowercase().contains(&q)
                || s.category().to_lowercase().contains(&q)
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
    #[allow(clippy::too_many_arguments)]
    pub fn create_skill(
        &self,
        scope: SkillScope,
        name: &str,
        description: &str,
        category: Option<&str>,
        kind: Option<&str>,
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
                let dir = self.global_skills_dir().ok_or_else(|| {
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
            category: category.map(|c| c.trim().to_lowercase()),
            kind: kind.map(|k| k.trim().to_lowercase()),
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
                    existing.frontmatter.category.as_deref(),
                    existing.frontmatter.kind.as_deref(),
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
                let dir = self.global_skills_dir().ok_or_else(|| {
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
            fm.category.as_deref(),
            fm.kind.as_deref(),
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

    // =========================================================================
    // MiniVault Bundles Engine (Skill Packs / Stacks)
    // =========================================================================

    /// Discovers all skill bundles across all tiers (Built-in, Global, Project).
    pub fn list_all_bundles(&self) -> Vec<VaultBundle> {
        let mut bundle_map: std::collections::BTreeMap<String, VaultBundle> =
            std::collections::BTreeMap::new();

        // 1. Built-in Core Bundles (Tier 1)
        for b in get_all_builtin_bundles() {
            bundle_map.insert(b.name.to_lowercase(), b);
        }

        // 2. Global User Bundles (Tier 2)
        if let Some(global_dir) = self.global_bundles_dir() {
            self.scan_bundles_directory(&global_dir, SkillScope::Global, &mut bundle_map);
        }

        // 3. Project Bundles (Tier 3)
        let proj_dir = self.project_bundles_dir();
        self.scan_bundles_directory(&proj_dir, SkillScope::Project, &mut bundle_map);

        bundle_map.into_values().collect()
    }

    fn scan_bundles_directory(
        &self,
        dir: &Path,
        scope: SkillScope,
        map: &mut std::collections::BTreeMap<String, VaultBundle>,
    ) {
        if !dir.exists() {
            return;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut bundle) = serde_json::from_str::<VaultBundle>(&content) {
                            bundle.scope = scope;
                            bundle.path = Some(path);
                            map.insert(bundle.name.to_lowercase(), bundle);
                        }
                    }
                }
            }
        }
    }

    /// Finds a bundle by name across all tiers.
    pub fn get_bundle(&self, name: &str) -> Option<VaultBundle> {
        let clean = name.trim().to_lowercase();
        self.list_all_bundles()
            .into_iter()
            .find(|b| b.name.eq_ignore_ascii_case(&clean))
    }

    /// Loads all skills in a bundle into the active project (.minicode/skills/).
    /// Returns (loaded_skills, failed_skills).
    pub fn load_bundle_to_project(&self, bundle_name: &str) -> Result<(Vec<String>, Vec<String>)> {
        let bundle = self.get_bundle(bundle_name).ok_or_else(|| {
            VaultError::NotFound(format!("Bundle '{}' not found in vault", bundle_name))
        })?;

        let mut loaded = Vec::new();
        let mut failed = Vec::new();

        for skill_name in &bundle.skills {
            match self.load_to_project(skill_name) {
                Ok(_) => loaded.push(skill_name.clone()),
                Err(_) => failed.push(skill_name.clone()),
            }
        }

        Ok((loaded, failed))
    }

    /// Unloads all skills in a bundle from the active project.
    pub fn unload_bundle_from_project(&self, bundle_name: &str) -> Result<Vec<String>> {
        let bundle = self.get_bundle(bundle_name).ok_or_else(|| {
            VaultError::NotFound(format!("Bundle '{}' not found in vault", bundle_name))
        })?;

        let mut unloaded = Vec::new();
        for skill_name in &bundle.skills {
            if self.unload_from_project(skill_name).is_ok() {
                unloaded.push(skill_name.clone());
            }
        }
        Ok(unloaded)
    }

    /// Creates and saves a custom bundle definition as JSON in Project or Global scope.
    pub fn create_bundle(
        &self,
        scope: SkillScope,
        name: &str,
        description: &str,
        category: &str,
        skills: Vec<String>,
        tags: Vec<String>,
    ) -> Result<VaultBundle> {
        let clean_name = sanitize_skill_name(name);
        if clean_name.is_empty() {
            return Err(VaultError::InvalidSkill(
                "Bundle name cannot be empty".to_string(),
            ));
        }

        let target_dir = match scope {
            SkillScope::Project => self.project_bundles_dir(),
            SkillScope::Global => self.global_bundles_dir().ok_or_else(|| {
                VaultError::InvalidSkill(
                    "Cannot resolve user home directory for global vault".to_string(),
                )
            })?,
            SkillScope::Builtin => {
                return Err(VaultError::InvalidSkill(
                    "Cannot create custom bundles in immutable Builtin scope".to_string(),
                ));
            }
        };

        fs::create_dir_all(&target_dir).map_err(|e| VaultError::Io {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        let file_path = target_dir.join(format!("{}.json", clean_name));
        let bundle = VaultBundle {
            name: clean_name.clone(),
            description: description.trim().to_string(),
            category: if category.trim().is_empty() {
                "general".to_string()
            } else {
                category.trim().to_lowercase()
            },
            skills,
            tags,
            scope,
            path: Some(file_path.clone()),
        };

        let json = serde_json::to_string_pretty(&bundle)
            .map_err(|e| VaultError::InvalidSkill(format!("Failed to serialize bundle: {}", e)))?;

        fs::write(&file_path, json).map_err(|e| VaultError::Io {
            path: file_path.display().to_string(),
            source: e,
        })?;

        Ok(bundle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::models::SkillKind;
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
                Some("api"),
                Some("reference"),
                "Always use Apollo Client with cache-and-network.",
                vec!["graphql".to_string()],
                vec!["*.graphql".to_string()],
            )
            .unwrap();

        assert_eq!(custom.name, "custom-graphql");
        assert_eq!(custom.scope, SkillScope::Project);
        assert_eq!(custom.category(), "api");
        assert_eq!(custom.kind(), SkillKind::Reference);

        // 4. Search
        let search_res = store.search_skills("graphql");
        assert_eq!(search_res.len(), 1);
        assert_eq!(search_res[0].name, "custom-graphql");

        let search_cat = store.search_skills_categorized("", Some("api"));
        assert_eq!(search_cat.len(), 1);
        assert_eq!(search_cat[0].name, "custom-graphql");

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

    #[test]
    fn test_vault_store_bundles_lifecycle() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let store = VaultStore::new(ws);

        // 1. List includes built-in bundles
        let bundles = store.list_all_bundles();
        assert!(bundles.len() >= 6);
        assert!(bundles.iter().any(|b| b.name == "fullstack-nextjs"));

        // 2. Load bundle to project
        let (loaded, failed) = store.load_bundle_to_project("fullstack-nextjs").unwrap();
        assert!(failed.is_empty());
        assert!(loaded.contains(&"react".to_string()));
        assert!(loaded.contains(&"nextjs".to_string()));
        assert!(loaded.contains(&"tailwind-v4".to_string()));

        // Check project skills dir
        assert!(store.project_skills_dir().join("react").exists());
        assert!(store.project_skills_dir().join("nextjs").exists());

        // 3. Create custom bundle
        let custom_bundle = store
            .create_bundle(
                SkillScope::Project,
                "custom-stack",
                "Custom team stack",
                "team",
                vec!["react".to_string(), "tailwind-v4".to_string()],
                vec!["custom".to_string()],
            )
            .unwrap();

        assert_eq!(custom_bundle.name, "custom-stack");
        assert_eq!(custom_bundle.scope, SkillScope::Project);
        assert!(store.get_bundle("custom-stack").is_some());

        // 4. Unload bundle
        let unloaded = store
            .unload_bundle_from_project("fullstack-nextjs")
            .unwrap();
        assert!(unloaded.contains(&"react".to_string()));
    }

    #[test]
    fn test_vault_sources_and_gotchas_lifecycle() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let store = VaultStore::new(ws).with_custom_global_vault(ws.join("test_global_vault"));

        // 1. Sources CRUD
        let src = LearnedSource {
            id: "src-1".to_string(),
            uri: "https://docs.rs/tokio".to_string(),
            title: "Tokio Async Runtime".to_string(),
            kind: SourceKind::WebDoc,
            summary: "Tokio async documentation".to_string(),
            tags: vec!["tokio".to_string(), "rust".to_string(), "async".to_string()],
            extracted_skill_or_doc: Some("tokio".to_string()),
            created_at: "2026-09-29T10:00:00Z".to_string(),
            last_referenced: "2026-09-29T10:00:00Z".to_string(),
        };

        store.add_or_update_source(src.clone()).unwrap();
        let sources = store.load_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "Tokio Async Runtime");

        let search_res = store.search_sources("tokio");
        assert_eq!(search_res.len(), 1);
        let search_empty = store.search_sources("nonexistent");
        assert!(search_empty.is_empty());

        // 2. Gotchas CRUD and Occurrence Bumping
        let gotcha = GlobalGotcha {
            id: "gotcha-1".to_string(),
            trigger: "tokio::spawn inside non-async scope".to_string(),
            context_scope: "Rust/Tokio".to_string(),
            failed_attempt: "Calling tokio::spawn without runtime handle".to_string(),
            verified_fix: "Use tokio::runtime::Handle::current().spawn or mark fn async"
                .to_string(),
            occurrence_count: 1,
            created_at: "2026-09-29T10:00:00Z".to_string(),
        };

        store.add_or_update_global_gotcha(gotcha.clone()).unwrap();
        let gotchas = store.load_global_gotchas();
        assert_eq!(gotchas.len(), 1);
        assert_eq!(gotchas[0].occurrence_count, 1);

        // Add the exact same trigger again to test automatic occurrence increment
        let duplicate_gotcha = GlobalGotcha {
            id: "gotcha-2".to_string(),
            trigger: "tokio::spawn inside non-async scope".to_string(),
            context_scope: "Rust/Tokio".to_string(),
            failed_attempt: "Another failure".to_string(),
            verified_fix: "Updated fix".to_string(),
            occurrence_count: 1,
            created_at: "2026-09-29T11:00:00Z".to_string(),
        };

        store.add_or_update_global_gotcha(duplicate_gotcha).unwrap();
        let updated_gotchas = store.load_global_gotchas();
        assert_eq!(updated_gotchas.len(), 1);
        assert_eq!(updated_gotchas[0].occurrence_count, 2);
        assert_eq!(updated_gotchas[0].verified_fix, "Updated fix");

        // Test relevant gotchas retrieval
        let relevant = store.find_relevant_gotchas("tokio async concurrency");
        assert_eq!(relevant.len(), 1);
        assert_eq!(relevant[0].occurrence_count, 2);

        let irrelevant = store.find_relevant_gotchas("react tailwind css");
        assert!(irrelevant.is_empty());
    }

    #[tokio::test]
    async fn test_vault_ingest_local_repo() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let store = VaultStore::new(ws).with_custom_global_vault(ws.join("test_global_vault"));

        // Create a fake local repo
        let repo_dir = dir.path().join("fake-repo");
        fs::create_dir_all(&repo_dir).unwrap();
        let readme_content = "# Fake Architecture\n\nThis is a pattern repository.";
        fs::write(repo_dir.join("README.md"), readme_content).unwrap();

        let (src, skill) = store
            .ingest_source(
                &repo_dir.to_string_lossy(),
                Some("fake-arch"),
                SkillScope::Project,
                None,
            )
            .await
            .unwrap();

        assert_eq!(src.kind, SourceKind::RepoTemplate);
        assert_eq!(src.title, "fake-arch");
        assert!(skill.is_some());
        let s = skill.unwrap();
        assert_eq!(s.name, "fake-arch");
        assert!(s.instructions.contains("This is a pattern repository."));

        // Verify stored in sources.json
        let sources = store.load_sources();
        assert!(sources.iter().any(|s| s.title == "fake-arch"));
    }
}
