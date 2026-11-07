//! MiniVault storage, multi-tier discovery, and CRUD engine.
//!
//! Modularized into focused domain submodules under `src/vault/store_impl/`:
//! - `skill_crud.rs` — skill creation, loading/unloading, surgical patching, deletion, remote import, and manifest active-skill mutation
//! - `context_and_bundles.rs` — stack token detection, skill-to-context scoring, progressive disclosure prompt assembly, and bundle management
//! - `tests.rs` — unit tests

#[path = "store_impl/context_and_bundles.rs"]
mod context_and_bundles;
#[path = "store_impl/skill_crud.rs"]
mod skill_crud;
#[cfg(test)]
#[path = "store_impl/tests.rs"]
mod tests;

use crate::vault::builtin::get_all_builtin_skills;
use crate::vault::models::{
    parse_skill_markdown, sanitize_skill_name, GlobalGotcha, LearnedSource, Result,
    SkillFrontmatter, SkillKind, SkillScope, SourceKind, VaultError, VaultSkill,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const VAULT_INDEX_MAGIC: [u8; 8] = *b"MCVLTSIX";
const VAULT_INDEX_VERSION: u32 = 1;
const VAULT_INDEX_FILE_NAME: &str = ".vault_index.bin";

/// Binary index record cached in `.minicode/vault/.vault_index.bin` and
/// `~/.config/minicode/vault/.vault_index.bin` via `bincode` + `memmap2`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultSkillIndexEntry {
    pub file_path: String,
    pub file_size: u64,
    pub mtime_secs: u64,
    pub mtime_nanos: u32,
    pub frontmatter: SkillFrontmatter,
    pub instructions: String,
    pub raw_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultBinaryIndex {
    magic: [u8; 8],
    version: u32,
    entries: std::collections::HashMap<String, VaultSkillIndexEntry>,
}

impl Default for VaultBinaryIndex {
    fn default() -> Self {
        Self {
            magic: VAULT_INDEX_MAGIC,
            version: VAULT_INDEX_VERSION,
            entries: std::collections::HashMap::new(),
        }
    }
}

fn vault_file_fingerprint(meta: &fs::Metadata) -> (u64, u64, u32) {
    let size = meta.len();
    let (secs, nanos) = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| (d.as_secs(), d.subsec_nanos()))
        .unwrap_or((0, 0));
    (size, secs, nanos)
}

fn load_mmap_vault_index(index_path: &Path) -> VaultBinaryIndex {
    let Ok(file) = fs::File::open(index_path) else {
        return VaultBinaryIndex::default();
    };
    let parsed: Option<VaultBinaryIndex> =
        if let Ok(mmap) = unsafe { memmap2::MmapOptions::new().map(&file) } {
            bincode::deserialize(&mmap[..]).ok()
        } else if let Ok(bytes) = fs::read(index_path) {
            bincode::deserialize(&bytes).ok()
        } else {
            None
        };

    match parsed {
        Some(idx) if idx.magic == VAULT_INDEX_MAGIC && idx.version == VAULT_INDEX_VERSION => idx,
        _ => VaultBinaryIndex::default(),
    }
}

fn save_mmap_vault_index(index_path: &Path, index: &VaultBinaryIndex) {
    let Ok(encoded) = bincode::serialize(index) else {
        return;
    };
    let Some(parent) = index_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let tmp_path = parent.join(format!(
        "{}.tmp.{}.{}",
        VAULT_INDEX_FILE_NAME,
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    if fs::write(&tmp_path, &encoded).is_ok() {
        if fs::rename(&tmp_path, index_path).is_err() {
            let _ = fs::remove_file(&tmp_path);
        }
    }
}

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

    /// Project-local binary skill header/content index: `.minicode/vault/.vault_index.bin`
    pub fn project_vault_index_file(&self) -> PathBuf {
        self.workspace_root
            .join(".minicode")
            .join("vault")
            .join(VAULT_INDEX_FILE_NAME)
    }

    /// Global binary skill header/content index: `~/.config/minicode/vault/.vault_index.bin`
    pub fn global_vault_index_file(&self) -> Option<PathBuf> {
        self.global_vault_dir()
            .map(|d| d.join(VAULT_INDEX_FILE_NAME))
    }

    fn upsert_mmap_vault_index_entry(
        &self,
        scope: SkillScope,
        file_path: &Path,
        frontmatter: &SkillFrontmatter,
        instructions: &str,
        raw_content: &str,
    ) {
        let Some(index_path) = (match scope {
            SkillScope::Global => self.global_vault_index_file(),
            SkillScope::Project | SkillScope::Builtin => Some(self.project_vault_index_file()),
        }) else {
            return;
        };
        let Ok(fs_meta) = fs::metadata(file_path) else {
            return;
        };
        let (file_size, mtime_secs, mtime_nanos) = vault_file_fingerprint(&fs_meta);
        let key = file_path.to_string_lossy().to_string();
        let mut index = load_mmap_vault_index(&index_path);
        index.entries.insert(
            key.clone(),
            VaultSkillIndexEntry {
                file_path: key,
                file_size,
                mtime_secs,
                mtime_nanos,
                frontmatter: frontmatter.clone(),
                instructions: instructions.to_string(),
                raw_content: raw_content.to_string(),
            },
        );
        save_mmap_vault_index(&index_path, &index);
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

            let scan_report = crate::vault::skillspector::SkillSpectorEngine::scan_raw_markdown(
                &clean_name,
                "SKILL.md",
                &instructions,
                target_scope,
            );
            self.enforce_security_gate(&scan_report)?;

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

            let scan_report = crate::vault::skillspector::SkillSpectorEngine::scan_raw_markdown(
                &clean_name,
                "SKILL.md",
                &instructions,
                target_scope,
            );
            self.enforce_security_gate(&scan_report)?;

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
    /// Uses a memory-mapped `bincode` binary index (`.vault_index.bin`) so unchanged `.md` files
    /// are loaded without re-reading or re-parsing YAML frontmatter on every turn.
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

        // 2. Global User Skills (Tier 2) — backed by global `.vault_index.bin`
        let global_idx_path = self.global_vault_index_file();
        let mut global_index = global_idx_path
            .as_deref()
            .map(load_mmap_vault_index)
            .unwrap_or_default();
        let mut global_dirty = false;
        let mut global_live_keys = std::collections::HashSet::new();

        if let Some(global_dir) = self.global_skills_dir() {
            self.scan_directory_into_map(
                &global_dir,
                SkillScope::Global,
                Some(SkillKind::Workflow),
                false,
                &mut skill_map,
                &mut global_index,
                &mut global_dirty,
                &mut global_live_keys,
            );
        }
        if let Some(legacy_dir) = self.legacy_global_skills_dir() {
            self.scan_directory_into_map(
                &legacy_dir,
                SkillScope::Global,
                Some(SkillKind::Workflow),
                false,
                &mut skill_map,
                &mut global_index,
                &mut global_dirty,
                &mut global_live_keys,
            );
        }

        let prev_global_len = global_index.entries.len();
        global_index
            .entries
            .retain(|k, _| global_live_keys.contains(k));
        if global_index.entries.len() != prev_global_len {
            global_dirty = true;
        }
        if global_dirty {
            if let Some(ref idx_path) = global_idx_path {
                if !global_index.entries.is_empty() || idx_path.exists() {
                    save_mmap_vault_index(idx_path, &global_index);
                }
            }
        }

        // 3. Project Skills & Knowledge Domains (Tier 3 - highest precedence) — backed by `.minicode/vault/.vault_index.bin`
        let proj_idx_path = self.project_vault_index_file();
        let mut proj_index = load_mmap_vault_index(&proj_idx_path);
        let mut proj_dirty = false;
        let mut proj_live_keys = std::collections::HashSet::new();

        // 3a. Primary project skills (.minicode/skills/)
        let project_dir = self.project_skills_dir();
        self.scan_directory_into_map(
            &project_dir,
            SkillScope::Project,
            Some(SkillKind::Workflow),
            true,
            &mut skill_map,
            &mut proj_index,
            &mut proj_dirty,
            &mut proj_live_keys,
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
                &mut proj_index,
                &mut proj_dirty,
                &mut proj_live_keys,
            );

            // Tier 3c: runtimes/ or stack subdirectories (e.g. rust/, python/, flutter/) -> Runtime Workflows
            let runtimes_dir = docs_dir.join("runtimes");
            self.scan_directory_into_map(
                &runtimes_dir,
                SkillScope::Project,
                Some(SkillKind::Runtime),
                false,
                &mut skill_map,
                &mut proj_index,
                &mut proj_dirty,
                &mut proj_live_keys,
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
                                &mut proj_index,
                                &mut proj_dirty,
                                &mut proj_live_keys,
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
                &mut proj_index,
                &mut proj_dirty,
                &mut proj_live_keys,
            );
        }

        // Tier 3e: Root DESIGN.md (Google Stitch / VoltAgent awesome-design-md standard)
        // Automatically discovers a root `DESIGN.md` file dropped into the workspace root and activates it
        // as an authoritative `SkillKind::Core` design system specification.
        let root_design_md = self.workspace_root.join("DESIGN.md");
        if root_design_md.is_file() && !deleted.contains("design") {
            if let Ok(content) = fs::read_to_string(&root_design_md) {
                if !content.trim().is_empty() {
                    let (mut fm, instructions) = parse_skill_markdown(&content, "design");
                    fm.name = "design".to_string();
                    fm.kind = Some(SkillKind::Core.as_str().to_string());
                    fm.category = Some("ui-styling".to_string());
                    fm.always_apply = true;
                    if fm.triggers.is_empty() {
                        fm.triggers = vec![
                            "design".to_string(),
                            "design.md".to_string(),
                            "design-md".to_string(),
                            "ui".to_string(),
                            "theme".to_string(),
                            "styling".to_string(),
                        ];
                    }
                    skill_map.insert(
                        "design".to_string(),
                        VaultSkill {
                            name: "design".to_string(),
                            description: fm.description.clone(),
                            scope: SkillScope::Project,
                            frontmatter: fm,
                            instructions,
                            raw_content: content,
                            path: Some(root_design_md),
                            is_active_in_project: true,
                        },
                    );
                }
            }
        }

        let prev_proj_len = proj_index.entries.len();
        proj_index.entries.retain(|k, _| proj_live_keys.contains(k));
        if proj_index.entries.len() != prev_proj_len {
            proj_dirty = true;
        }
        if proj_dirty && (!proj_index.entries.is_empty() || proj_idx_path.exists()) {
            save_mmap_vault_index(&proj_idx_path, &proj_index);
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

    /// Scans a directory for `.md` files or `<name>/SKILL.md` folders and inserts them into `map`,
    /// utilizing the memory-mapped `VaultBinaryIndex` when `(file_size, mtime_secs, mtime_nanos)` matches.
    #[allow(clippy::too_many_arguments)]
    fn scan_directory_into_map(
        &self,
        dir: &Path,
        scope: SkillScope,
        default_kind: Option<SkillKind>,
        is_active: bool,
        map: &mut std::collections::BTreeMap<String, VaultSkill>,
        index: &mut VaultBinaryIndex,
        index_dirty: &mut bool,
        live_keys: &mut std::collections::HashSet<String>,
    ) {
        if !dir.exists() || !dir.is_dir() {
            return;
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut load_into_map =
            |file_path: PathBuf,
             fallback_name: &str,
             map: &mut std::collections::BTreeMap<String, VaultSkill>| {
                let Ok(fs_meta) = fs::metadata(&file_path) else {
                    return;
                };
                let (file_size, mtime_secs, mtime_nanos) = vault_file_fingerprint(&fs_meta);
                let key = file_path.to_string_lossy().to_string();
                live_keys.insert(key.clone());

                if let Some(cached) = index.entries.get(&key).filter(|e| {
                    e.file_size == file_size
                        && e.mtime_secs == mtime_secs
                        && e.mtime_nanos == mtime_nanos
                }) {
                    let mut fm = cached.frontmatter.clone();
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
                        instructions: cached.instructions.clone(),
                        raw_content: cached.raw_content.clone(),
                        path: Some(file_path),
                        is_active_in_project: is_active,
                    };
                    map.insert(skill.name.to_lowercase(), skill);
                    return;
                }

                if let Ok(content) = fs::read_to_string(&file_path) {
                    let (mut fm, instructions) = parse_skill_markdown(&content, fallback_name);
                    if fm.kind.is_none() {
                        if let Some(dk) = default_kind {
                            fm.kind = Some(dk.as_str().to_string());
                        }
                    }
                    index.entries.insert(
                        key.clone(),
                        VaultSkillIndexEntry {
                            file_path: key,
                            file_size,
                            mtime_secs,
                            mtime_nanos,
                            frontmatter: fm.clone(),
                            instructions: instructions.clone(),
                            raw_content: content.clone(),
                        },
                    );
                    *index_dirty = true;

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
}
