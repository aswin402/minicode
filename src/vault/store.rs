//! MiniVault storage, multi-tier discovery, and CRUD engine.

use crate::vault::builtin::{get_all_builtin_bundles, get_all_builtin_skills};
use crate::vault::models::{
    format_skill_markdown, parse_skill_markdown, sanitize_skill_name, GlobalGotcha, LearnedSource,
    Result, SkillFrontmatter, SkillKind, SkillScope, SourceKind, VaultBundle, VaultError,
    VaultSkill,
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

    /// Project-local learned gotchas fallback file: `.minicode/vault/gotchas.json`
    pub fn project_gotchas_file(&self) -> PathBuf {
        self.workspace_root
            .join(".minicode")
            .join("vault")
            .join("gotchas.json")
    }

    /// Project-local deleted skills tombstone file: `.minicode/vault/deleted_skills.json`
    pub fn project_deleted_skills_file(&self) -> PathBuf {
        self.workspace_root
            .join(".minicode")
            .join("vault")
            .join("deleted_skills.json")
    }

    /// Global deleted skills tombstone file: `~/.config/minicode/vault/deleted_skills.json`
    pub fn global_deleted_skills_file(&self) -> Option<PathBuf> {
        self.global_vault_dir()
            .map(|d| d.join("deleted_skills.json"))
    }

    fn read_tombstone_set(path: &Path) -> std::collections::HashSet<String> {
        if !path.exists() {
            return std::collections::HashSet::new();
        }
        let Ok(content) = fs::read_to_string(path) else {
            return std::collections::HashSet::new();
        };
        serde_json::from_str::<Vec<String>>(&content)
            .unwrap_or_default()
            .into_iter()
            .map(|s| sanitize_skill_name(&s))
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn write_tombstone_set(path: &Path, set: &std::collections::HashSet<String>) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| VaultError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        let mut sorted: Vec<String> = set.iter().cloned().collect();
        sorted.sort();
        let json = serde_json::to_string_pretty(&sorted)
            .map_err(|e| VaultError::InvalidSkill(e.to_string()))?;
        fs::write(path, json).map_err(|e| VaultError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Ok(())
    }

    /// Loads the set of deleted/suppressed skill names across project and global tombstones.
    pub fn load_deleted_skills(&self) -> std::collections::HashSet<String> {
        let mut deleted = Self::read_tombstone_set(&self.project_deleted_skills_file());
        if let Some(global_file) = self.global_deleted_skills_file() {
            deleted.extend(Self::read_tombstone_set(&global_file));
        }
        deleted
    }

    fn record_skill_tombstone(&self, scope: SkillScope, clean_name: &str) -> Result<()> {
        let target_file = match scope {
            SkillScope::Global => self
                .global_deleted_skills_file()
                .unwrap_or_else(|| self.project_deleted_skills_file()),
            SkillScope::Project | SkillScope::Builtin => self.project_deleted_skills_file(),
        };
        let mut set = Self::read_tombstone_set(&target_file);
        set.insert(clean_name.to_lowercase());
        Self::write_tombstone_set(&target_file, &set)
    }

    fn clear_skill_tombstone(&self, clean_name: &str) {
        let lower = clean_name.to_lowercase();
        let proj_file = self.project_deleted_skills_file();
        if proj_file.exists() {
            let mut set = Self::read_tombstone_set(&proj_file);
            if set.remove(&lower) {
                let _ = Self::write_tombstone_set(&proj_file, &set);
            }
        }
        if let Some(global_file) = self.global_deleted_skills_file() {
            if global_file.exists() {
                let mut set = Self::read_tombstone_set(&global_file);
                if set.remove(&lower) {
                    let _ = Self::write_tombstone_set(&global_file, &set);
                }
            }
        }
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

    /// Loads universal cross-project learned gotchas from `gotchas.json` (plus any workspace-local `.minicode/vault/gotchas.json`).
    pub fn load_global_gotchas(&self) -> Vec<GlobalGotcha> {
        let mut combined: Vec<GlobalGotcha> = Vec::new();
        if let Some(path) = self.global_gotchas_file() {
            if path.exists() {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(list) = serde_json::from_str::<Vec<GlobalGotcha>>(&data) {
                        combined.extend(list);
                    }
                }
            }
        }
        let proj_path = self.project_gotchas_file();
        if proj_path.exists() {
            if let Ok(data) = fs::read_to_string(&proj_path) {
                if let Ok(list) = serde_json::from_str::<Vec<GlobalGotcha>>(&data) {
                    for g in list {
                        if !combined
                            .iter()
                            .any(|c| c.trigger.eq_ignore_ascii_case(&g.trigger))
                        {
                            combined.push(g);
                        }
                    }
                }
            }
        }
        combined
    }

    /// Saves universal gotchas to `gotchas.json` (falling back to `.minicode/vault/gotchas.json` if global config is read-only/sandboxed).
    pub fn save_global_gotchas(&self, gotchas: &[GlobalGotcha]) -> Result<()> {
        let content = serde_json::to_string_pretty(gotchas)
            .map_err(|e| VaultError::InvalidSkill(e.to_string()))?;

        if let Some(path) = self.global_gotchas_file() {
            let can_write_global = path
                .parent()
                .map(|p| fs::create_dir_all(p).is_ok())
                .unwrap_or(false)
                && fs::write(&path, &content).is_ok();
            if can_write_global {
                return Ok(());
            }
        }

        let proj_path = self.project_gotchas_file();
        if let Some(parent) = proj_path.parent() {
            fs::create_dir_all(parent).map_err(|e| VaultError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        fs::write(&proj_path, content).map_err(|e| VaultError::Io {
            path: proj_path.display().to_string(),
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
        let deleted = self.load_deleted_skills();
        let mut skill_map: std::collections::BTreeMap<String, VaultSkill> =
            std::collections::BTreeMap::new();

        // 1. Built-in Core Skills (Tier 1)
        for skill in get_all_builtin_skills() {
            let lower = skill.name.to_lowercase();
            if !deleted.contains(&lower) {
                skill_map.insert(lower, skill);
            }
        }

        // 2. Global User Skills (Tier 2)
        if let Some(global_dir) = self.global_skills_dir() {
            self.scan_directory_into_map(
                &global_dir,
                SkillScope::Global,
                Some(SkillKind::Workflow),
                false,
                &mut skill_map,
            );
        }
        if let Some(legacy_dir) = self.legacy_global_skills_dir() {
            self.scan_directory_into_map(
                &legacy_dir,
                SkillScope::Global,
                Some(SkillKind::Workflow),
                false,
                &mut skill_map,
            );
        }

        // 3. Project Skills & Knowledge Domains (Tier 3 - highest precedence)
        // 3a. Primary project skills (.minicode/skills/)
        let project_dir = self.project_skills_dir();
        self.scan_directory_into_map(
            &project_dir,
            SkillScope::Project,
            Some(SkillKind::Workflow),
            true,
            &mut skill_map,
        );

        // 3b. Structured project documentation directory (minikit_docs/ or onpkg_docs/)
        let docs_dir = crate::tools::minikit::resolve_docs_dir(&self.workspace_root);
        if docs_dir.exists() {
            // Tier 3a: packages/ -> Package Reference Docs & Continuous Learning Base
            let packages_dir = docs_dir.join("packages");
            self.scan_directory_into_map(
                &packages_dir,
                SkillScope::Project,
                Some(SkillKind::Reference),
                false,
                &mut skill_map,
            );

            // Tier 3c: runtimes/ or stack subdirectories (e.g. rust/, python/, flutter/) -> Runtime Workflows
            let runtimes_dir = docs_dir.join("runtimes");
            self.scan_directory_into_map(
                &runtimes_dir,
                SkillScope::Project,
                Some(SkillKind::Runtime),
                false,
                &mut skill_map,
            );

            if let Ok(entries) = fs::read_dir(&docs_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        let dname = p.file_name().unwrap_or_default().to_string_lossy();
                        if dname != "skills"
                            && dname != "packages"
                            && dname != "core"
                            && dname != "runtimes"
                            && !dname.starts_with('.')
                        {
                            self.scan_directory_into_map(
                                &p,
                                SkillScope::Project,
                                Some(SkillKind::Runtime),
                                false,
                                &mut skill_map,
                            );
                        }
                    }
                }
            }

            // Tier 3d: core/ -> Core Invariants & Specs
            let core_dir = docs_dir.join("core");
            self.scan_directory_into_map(
                &core_dir,
                SkillScope::Project,
                Some(SkillKind::Core),
                false,
                &mut skill_map,
            );
        }

        // Mark active status based on manifest (minikit.json / onpkg.json) or presence in .minicode/skills/
        let manifest_active = self.get_manifest_active_skills();

        let mut skills: Vec<VaultSkill> = skill_map
            .into_values()
            .map(|mut s| {
                if manifest_active
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
        default_kind: Option<SkillKind>,
        is_active: bool,
        map: &mut std::collections::BTreeMap<String, VaultSkill>,
    ) {
        if !dir.exists() || !dir.is_dir() {
            return;
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let load_into_map =
            |file_path: PathBuf,
             fallback_name: &str,
             map: &mut std::collections::BTreeMap<String, VaultSkill>| {
                if let Ok(content) = fs::read_to_string(&file_path) {
                    let (mut fm, instructions) = parse_skill_markdown(&content, fallback_name);
                    if fm.kind.is_none() {
                        if let Some(dk) = default_kind {
                            fm.kind = Some(dk.as_str().to_string());
                        }
                    }
                    let skill = VaultSkill {
                        name: fm.name.clone(),
                        description: fm.description.clone(),
                        scope,
                        frontmatter: fm,
                        instructions,
                        raw_content: content,
                        path: Some(file_path),
                        is_active_in_project: is_active,
                    };
                    map.insert(skill.name.to_lowercase(), skill);
                }
            };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if name.eq_ignore_ascii_case("index") || name.eq_ignore_ascii_case("log") {
                    continue;
                }
                load_into_map(path, &name, map);
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
                    load_into_map(target, &folder_name, map);
                }
            }
        }
    }

    /// Finds a skill by name or trigger/alias across the hierarchy (Project -> Global -> Built-in).
    pub fn get_skill(&self, name: &str) -> Result<VaultSkill> {
        let clean = sanitize_skill_name(name);
        let all = self.list_all_skills();

        // 1. Exact name match
        if let Some(s) = all.iter().find(|s| s.name.eq_ignore_ascii_case(&clean)) {
            return Ok(s.clone());
        }

        // 2. Trigger or prefix alias match (e.g. "tailwind" -> "tailwind-v4", "next" -> "nextjs", "rust" -> "rust-tokio")
        if let Some(s) = all.iter().find(|s| {
            s.frontmatter
                .triggers
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&clean))
                || s.name.starts_with(&format!("{}-", clean))
        }) {
            return Ok(s.clone());
        }

        Err(VaultError::NotFound(name.to_string()))
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

        let effective_scope = match scope {
            SkillScope::Builtin => SkillScope::Project,
            other => other,
        };
        let target_dir = match effective_scope {
            SkillScope::Project => self.project_skills_dir().join(&clean_name),
            SkillScope::Global => {
                let dir = self.global_skills_dir().ok_or_else(|| {
                    VaultError::InvalidSkill(
                        "Cannot resolve user home directory for global vault".to_string(),
                    )
                })?;
                dir.join(&clean_name)
            }
            SkillScope::Builtin => self.project_skills_dir().join(&clean_name),
        };

        fs::create_dir_all(&target_dir).map_err(|e| VaultError::Io {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        self.clear_skill_tombstone(&clean_name);

        let effective_triggers = if triggers.is_empty() {
            crate::vault::models::infer_triggers_from_metadata(&clean_name, description)
        } else {
            triggers
        };

        let fm = SkillFrontmatter {
            name: clean_name.clone(),
            description: description.trim().to_string(),
            version: "1.0.0".to_string(),
            author: Some("minicode".to_string()),
            category: category.map(|c| c.trim().to_lowercase()),
            kind: kind.map(|k| k.trim().to_lowercase()),
            globs,
            triggers: effective_triggers,
            always_apply: false,
            allowed_tools: Vec::new(),
        };

        let content = format_skill_markdown(&fm, instructions);
        let target_file = target_dir.join("SKILL.md");

        fs::write(&target_file, &content).map_err(|e| VaultError::Io {
            path: target_file.display().to_string(),
            source: e,
        })?;

        if effective_scope == SkillScope::Project {
            let _ = self.add_to_manifest_active_skills(&clean_name);
        }

        Ok(VaultSkill {
            name: clean_name,
            description: fm.description.clone(),
            scope: effective_scope,
            frontmatter: fm,
            instructions: instructions.trim().to_string(),
            raw_content: content,
            path: Some(target_file),
            is_active_in_project: effective_scope == SkillScope::Project,
        })
    }

    /// Loads a skill from Built-in or Global vault into the active Project (`.minicode/skills/<name>/SKILL.md`).
    /// Also clears any deletion tombstone if a previously deleted built-in skill is loaded.
    pub fn load_to_project(&self, name: &str) -> Result<VaultSkill> {
        let clean_name = sanitize_skill_name(name);
        self.clear_skill_tombstone(&clean_name);
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

        // Copy Level-3 progressive disclosure subdirectories (scripts/, references/, assets/, etc.) if present
        if let Some(ref source_path) = source_skill.path {
            if source_path.file_name().and_then(|s| s.to_str()) == Some("SKILL.md") {
                if let Some(source_dir) = source_path.parent() {
                    const RESOURCE_DIRS: &[&str] = &[
                        "scripts",
                        "references",
                        "reference",
                        "assets",
                        "templates",
                        "themes",
                        "examples",
                        "ooxml",
                    ];
                    for subdir_name in RESOURCE_DIRS {
                        let src_sub = source_dir.join(subdir_name);
                        if src_sub.is_dir() {
                            let dst_sub = target_dir.join(subdir_name);
                            let _ = Self::copy_dir_recursive(&src_sub, &dst_sub);
                        }
                    }
                }
            }
        }

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

    fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let fname = entry.file_name();
            let fname_str = fname.to_string_lossy();
            if fname_str == "__pycache__"
                || fname_str == ".DS_Store"
                || fname_str.ends_with(".pyc")
                || fname_str.ends_with(".pyo")
            {
                continue;
            }
            let ty = entry.file_type()?;
            let dst_path = dst.join(&fname);
            if ty.is_dir() {
                Self::copy_dir_recursive(&entry.path(), &dst_path)?;
            } else if ty.is_file() {
                fs::copy(entry.path(), dst_path)?;
            }
        }
        Ok(())
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

    /// Updates the instructions or content of an existing skill across any scope (including Builtin).
    pub fn update_skill(
        &self,
        scope: SkillScope,
        name: &str,
        new_instructions: &str,
    ) -> Result<VaultSkill> {
        self.patch_skill_full(
            scope,
            name,
            Some(new_instructions),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    /// Full CRUD update/patch for any skill (Project, Global, or Builtin override).
    /// Supports replacing `instructions`, surgical `search_block` -> `replace_block` patching,
    /// and updating frontmatter metadata (`description`, `category`, `kind`, `triggers`, `globs`, `always_apply`).
    #[allow(clippy::too_many_arguments)]
    pub fn patch_skill_full(
        &self,
        scope: SkillScope,
        name: &str,
        new_instructions: Option<&str>,
        search_block: Option<&str>,
        replace_block: Option<&str>,
        description: Option<&str>,
        category: Option<&str>,
        kind: Option<&str>,
        triggers: Option<Vec<String>>,
        globs: Option<Vec<String>>,
        always_apply: Option<bool>,
    ) -> Result<VaultSkill> {
        let clean = sanitize_skill_name(name);
        self.clear_skill_tombstone(&clean);
        let existing = self.get_skill(&clean)?;

        let updated_instructions = if let Some(full) = new_instructions {
            full.trim().to_string()
        } else if let (Some(search), Some(replace)) = (search_block, replace_block) {
            if !existing.instructions.contains(search) {
                return Err(VaultError::InvalidSkill(format!(
                    "Patch search_block not found in skill '{}'",
                    existing.name
                )));
            }
            existing
                .instructions
                .replacen(search, replace, 1)
                .trim()
                .to_string()
        } else {
            existing.instructions.clone()
        };

        let mut fm = existing.frontmatter.clone();
        if let Some(desc) = description {
            if !desc.trim().is_empty() {
                fm.description = desc.trim().to_string();
            }
        }
        if let Some(cat) = category {
            if !cat.trim().is_empty() {
                fm.category = Some(cat.trim().to_lowercase());
            }
        }
        if let Some(k) = kind {
            if !k.trim().is_empty() {
                fm.kind = Some(k.trim().to_lowercase());
            }
        }
        if let Some(trigs) = triggers {
            if !trigs.is_empty() {
                fm.triggers = trigs;
            }
        }
        if let Some(g) = globs {
            fm.globs = g;
        }
        if let Some(aa) = always_apply {
            fm.always_apply = aa;
        }

        let effective_scope = match scope {
            SkillScope::Builtin => SkillScope::Project,
            other => other,
        };

        let target_path = match (&existing.path, existing.scope == effective_scope) {
            (Some(p), true) => p.clone(),
            _ => {
                let target_dir = match effective_scope {
                    SkillScope::Project | SkillScope::Builtin => {
                        self.project_skills_dir().join(&existing.name)
                    }
                    SkillScope::Global => {
                        let dir = self.global_skills_dir().ok_or_else(|| {
                            VaultError::InvalidSkill(
                                "Cannot resolve user home directory for global vault".to_string(),
                            )
                        })?;
                        dir.join(&existing.name)
                    }
                };
                fs::create_dir_all(&target_dir).map_err(|e| VaultError::Io {
                    path: target_dir.display().to_string(),
                    source: e,
                })?;
                target_dir.join("SKILL.md")
            }
        };

        let new_content = format_skill_markdown(&fm, &updated_instructions);
        fs::write(&target_path, &new_content).map_err(|e| VaultError::Io {
            path: target_path.display().to_string(),
            source: e,
        })?;

        if effective_scope == SkillScope::Project {
            let _ = self.add_to_manifest_active_skills(&existing.name);
        }

        Ok(VaultSkill {
            name: existing.name,
            description: fm.description.clone(),
            scope: effective_scope,
            frontmatter: fm,
            instructions: updated_instructions,
            raw_content: new_content,
            path: Some(target_path),
            is_active_in_project: effective_scope == SkillScope::Project,
        })
    }

    /// Deletes a skill completely from the specified scope (including built-in/orchestrator skills via tombstones).
    pub fn delete_skill(&self, scope: SkillScope, name: &str) -> Result<()> {
        let clean = self
            .get_skill(name)
            .map(|s| s.name)
            .unwrap_or_else(|_| sanitize_skill_name(name));
        let is_builtin = crate::vault::builtin::find_builtin_skill(&clean).is_some();

        match scope {
            SkillScope::Project => {
                self.unload_from_project(&clean)?;
                if is_builtin {
                    self.record_skill_tombstone(SkillScope::Project, &clean)?;
                }
                Ok(())
            }
            SkillScope::Global => {
                if let Some(dir) = self.global_skills_dir() {
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
                }
                if is_builtin {
                    self.record_skill_tombstone(SkillScope::Global, &clean)?;
                }
                Ok(())
            }
            SkillScope::Builtin => {
                let _ = self.unload_from_project(&clean);
                self.record_skill_tombstone(SkillScope::Builtin, &clean)?;
                Ok(())
            }
        }
    }

    /// Normalizes GitHub `/blob/`, `/tree/`, or `anthropics/skills/<name>` shorthand URLs
    /// into raw `SKILL.md` URLs ready for direct HTTP download.
    pub fn normalize_skill_import_url(url: &str) -> String {
        let trimmed = url.trim().trim_end_matches('/');

        // 1. Shorthand: `anthropics/skills/<skill>` or `github:<owner>/<repo>/<skill>`
        if let Some(rest) = trimmed.strip_prefix("anthropics/skills/") {
            let skill_path = rest
                .trim_start_matches("skills/")
                .trim_end_matches("/SKILL.md");
            return format!(
                "https://raw.githubusercontent.com/anthropics/skills/main/skills/{}/SKILL.md",
                skill_path
            );
        }
        if let Some(rest) = trimmed.strip_prefix("github:") {
            let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() >= 3 {
                let owner = parts[0];
                let repo = parts[1];
                let subpath = parts[2..].join("/");
                let final_path = if subpath.ends_with(".md") {
                    subpath
                } else {
                    format!("{}/SKILL.md", subpath)
                };
                return format!(
                    "https://raw.githubusercontent.com/{}/{}/main/{}",
                    owner, repo, final_path
                );
            }
        }

        // 2. GitHub web URL: `https://github.com/<owner>/<repo>/blob/<branch>/<path>`
        // or `https://github.com/<owner>/<repo>/tree/<branch>/<path>`
        if let Some(gh_rest) = trimmed
            .strip_prefix("https://github.com/")
            .or_else(|| trimmed.strip_prefix("http://github.com/"))
        {
            let parts: Vec<&str> = gh_rest.split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() >= 5 && (parts[2] == "blob" || parts[2] == "tree") {
                let owner = parts[0];
                let repo = parts[1];
                let branch = parts[3];
                let subpath = parts[4..].join("/");
                let final_path = if parts[2] == "tree" && !subpath.ends_with(".md") {
                    format!("{}/SKILL.md", subpath)
                } else {
                    subpath
                };
                return format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/{}",
                    owner, repo, branch, final_path
                );
            }
        }

        trimmed.to_string()
    }

    /// Fetches a skill definition from an HTTP URL (or GitHub shorthand/tree URL) and imports it into the target scope.
    pub async fn import_from_url(
        &self,
        url: &str,
        target_scope: SkillScope,
        name_override: Option<&str>,
    ) -> Result<VaultSkill> {
        let resolved_url = Self::normalize_skill_import_url(url);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| VaultError::Network {
                url: resolved_url.clone(),
                reason: e.to_string(),
            })?;

        let resp = client
            .get(&resolved_url)
            .send()
            .await
            .map_err(|e| VaultError::Network {
                url: resolved_url.clone(),
                reason: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(VaultError::Network {
                url: resolved_url,
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let body = resp.text().await.map_err(|e| VaultError::Network {
            url: resolved_url.clone(),
            reason: e.to_string(),
        })?;

        let fallback_name = name_override.unwrap_or_else(|| {
            resolved_url
                .trim_end_matches("/SKILL.md")
                .split('/')
                .next_back()
                .unwrap_or("imported-skill")
                .trim_end_matches(".md")
        });

        let (mut fm, instructions) = parse_skill_markdown(&body, fallback_name);
        if let Some(override_name) = name_override {
            fm.name = sanitize_skill_name(override_name);
        }

        let mut created = self.create_skill(
            target_scope,
            &fm.name,
            &fm.description,
            fm.category.as_deref(),
            fm.kind.as_deref(),
            &instructions,
            fm.triggers,
            fm.globs,
        )?;

        if !fm.allowed_tools.is_empty() {
            created.frontmatter.allowed_tools = fm.allowed_tools;
            if let Some(ref target_file) = created.path {
                let updated_content =
                    format_skill_markdown(&created.frontmatter, &created.instructions);
                let _ = fs::write(target_file, &updated_content);
                created.raw_content = updated_content;
            }
        }

        Ok(created)
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

    fn mutate_manifest_active_skills<F>(&self, mutator: F) -> Result<()>
    where
        F: FnOnce(&mut Vec<serde_json::Value>) -> bool,
    {
        let manifest_file = ["minikit.json", "onpkg.json"]
            .into_iter()
            .map(|name| self.workspace_root.join(name))
            .find(|p| p.exists());

        if let Some(path) = manifest_file {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(arr) = val.get_mut("active_skills").and_then(|a| a.as_array_mut()) {
                        if mutator(arr) {
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

    /// Adds a skill to `active_skills` in the workspace manifest.
    fn add_to_manifest_active_skills(&self, skill_name: &str) -> Result<()> {
        let name_val = serde_json::Value::String(skill_name.to_lowercase());
        self.mutate_manifest_active_skills(|arr| {
            if !arr.contains(&name_val) {
                arr.push(name_val);
                true
            } else {
                false
            }
        })
    }

    /// Removes a skill from `active_skills` in the workspace manifest.
    fn remove_from_manifest_active_skills(&self, skill_name: &str) -> Result<()> {
        let target_lower = skill_name.to_lowercase();
        self.mutate_manifest_active_skills(|arr| {
            let before = arr.len();
            arr.retain(|s| s.as_str().map(|n| n.to_lowercase()) != Some(target_lower.clone()));
            arr.len() != before
        })
    }

    // =========================================================================
    // Dynamic Context Matching & Prompt Assembly Engine (Zero-Hardcoding)
    // =========================================================================

    /// Scans workspace manifests and configuration files to extract a normalized set of technology package tokens.
    /// Supports `package.json`, `Cargo.toml`, `pyproject.toml`, `requirements.txt`, `pubspec.yaml`, and root infrastructure markers.
    pub fn detect_workspace_package_tokens(&self) -> std::collections::HashSet<String> {
        let mut tokens = std::collections::HashSet::new();

        // 1. Node.js / Web (package.json)
        let pkg_path = self.workspace_root.join("package.json");
        if let Ok(content) = fs::read_to_string(&pkg_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                for dep_field in &["dependencies", "devDependencies", "peerDependencies"] {
                    if let Some(deps) = json.get(*dep_field).and_then(|d| d.as_object()) {
                        for key in deps.keys() {
                            let lower = key.to_lowercase();
                            tokens.insert(lower.clone());

                            // Clean scopes (@scope/pkg -> scope, pkg)
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
                            if let Some(stem) = clean.strip_suffix("css") {
                                if stem.len() >= 3 {
                                    tokens.insert(stem.to_string());
                                }
                            }
                            if !clean.contains('/') && !clean.ends_with("js") {
                                tokens.insert(format!("{}js", clean));
                            }
                        }
                    }
                }
            }
        }

        // 2. Rust (Cargo.toml)
        let cargo_path = self.workspace_root.join("Cargo.toml");
        if let Ok(content) = fs::read_to_string(&cargo_path) {
            if let Ok(toml_val) = toml::from_str::<toml::Value>(&content) {
                for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
                    if let Some(deps) = toml_val.get(*section).and_then(|d| d.as_table()) {
                        for key in deps.keys() {
                            let lower = key.to_lowercase();
                            tokens.insert(lower.clone());
                            for part in lower.split('-').chain(lower.split('_')) {
                                if !part.is_empty() {
                                    tokens.insert(part.to_string());
                                }
                            }
                        }
                    }
                }
                tokens.insert("rust".to_string());
                tokens.insert("cargo".to_string());
            }
        }

        // 3. Python (pyproject.toml or requirements.txt)
        let pyproject_path = self.workspace_root.join("pyproject.toml");
        if let Ok(content) = fs::read_to_string(&pyproject_path) {
            tokens.insert("python".to_string());
            if let Ok(toml_val) = toml::from_str::<toml::Value>(&content) {
                if let Some(deps) = toml_val
                    .get("project")
                    .and_then(|p| p.get("dependencies"))
                    .and_then(|d| d.as_array())
                {
                    for dep in deps {
                        if let Some(s) = dep.as_str() {
                            let name = s
                                .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                                .next()
                                .unwrap_or_default();
                            if !name.is_empty() {
                                tokens.insert(name.to_lowercase());
                            }
                        }
                    }
                }
            }
        }
        let req_path = self.workspace_root.join("requirements.txt");
        if let Ok(content) = fs::read_to_string(&req_path) {
            tokens.insert("python".to_string());
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    let name = trimmed
                        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                        .next()
                        .unwrap_or_default();
                    if !name.is_empty() {
                        tokens.insert(name.to_lowercase());
                    }
                }
            }
        }

        // 4. Flutter / Dart (pubspec.yaml)
        let pubspec_path = self.workspace_root.join("pubspec.yaml");
        if let Ok(content) = fs::read_to_string(&pubspec_path) {
            tokens.insert("flutter".to_string());
            tokens.insert("dart".to_string());
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some((k, _)) = trimmed.split_once(':') {
                    let clean_k = k.trim().to_lowercase();
                    if !clean_k.is_empty() && !clean_k.starts_with('#') {
                        tokens.insert(clean_k);
                    }
                }
            }
        }

        // 5. Root infrastructure & framework config markers
        let root_markers = [
            ("Dockerfile", "docker"),
            ("docker-compose.yml", "docker"),
            ("compose.yaml", "docker"),
            ("compose.yml", "docker"),
            ("schema.prisma", "prisma"),
            ("vite.config.ts", "vite"),
            ("vite.config.js", "vite"),
            ("vite.config.mjs", "vite"),
            ("next.config.js", "nextjs"),
            ("next.config.mjs", "nextjs"),
            ("next.config.ts", "nextjs"),
            ("tailwind.config.js", "tailwind"),
            ("tailwind.config.ts", "tailwind"),
            ("tailwind.config.mjs", "tailwind"),
        ];

        for (file_marker, token) in &root_markers {
            if self.workspace_root.join(file_marker).exists() {
                tokens.insert(token.to_string());
            }
        }

        if self
            .workspace_root
            .join("prisma")
            .join("schema.prisma")
            .exists()
        {
            tokens.insert("prisma".to_string());
        }

        tokens
    }

    /// Evaluates if a skill matches the workspace stack based on token-set intersection
    /// between the skill's identifier tokens (`skill.name`) and detected workspace package tokens.
    pub fn matches_skill_to_stack(
        &self,
        skill: &VaultSkill,
        stack_tokens: &std::collections::HashSet<String>,
    ) -> bool {
        if stack_tokens.is_empty() {
            return false;
        }
        let name_lower = skill.name.to_lowercase();
        if stack_tokens.contains(&name_lower) {
            return true;
        }

        // Extract non-version identifier tokens from skill name (e.g. "tailwind-v4" -> ["tailwind"],
        // "rust-tokio" -> ["rust", "tokio"], "slack-gif-creator" -> ["slack", "gif", "creator"]).
        let name_tokens: Vec<&str> = name_lower
            .split('-')
            .filter(|part| {
                part.len() >= 3
                    && !(part.starts_with('v') && part[1..].chars().all(|c| c.is_ascii_digit()))
            })
            .collect();

        if name_tokens.is_empty() {
            return false;
        }

        let token_in_stack = |tok: &str| -> bool {
            let tok_stem = tok
                .strip_suffix("js")
                .or_else(|| tok.strip_suffix("css"))
                .unwrap_or(tok);
            stack_tokens.contains(tok)
                || stack_tokens.contains(tok_stem)
                || (tok_stem.len() >= 3
                    && stack_tokens.iter().any(|st| {
                        let st_stem = st
                            .strip_suffix("js")
                            .or_else(|| st.strip_suffix("css"))
                            .unwrap_or(st.as_str());
                        st_stem == tok_stem
                    }))
        };

        // A skill matches the workspace stack if every primary token in its name is present in
        // `stack_tokens`, or for Runtime skills if any primary runtime token is present.
        if skill.kind() == SkillKind::Runtime {
            name_tokens.iter().any(|tok| token_in_stack(tok))
        } else {
            name_tokens.iter().all(|tok| token_in_stack(tok))
        }
    }

    fn match_from_loaded_skills<P: AsRef<Path>>(
        &self,
        all_skills: &[VaultSkill],
        stack_tokens: &std::collections::HashSet<String>,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> Vec<VaultSkill> {
        let prompt_lower = prompt.map(|p| p.to_lowercase());
        let objective_lower = session_objective.map(|o| o.to_lowercase());

        let mut matched = Vec::new();

        for skill in all_skills {
            if skill.is_active_in_project
                || skill.frontmatter.always_apply
                || self.matches_skill_to_stack(skill, stack_tokens)
                || (!active_files.is_empty()
                    && active_files
                        .iter()
                        .any(|f| skill.matches_file(&self.workspace_root, f.as_ref())))
                || prompt_lower
                    .as_ref()
                    .map(|p| skill.matches_prompt(p))
                    .unwrap_or(false)
                || objective_lower
                    .as_ref()
                    .map(|obj| skill.matches_prompt(obj))
                    .unwrap_or(false)
            {
                matched.push(skill.clone());
            }
        }

        matched.sort_by(|a, b| a.name.cmp(&b.name));
        matched.dedup_by(|a, b| a.name == b.name);
        matched
    }

    /// Matches skills for the current context (stack tokens, active files, user prompt, and session objective).
    /// Purely metadata-driven: checks frontmatter `always_apply`, `globs`, `triggers`, and project scope.
    pub fn match_skills_for_context<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> Vec<VaultSkill> {
        let all_skills = self.list_all_skills();
        let stack_tokens = self.detect_workspace_package_tokens();
        self.match_from_loaded_skills(
            &all_skills,
            &stack_tokens,
            prompt,
            session_objective,
            active_files,
        )
    }

    /// Formats progressive agent prompt context using Anthropic's 3-Level Progressive Disclosure:
    /// - Level 1: Compact metadata index (`name`, `description`, `path` / `triggers`) for local package references and available vault skills.
    /// - Level 2: Full guidelines for concise active stack/workflow skills within a bounded token budget; executive summary + on-demand pointer (`vault_show` / `read_file`) for large skill manuals.
    /// - Level 3: Bundled `scripts/`, `references/`, and `assets/` paths surfaced for on-demand inspection.
    pub fn format_prompt_skills_context<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> String {
        const MAX_INLINE_SKILL_CHARS: usize = 4_500;
        const TOTAL_LEVEL2_BUDGET_CHARS: usize = 12_000;
        const EXECUTIVE_SUMMARY_CHARS: usize = 900;

        let all_skills = self.list_all_skills();
        if all_skills.is_empty() {
            return String::new();
        }

        let stack_tokens = self.detect_workspace_package_tokens();
        let matched = self.match_from_loaded_skills(
            &all_skills,
            &stack_tokens,
            prompt,
            session_objective,
            active_files,
        );
        let matched_names: std::collections::HashSet<String> =
            matched.iter().map(|s| s.name.to_lowercase()).collect();

        let mut out = String::new();
        let mut used_level2_chars: usize = 0;

        // 1. Core Project Invariants & Specifications (Authoritative)
        let core_specs: Vec<_> = matched
            .iter()
            .filter(|s| s.kind() == SkillKind::Core)
            .collect();
        if !core_specs.is_empty() {
            out.push_str("\n# Core Project Invariants & Specifications (Authoritative):\n");
            out.push_str("The following project specifications define the non-negotiable contract and architecture for this workspace:\n\n");
            for s in core_specs {
                out.push_str(&format!(
                    "## Core Spec: `{}`\n{}\n\n",
                    s.name,
                    s.instructions.trim()
                ));
            }
        }

        // 2. Runtime & Architecture Workflow Conventions
        let runtime_workflows: Vec<_> = matched
            .iter()
            .filter(|s| s.kind() == SkillKind::Runtime)
            .collect();
        if !runtime_workflows.is_empty() {
            out.push_str("\n# Project Architecture & Runtime Workflow Conventions:\n");
            out.push_str("The workspace runtime conventions are active. Adhere strictly to these patterns:\n\n");
            for r in runtime_workflows {
                out.push_str(&format!(
                    "## Runtime `{}`\n_{}_\n\n{}\n\n",
                    r.name,
                    r.description,
                    r.instructions.trim()
                ));
            }
        }

        // Helper to determine if a Reference skill is an on-disk `minikit_docs/packages/<pkg>.md` manual
        let is_package_manual = |s: &VaultSkill| -> bool {
            s.kind() == SkillKind::Reference
                && s.path
                    .as_ref()
                    .is_some_and(|p| p.components().any(|c| c.as_os_str() == "packages"))
                && s.instructions.len() > 1_800
        };

        // 3. Active Procedural & Behavioral Skills & 4. Concise Technology Reference Guides (Level 2)
        for (kind, heading, subheading) in [
            (
                SkillKind::Workflow,
                "\n# Active Procedural & Behavioral Skills:\n",
                "The following engineering methodologies govern execution quality for this workspace:\n\n",
            ),
            (
                SkillKind::Reference,
                "\n# Active Technology & Methodology Skills:\n",
                "The following domain skills are actively applied to this workspace/task. Adhere strictly to their guidelines and invariants:\n\n",
            ),
        ] {
            let mut group: Vec<_> = matched
                .iter()
                .filter(|s| s.kind() == kind && !is_package_manual(s))
                .collect();
            // Prioritize concise, high-signal skills first so core rules fit comfortably in Level-2 budget
            group.sort_by_key(|s| s.instructions.len());

            if !group.is_empty() {
                out.push_str(heading);
                out.push_str(subheading);
                for skill in group {
                    let body = skill.instructions.trim();
                    let bundled = skill.discover_bundled_resources();
                    let bundled_line = if !bundled.is_empty() {
                        format!(
                            "\n_Level-3 Bundled Resources: {}_\n",
                            bundled.join(", ")
                        )
                    } else {
                        String::new()
                    };

                    if body.len() <= MAX_INLINE_SKILL_CHARS
                        && used_level2_chars.saturating_add(body.len()) <= TOTAL_LEVEL2_BUDGET_CHARS
                    {
                        used_level2_chars = used_level2_chars.saturating_add(body.len());
                        out.push_str(&format!(
                            "## Skill `{}` ({})\n_{}_{}\n\n{}\n\n",
                            skill.name,
                            skill.category(),
                            skill.description,
                            bundled_line,
                            body
                        ));
                    } else {
                        let preview_end = body
                            .char_indices()
                            .take_while(|(idx, _)| *idx < EXECUTIVE_SUMMARY_CHARS)
                            .last()
                            .map(|(idx, ch)| idx + ch.len_utf8())
                            .unwrap_or(0);
                        let preview = body[..preview_end].trim();
                        let path_hint = skill
                            .path
                            .as_ref()
                            .and_then(|p| p.strip_prefix(&self.workspace_root).ok().or(Some(p.as_path())))
                            .map(|p| format!(" or `read_file(\"{}\")`", p.display()))
                            .unwrap_or_default();
                        out.push_str(&format!(
                            "## Skill `{}` ({}) [Progressive Level-2 Summary]\n_{}_{}\n\n{}\n\n> Full specification available on-demand via `vault_show(\"{}\")`{}.\n\n",
                            skill.name,
                            skill.category(),
                            skill.description,
                            bundled_line,
                            preview,
                            skill.name,
                            path_hint
                        ));
                    }
                }
            }
        }

        // 5. Local Package References & Continuous Learning Protocol (Level 1 Index -> Level 3 On-Demand)
        out.push_str("\n# Local Package References & Self-Improving Knowledge Base:\n");
        out.push_str("Reference manuals and troubleshooting guides for libraries and packages used in this workspace.\n");
        let package_manuals: Vec<_> = matched.iter().filter(|s| is_package_manual(s)).collect();
        if !package_manuals.is_empty() {
            out.push_str("Installed Workspace Package Manuals (inspect on-demand via `read_file` or `kit_skill_show(name)`):\n");
            for pkg in package_manuals {
                let rel_path = pkg
                    .path
                    .as_ref()
                    .and_then(|p| p.strip_prefix(&self.workspace_root).ok())
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| format!("minikit_docs/packages/{}.md", pkg.name));
                out.push_str(&format!(
                    "  • `{}` (`{}`): {}\n",
                    pkg.name, rel_path, pkg.description
                ));
            }
            out.push('\n');
        }
        out.push_str("Continuous Learning Protocol:\n\
            1. Local Reference First: When using or troubleshooting a package/framework, check its local documentation in `minikit_docs/packages/<pkg>.md` (or inspect via `kit_skill_show(name)`).\n\
            2. Web Research: If the package doc does not contain the answer or an error occurs, search the web or consult official documentation.\n\
            3. Verification: Test and verify the fix in the workspace.\n\
            4. Self-Improvement: Once the fix is verified, CREATE or UPDATE `minikit_docs/packages/<pkg>.md` with the solution so minicode and the human developer retain that knowledge permanently!\n\n");

        // Section 6: Directory of available skills (Level 1 Metadata Index)
        let available: Vec<_> = all_skills
            .into_iter()
            .filter(|s| !matched_names.contains(&s.name.to_lowercase()))
            .collect();

        if !available.is_empty() {
            out.push_str("# Available Domain Skills in Vault:\n");
            out.push_str("The following specialized skills are available on-demand. Invoke `kit_skill_show(name)` or `vault_show(name)` to inspect their complete guidelines before implementing:\n");
            for s in available {
                let badge = s.kind().badge();
                let triggers_hint = if !s.frontmatter.triggers.is_empty() {
                    format!(" [triggers: {}]", s.frontmatter.triggers.join(", "))
                } else {
                    String::new()
                };
                out.push_str(&format!(
                    "  • [{}] `{}`: {}{}\n",
                    badge, s.name, s.description, triggers_hint
                ));
            }
            out.push('\n');
        }

        out
    }

    /// Formats dynamic turn-level skill activations for Hermes agent orchestration.
    /// Emits `<auto_activated_skills>` if new skills match the turn prompt or modified files.
    pub fn format_dynamic_turn_skills<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
        exclude_skills: &[String],
    ) -> Option<String> {
        let all_skills = self.list_all_skills();
        if all_skills.is_empty() {
            return None;
        }
        let stack_tokens = self.detect_workspace_package_tokens();

        let mut exclude_set: std::collections::HashSet<String> =
            exclude_skills.iter().map(|s| s.to_lowercase()).collect();

        // Exclude baseline workspace/stack skills that are already injected into the static system prompt
        for baseline_skill in
            self.match_from_loaded_skills::<&str>(&all_skills, &stack_tokens, None, None, &[])
        {
            exclude_set.insert(baseline_skill.name.to_lowercase());
        }

        let matched = self.match_from_loaded_skills(
            &all_skills,
            &stack_tokens,
            prompt,
            session_objective,
            active_files,
        );
        let new_skills: Vec<_> = matched
            .into_iter()
            .filter(|s| !exclude_set.contains(&s.name.to_lowercase()))
            .take(3)
            .collect();

        if new_skills.is_empty() {
            return None;
        }

        let mut out = String::from("<auto_activated_skills>\n");
        out.push_str("  Hermes Progressive Skill Intelligence Activated:\n");
        out.push_str("  The following domain skills automatically matched your prompt intent or active files:\n");

        for skill in new_skills {
            out.push_str(&format!(
                "  • Skill `{}`: {}\n",
                skill.name, skill.description
            ));
            let bundled = skill.discover_bundled_resources();
            if !bundled.is_empty() {
                if let Some(ref skill_path) = skill.path {
                    if let Some(skill_dir) = skill_path.parent() {
                        out.push_str(&format!(
                            "    Bundled Level-3 Resources in `{}` (read/execute on demand): {}\n",
                            skill_dir.display(),
                            bundled.join(", ")
                        ));
                    }
                }
            }
            out.push_str("    Guidelines & Invariants:\n");
            for line in skill.instructions.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("---") {
                    out.push_str(&format!("      {}\n", trimmed));
                }
            }
            out.push('\n');
        }

        out.push_str(
            "  Directive: Adhere strictly to these procedural skill guidelines for this task.\n",
        );
        out.push_str("</auto_activated_skills>");
        Some(out)
    }

    /// Collects all `allowed_tools` (tool names or category identifiers) declared in the YAML frontmatter
    /// of skills currently active for the workspace, prompt, or active file set.
    pub fn collect_active_skill_allowed_tools<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> Vec<String> {
        let all_skills = self.list_all_skills();
        if all_skills.is_empty() {
            return Vec::new();
        }
        let stack_tokens = self.detect_workspace_package_tokens();
        let matched = self.match_from_loaded_skills(
            &all_skills,
            &stack_tokens,
            prompt,
            session_objective,
            active_files,
        );
        let mut allowed = Vec::new();
        for skill in matched {
            for tool_name in skill.frontmatter.allowed_tools {
                let trimmed = tool_name.trim().to_string();
                if !trimmed.is_empty() && !allowed.contains(&trimmed) {
                    allowed.push(trimmed);
                }
            }
        }
        allowed
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

    #[test]
    fn test_vault_workspace_matching_and_prompt_assembly() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let store = VaultStore::new(ws);

        // 1. Initially empty directory
        let initial_tokens = store.detect_workspace_package_tokens();
        assert!(initial_tokens.is_empty());

        // 2. Add package.json with react and tailwindcss
        let pkg_json = r#"{
            "name": "my-app",
            "dependencies": {
                "react": "^19.0.0",
                "react-dom": "^19.0.0",
                "@tailwindcss/vite": "^4.0.0"
            }
        }"#;
        fs::write(ws.join("package.json"), pkg_json).unwrap();

        let tokens = store.detect_workspace_package_tokens();
        assert!(tokens.contains("react"));
        assert!(tokens.contains("tailwindcss") || tokens.contains("tailwind"));
        assert!(tokens.contains("vite"));

        // 3. Match skills for context based on stack
        let matched = store.match_skills_for_context::<&str>(None, None, &[]);
        let matched_names: Vec<_> = matched.iter().map(|s| s.name.as_str()).collect();
        assert!(matched_names.contains(&"react"));
        assert!(matched_names.contains(&"tailwind-v4") || matched_names.contains(&"tailwind"));

        // 4. Prompt matching for methodology skill (e.g. systematic-debugging when user says "fix this bug")
        let debug_matched = store.match_skills_for_context::<&str>(
            Some("Please fix this bug in user authentication"),
            None,
            &[],
        );
        let debug_names: Vec<_> = debug_matched.iter().map(|s| s.name.as_str()).collect();
        assert!(debug_names.contains(&"systematic-debugging"));
        assert!(debug_names.contains(&"security-audit"));

        // 5. Test format_prompt_skills_context contains full invariants
        let prompt_context = store.format_prompt_skills_context::<&str>(None, None, &[]);
        assert!(prompt_context.contains("# Active Technology & Methodology Skills:"));
        assert!(prompt_context.contains("Skill `react`"));
        assert!(prompt_context.contains("Functional Components"));
        assert!(prompt_context.contains("# Available Domain Skills in Vault:"));

        // 6. Test dynamic turn activation formatting
        let dynamic_block = store.format_dynamic_turn_skills::<&str>(
            Some("Help me write unit tests"),
            None,
            &[],
            &["react".to_string()],
        );
        assert!(dynamic_block.is_some());
        let dyn_str = dynamic_block.unwrap();
        assert!(dyn_str.contains("<auto_activated_skills>"));
        assert!(dyn_str.contains("Skill `tdd-workflow`"));
    }

    #[test]
    fn test_level3_progressive_disclosure_and_allowed_tools() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let global_vault = ws.join("global_vault");
        let store = VaultStore::new(ws).with_custom_global_vault(global_vault.clone());

        // Create a global skill with bundled scripts/ and references/
        let created = store
            .create_skill(
                SkillScope::Global,
                "custom-audit",
                "Audit API endpoints and run schema verification scripts",
                Some("security"),
                Some("workflow"),
                "Run `scripts/check.py` and consult `references/guide.md`.",
                vec![],
                vec!["*.py".to_string()],
            )
            .unwrap();

        // Auto-inferred triggers should be populated from name/description
        assert!(!created.frontmatter.triggers.is_empty());
        assert!(created
            .frontmatter
            .triggers
            .contains(&"custom-audit".to_string()));

        let global_skill_dir = global_vault.join("skills").join("custom-audit");
        fs::create_dir_all(global_skill_dir.join("scripts")).unwrap();
        fs::create_dir_all(global_skill_dir.join("references")).unwrap();
        fs::write(
            global_skill_dir.join("scripts").join("check.py"),
            "print('ok')",
        )
        .unwrap();
        fs::write(
            global_skill_dir.join("references").join("guide.md"),
            "# Reference Guide",
        )
        .unwrap();

        let fetched = store.get_skill("custom-audit").unwrap();
        let resources = fetched.discover_bundled_resources();
        assert_eq!(
            resources,
            vec![
                "references/guide.md".to_string(),
                "scripts/check.py".to_string()
            ]
        );

        // Load into project and verify Level-3 subdirectories are copied
        let loaded = store.load_to_project("custom-audit").unwrap();
        assert_eq!(loaded.discover_bundled_resources().len(), 2);

        // Verify allowed_tools collection for active skills (e.g. mcp-builder)
        let allowed = store.collect_active_skill_allowed_tools::<&str>(
            Some("build an mcp server with fastmcp"),
            None,
            &[],
        );
        assert!(allowed.contains(&"exec_cmd".to_string()));

        // Verify GitHub URL & anthropics/skills shorthand normalization
        assert_eq!(
            VaultStore::normalize_skill_import_url("anthropics/skills/mcp-builder"),
            "https://raw.githubusercontent.com/anthropics/skills/main/skills/mcp-builder/SKILL.md"
        );
        assert_eq!(
            VaultStore::normalize_skill_import_url(
                "https://github.com/anthropics/skills/tree/main/skills/webapp-testing"
            ),
            "https://raw.githubusercontent.com/anthropics/skills/main/skills/webapp-testing/SKILL.md"
        );
    }

    #[test]
    fn test_vault_full_crud_on_builtin_and_orchestrator_skills() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let store = VaultStore::new(ws);

        // 1. Read built-in orchestrator skill (`ask-user-inquiry`)
        let ask_skill = store.get_skill("ask-user-inquiry").unwrap();
        assert_eq!(ask_skill.scope, SkillScope::Builtin);
        assert_eq!(ask_skill.category(), "orchestrator");

        // 2. Surgically patch built-in orchestrator skill via patch_skill_full
        let patched = store
            .patch_skill_full(
                SkillScope::Project,
                "ask-user-inquiry",
                None,
                Some("4 to 6 distinct, well-explained options"),
                Some("4 to 6 distinct, well-explained options (Team Custom Rule)"),
                Some("Use when clarifying product scope with team custom batching rule"),
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(patched.scope, SkillScope::Project);
        assert!(patched
            .instructions
            .contains("4 to 6 distinct, well-explained options (Team Custom Rule)"));
        assert_eq!(
            patched.description,
            "Use when clarifying product scope with team custom batching rule"
        );

        // Verify MiniPowerEngine::format_brainstorm_prompt dynamically reflects the CRUD update
        let brainstorm_prompt = crate::agent::minipower::MiniPowerEngine::format_brainstorm_prompt(
            ws,
            "Custom OAuth Flow",
        );
        assert!(brainstorm_prompt
            .contains("4 to 6 distinct, well-explained options (Team Custom Rule)"));

        // 3. Delete built-in orchestrator skill (`ask-user-inquiry`) and verify tombstone suppresses it
        store
            .delete_skill(SkillScope::Project, "ask-user-inquiry")
            .unwrap();
        assert!(store.get_skill("ask-user-inquiry").is_err());
        assert!(!store
            .list_all_skills()
            .iter()
            .any(|s| s.name == "ask-user-inquiry"));

        // 4. Restore deleted built-in skill via load_to_project
        let restored = store.load_to_project("ask-user-inquiry").unwrap();
        assert_eq!(restored.name, "ask-user-inquiry");
        assert!(store.get_skill("ask-user-inquiry").is_ok());
    }
}
