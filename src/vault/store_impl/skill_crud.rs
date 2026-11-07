//! Skill CRUD, project loading/unloading, remote URL import, and manifest active-skill mutation for [`VaultStore`].

use super::*;
use crate::vault::models::{
    format_skill_markdown, parse_skill_markdown, sanitize_skill_name, Result, SkillFrontmatter,
    SkillScope, VaultError, VaultSkill,
};
use std::fs;
use std::path::Path;

impl VaultStore {
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
        self.create_skill_with_tools(
            scope,
            name,
            description,
            category,
            kind,
            instructions,
            triggers,
            globs,
            Vec::new(),
        )
    }

    /// Creates a new skill in the target scope (`Project` or `Global`) with optional `allowed_tools`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_skill_with_tools(
        &self,
        scope: SkillScope,
        name: &str,
        description: &str,
        category: Option<&str>,
        kind: Option<&str>,
        instructions: &str,
        triggers: Vec<String>,
        globs: Vec<String>,
        allowed_tools: Vec<String>,
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
            allowed_tools,
        };

        let content = format_skill_markdown(&fm, instructions);
        let scan_report = crate::vault::skillspector::SkillSpectorEngine::scan_raw_markdown(
            &clean_name,
            "SKILL.md",
            &content,
            effective_scope,
        );
        self.enforce_security_gate(&scan_report)?;

        fs::create_dir_all(&target_dir).map_err(|e| VaultError::Io {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        self.clear_skill_tombstone(&clean_name);
        let target_file = target_dir.join("SKILL.md");

        fs::write(&target_file, &content).map_err(|e| VaultError::Io {
            path: target_file.display().to_string(),
            source: e,
        })?;

        let clean_instructions = instructions.trim().to_string();
        self.upsert_mmap_vault_index_entry(
            effective_scope,
            &target_file,
            &fm,
            &clean_instructions,
            &content,
        );

        if effective_scope == SkillScope::Project {
            let _ = self.add_to_manifest_active_skills(&clean_name);
        }

        Ok(VaultSkill {
            name: clean_name,
            description: fm.description.clone(),
            scope: effective_scope,
            frontmatter: fm,
            instructions: clean_instructions,
            raw_content: content,
            path: Some(target_file),
            is_active_in_project: effective_scope == SkillScope::Project,
        })
    }

    /// Enforces the fail-closed SkillSpector pre-install security gate.
    /// Rejects skills with `safe_to_install == false` (`risk_score > 50` / `DO_NOT_INSTALL`).
    pub fn enforce_security_gate(
        &self,
        report: &crate::vault::skillspector::SkillScanReport,
    ) -> Result<()> {
        if !report.safe_to_install {
            let top_findings: Vec<String> = report
                .findings
                .iter()
                .take(4)
                .map(|f| {
                    format!(
                        "[{} {}] {} ({}:{})",
                        f.id,
                        f.severity.as_str(),
                        f.title,
                        f.file,
                        f.line
                    )
                })
                .collect();
            return Err(VaultError::SecurityGateRejected {
                skill: report.skill_name.clone(),
                score: report.risk_score,
                recommendation: report.recommendation.as_str().to_string(),
                summary: top_findings.join("; "),
            });
        }
        Ok(())
    }

    /// Scans a skill registered in the vault by name using `SkillSpectorEngine`.
    pub fn scan_skill(
        &self,
        name: &str,
        use_baseline: bool,
    ) -> Result<crate::vault::skillspector::SkillScanReport> {
        let skill = self.get_skill(name)?;
        let baseline = use_baseline.then(|| {
            crate::vault::skillspector::SkillSpectorBaseline::load_for_workspace(
                &self.workspace_root,
            )
        });
        Ok(
            crate::vault::skillspector::SkillSpectorEngine::scan_vault_skill(
                &skill,
                baseline.as_ref(),
            ),
        )
    }

    /// Scans an arbitrary local directory or file path using `SkillSpectorEngine`.
    pub fn scan_path(
        &self,
        path: &Path,
        use_baseline: bool,
    ) -> crate::vault::skillspector::SkillScanReport {
        let baseline = use_baseline.then(|| {
            crate::vault::skillspector::SkillSpectorBaseline::load_for_workspace(
                &self.workspace_root,
            )
        });
        crate::vault::skillspector::SkillSpectorEngine::scan_directory_or_file(
            path,
            baseline.as_ref(),
        )
    }

    /// Loads a skill from Built-in or Global vault into the active Project (`.minicode/skills/<name>/SKILL.md`).
    /// Also clears any deletion tombstone if a previously deleted built-in skill is loaded.
    pub fn load_to_project(&self, name: &str) -> Result<VaultSkill> {
        let clean_name = sanitize_skill_name(name);
        self.clear_skill_tombstone(&clean_name);
        let source_skill = self.get_skill(&clean_name)?;

        // Pre-install SkillSpector security gate for non-Builtin skills
        if source_skill.scope != SkillScope::Builtin {
            let baseline = crate::vault::skillspector::SkillSpectorBaseline::load_for_workspace(
                &self.workspace_root,
            );
            let report = crate::vault::skillspector::SkillSpectorEngine::scan_vault_skill(
                &source_skill,
                Some(&baseline),
            );
            self.enforce_security_gate(&report)?;
        }

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

        self.upsert_mmap_vault_index_entry(
            SkillScope::Project,
            &target_file,
            &source_skill.frontmatter,
            &source_skill.instructions,
            &source_skill.raw_content,
        );

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
            let lower = fname_str.to_ascii_lowercase();
            if matches!(fname_str.as_ref(), "__pycache__" | ".DS_Store")
                || crate::constants::SKILLSPECTOR_BINARY_EXTENSIONS
                    .iter()
                    .any(|ext| lower.ends_with(ext))
            {
                continue;
            }
            let ty = entry.file_type()?;
            let dst_path = dst.join(&fname);
            if ty.is_dir() {
                Self::copy_dir_recursive(&entry.path(), &dst_path)?;
            } else if ty.is_file() {
                if let Ok(meta) = entry.metadata() {
                    if meta.len() > crate::constants::SKILLSPECTOR_MAX_FILE_BYTES {
                        continue;
                    }
                }
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
        let scan_report = crate::vault::skillspector::SkillSpectorEngine::scan_raw_markdown(
            &existing.name,
            "SKILL.md",
            &new_content,
            effective_scope,
        );
        self.enforce_security_gate(&scan_report)?;

        fs::write(&target_path, &new_content).map_err(|e| VaultError::Io {
            path: target_path.display().to_string(),
            source: e,
        })?;

        self.upsert_mmap_vault_index_entry(
            effective_scope,
            &target_path,
            &fm,
            &updated_instructions,
            &new_content,
        );

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

    /// Normalizes a brand slug for `VoltAgent/awesome-design-md` into its canonical directory name.
    pub fn normalize_awesome_design_slug(raw: &str) -> String {
        let slug = raw
            .trim()
            .trim_matches('/')
            .trim_start_matches("design-md/")
            .trim_end_matches("/DESIGN.md")
            .trim_end_matches("/design.md")
            .trim_end_matches("/design-md")
            .to_lowercase();
        match slug.as_str() {
            "linear" => "linear.app".to_string(),
            "x" | "twitter" => "x.com".to_string(),
            "cal" => "cal.com".to_string(),
            other => other.to_string(),
        }
    }

    /// Normalizes GitHub `/blob/`, `/tree/`, `anthropics/skills/<name>`, or `awesome-design-md/<brand>`
    /// shorthand URLs into raw markdown URLs ready for direct HTTP download.
    pub fn normalize_skill_import_url(url: &str) -> String {
        let trimmed = url.trim().trim_end_matches('/');

        // 0. Shorthand: `VoltAgent/awesome-design-md` (`awesome-design-md/<brand>`, `design-md:<brand>`, `getdesign.md/<brand>`)
        if let Some(rest) = trimmed
            .strip_prefix("awesome-design-md/")
            .or_else(|| trimmed.strip_prefix("awesome-design-md:"))
            .or_else(|| trimmed.strip_prefix("voltagent/awesome-design-md/"))
            .or_else(|| trimmed.strip_prefix("VoltAgent/awesome-design-md/"))
            .or_else(|| trimmed.strip_prefix("design-md/"))
            .or_else(|| trimmed.strip_prefix("design-md:"))
            .or_else(|| trimmed.strip_prefix("https://getdesign.md/"))
            .or_else(|| trimmed.strip_prefix("http://getdesign.md/"))
            .or_else(|| trimmed.strip_prefix("getdesign.md/"))
        {
            let slug = Self::normalize_awesome_design_slug(rest);
            if !slug.is_empty() {
                return format!(
                    "https://raw.githubusercontent.com/VoltAgent/awesome-design-md/main/design-md/{}/DESIGN.md",
                    slug
                );
            }
        }

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
                if owner.eq_ignore_ascii_case("voltagent")
                    && repo.eq_ignore_ascii_case("awesome-design-md")
                {
                    let slug = Self::normalize_awesome_design_slug(&subpath);
                    return format!(
                        "https://raw.githubusercontent.com/VoltAgent/awesome-design-md/main/design-md/{}/DESIGN.md",
                        slug
                    );
                }
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
                    if repo.eq_ignore_ascii_case("awesome-design-md")
                        || subpath.starts_with("design-md/")
                    {
                        format!("{}/DESIGN.md", subpath)
                    } else {
                        format!("{}/SKILL.md", subpath)
                    }
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

        let is_design_md =
            resolved_url.ends_with("/DESIGN.md") || resolved_url.ends_with("/design.md");
        let default_derived_name = if is_design_md {
            let parent_slug = resolved_url
                .trim_end_matches("/DESIGN.md")
                .trim_end_matches("/design.md")
                .split('/')
                .next_back()
                .unwrap_or("brand");
            format!("design-md-{}", sanitize_skill_name(parent_slug))
        } else {
            resolved_url
                .trim_end_matches("/SKILL.md")
                .split('/')
                .next_back()
                .unwrap_or("imported-skill")
                .trim_end_matches(".md")
                .to_string()
        };
        let fallback_name = name_override.unwrap_or(&default_derived_name);

        let (mut fm, instructions) = parse_skill_markdown(&body, fallback_name);
        if let Some(override_name) = name_override {
            fm.name = sanitize_skill_name(override_name);
        } else if is_design_md && !fm.name.starts_with("design-md-") {
            fm.name = sanitize_skill_name(&format!("design-md-{}", fm.name));
        }

        // Pre-install SkillSpector security gate before writing remote skill to disk
        let scan_report = crate::vault::skillspector::SkillSpectorEngine::scan_raw_markdown(
            &fm.name,
            "SKILL.md",
            &body,
            target_scope,
        );
        self.enforce_security_gate(&scan_report)?;

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

    pub(super) fn mutate_manifest_active_skills<F>(&self, mutator: F) -> Result<()>
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
                    if let Some(obj) = val.as_object_mut() {
                        if !obj.get("active_skills").is_some_and(|v| v.is_array()) {
                            obj.insert(
                                "active_skills".to_string(),
                                serde_json::Value::Array(Vec::new()),
                            );
                        }
                    }
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
    pub(super) fn add_to_manifest_active_skills(&self, skill_name: &str) -> Result<()> {
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
    pub(super) fn remove_from_manifest_active_skills(&self, skill_name: &str) -> Result<()> {
        let target_lower = skill_name.to_lowercase();
        self.mutate_manifest_active_skills(|arr| {
            let before = arr.len();
            arr.retain(|s| s.as_str().map(|n| n.to_lowercase()) != Some(target_lower.clone()));
            arr.len() != before
        })
    }
}
