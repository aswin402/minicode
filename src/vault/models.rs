//! Domain models, scopes, and frontmatter representation for MiniVault.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("Skill '{0}' not found in vault")]
    NotFound(String),

    #[error("Skill '{0}' already exists in target scope")]
    AlreadyExists(String),

    #[error("File operation failed for '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Network download failed for '{url}': {reason}")]
    Network { url: String, reason: String },

    #[error("Invalid skill definition: {0}")]
    InvalidSkill(String),

    #[error("Invalid frontmatter in '{path}': {reason}")]
    Frontmatter { path: String, reason: String },
}

pub type Result<T> = std::result::Result<T, VaultError>;

/// Storage scope of a skill in the vault hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    /// Embedded core skill compiled directly into the binary
    Builtin,
    /// User-level skill stored in ~/.config/minicode/vault/skills/
    Global,
    /// Repository-level skill stored in .minicode/skills/ or minikit_docs/skills/
    Project,
}

impl SkillScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillScope::Builtin => "builtin",
            SkillScope::Global => "global",
            SkillScope::Project => "project",
        }
    }
}

/// Functional classification of an entry in MiniVault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillKind {
    /// Technical Reference Guide (framework/library documentation, API cheatsheet, package gotchas)
    /// Example: `react`, `vite`, `tailwind-v4`, `postgres`, `docker`, `express`, `prisma`, `mongodb`
    Reference,
    /// Behavioral / Operational Skill (methodology, protocols, decision-making, quality gates)
    /// Example: `tdd-workflow`, `systematic-debugging`, `security-audit`, `frontend-design`, `ui-ux-pro-max`
    Workflow,
    /// Runtime & Stack Workflow Guide (ecosystem conventions, build safety, script rules)
    /// Example: `rust-tokio`, `bun-workflow`, `flutter-workflow`
    Runtime,
    /// Core Project Invariant / System Contract
    /// Example: `coreidea`, `prd`, `architecture`, `spec`, `todo`
    Core,
}

impl SkillKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillKind::Reference => "reference",
            SkillKind::Workflow => "workflow",
            SkillKind::Runtime => "runtime",
            SkillKind::Core => "core",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            SkillKind::Reference => "Doc",
            SkillKind::Workflow => "Skill",
            SkillKind::Runtime => "Runtime",
            SkillKind::Core => "Core",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            SkillKind::Reference => "Technical Reference Guide (Cheatsheet / Invariants)",
            SkillKind::Workflow => "Behavioral Skill (Operational Methodology)",
            SkillKind::Runtime => "Runtime & Stack Workflow (Architecture Conventions)",
            SkillKind::Core => "Core Project Invariant (Authoritative Specification)",
        }
    }
}

impl std::fmt::Display for SkillKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for SkillKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().trim() {
            "reference" | "ref" | "doc" | "docs" | "guide" | "package" | "packages"
            | "cheatsheet" | "packagedoc" => Ok(Self::Reference),
            "workflow" | "skill" | "behavior" | "methodology" | "protocol" => Ok(Self::Workflow),
            "runtime" | "stack" | "toolchain" | "architecture" => Ok(Self::Runtime),
            "core" | "spec" | "contract" | "invariants" => Ok(Self::Core),
            other => Err(format!(
                "Invalid skill kind '{}'. Valid kinds: 'reference', 'workflow', 'runtime', 'core'",
                other
            )),
        }
    }
}

/// Classification of an external learned source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Web documentation, official guide, or cheatsheet URL (e.g. docs.rs, nextjs.org)
    WebDoc,
    /// Reference repository or local template directory
    RepoTemplate,
    /// Software package, library, or crate documentation
    PackageGuide,
    /// MCP tool, server integration, or CLI assistant
    McpTool,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WebDoc => "web_doc",
            Self::RepoTemplate => "repo_template",
            Self::PackageGuide => "package_guide",
            Self::McpTool => "mcp_tool",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::WebDoc => "WebDoc",
            Self::RepoTemplate => "Repo",
            Self::PackageGuide => "Pkg",
            Self::McpTool => "MCP",
        }
    }
}

/// An ingested and bookmarked external knowledge source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LearnedSource {
    pub id: String,
    pub uri: String,
    pub title: String,
    pub kind: SourceKind,
    pub summary: String,
    pub tags: Vec<String>,
    pub extracted_skill_or_doc: Option<String>,
    pub created_at: String,
    pub last_referenced: String,
}

fn matches_text_or_tokens(q: &str, fields: &[&str], tags: &[String]) -> bool {
    let q_lower = q.to_lowercase();
    let lower_fields: Vec<String> = fields.iter().map(|f| f.to_lowercase()).collect();
    let lower_tags: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();

    if lower_fields.iter().any(|f| f.contains(&q_lower))
        || lower_tags.iter().any(|t| t.contains(&q_lower))
    {
        return true;
    }

    q_lower
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(str::trim)
        .filter(|t| t.len() >= 3)
        .any(|token| {
            lower_fields.iter().any(|f| f.contains(token))
                || lower_tags.iter().any(|t| t.contains(token))
        })
}

impl LearnedSource {
    pub fn matches_query(&self, q: &str) -> bool {
        matches_text_or_tokens(q, &[&self.uri, &self.title, &self.summary], &self.tags)
    }
}

/// A universal, cross-project learned technical trap, compiler gotcha, or negative knowledge invariant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalGotcha {
    pub id: String,
    pub trigger: String,
    pub context_scope: String,
    pub failed_attempt: String,
    pub verified_fix: String,
    pub created_at: String,
    pub occurrence_count: usize,
}

impl GlobalGotcha {
    pub fn matches_query(&self, q: &str) -> bool {
        matches_text_or_tokens(
            q,
            &[
                &self.trigger,
                &self.context_scope,
                &self.failed_attempt,
                &self.verified_fix,
            ],
            &[],
        )
    }

    pub fn format_prompt_block(&self) -> String {
        format!(
            "• [{}] Trap: {}\n    Avoid: {}\n    Fix:   {}",
            self.context_scope, self.trigger, self.failed_attempt, self.verified_fix
        )
    }
}

/// Metadata extracted from the YAML frontmatter of a `SKILL.md` or `.md` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub globs: Vec<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub always_apply: bool,
    #[serde(default, alias = "allowed-tools")]
    pub allowed_tools: Vec<String>,
}

impl SkillFrontmatter {
    /// Returns the category name or 'general' if unspecified.
    pub fn category_name(&self) -> &str {
        match &self.category {
            Some(c) if !c.trim().is_empty() => c.trim(),
            _ => "general",
        }
    }

    /// Returns the functional kind (`SkillKind::Reference`, `SkillKind::Workflow`, `SkillKind::Runtime`, `SkillKind::Core`).
    /// Defaults to `Workflow` for testing/quality/security, `Runtime` for runtime/stack, `Core` for core/spec, and `Reference` for package documentation.
    pub fn skill_kind(&self) -> SkillKind {
        if let Some(k) = &self.kind {
            if let Ok(kind) = k.parse::<SkillKind>() {
                return kind;
            }
        }
        match self.category_name() {
            "testing" | "quality" | "security" | "workflow" => SkillKind::Workflow,
            "runtime" | "stack" | "toolchain" => SkillKind::Runtime,
            "core" | "spec" | "architecture" => SkillKind::Core,
            _ => SkillKind::Reference,
        }
    }
}

impl Default for SkillFrontmatter {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            version: default_version(),
            author: None,
            category: None,
            kind: None,
            globs: Vec::new(),
            triggers: Vec::new(),
            always_apply: false,
            allowed_tools: Vec::new(),
        }
    }
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// A named collection of related skills that can be loaded into a project as a single cohesive pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultBundle {
    pub name: String,
    pub description: String,
    #[serde(default = "default_bundle_category")]
    pub category: String,
    pub skills: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_bundle_scope")]
    pub scope: SkillScope,
    #[serde(default)]
    pub path: Option<PathBuf>,
}

fn default_bundle_category() -> String {
    "general".to_string()
}

fn default_bundle_scope() -> SkillScope {
    SkillScope::Builtin
}

impl VaultBundle {
    /// Checks if this bundle matches a search query across name, description, category, and tags.
    pub fn matches_query(&self, query_lower: &str) -> bool {
        if query_lower.is_empty() {
            return true;
        }
        self.name.to_lowercase().contains(query_lower)
            || self.description.to_lowercase().contains(query_lower)
            || self.category.to_lowercase().contains(query_lower)
            || self
                .tags
                .iter()
                .any(|t| t.to_lowercase().contains(query_lower))
            || self
                .skills
                .iter()
                .any(|s| s.to_lowercase().contains(query_lower))
    }
}

/// A fully parsed skill stored in MiniVault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSkill {
    pub name: String,
    pub description: String,
    pub scope: SkillScope,
    pub frontmatter: SkillFrontmatter,
    pub instructions: String,
    pub raw_content: String,
    pub path: Option<PathBuf>,
    pub is_active_in_project: bool,
}

impl VaultSkill {
    /// Returns the functional kind of this skill (Reference Doc vs Behavioral Workflow Skill).
    pub fn kind(&self) -> SkillKind {
        self.frontmatter.skill_kind()
    }

    /// Returns the category of this skill (e.g. 'frontend', 'backend', 'testing', 'security').
    pub fn category(&self) -> &str {
        self.frontmatter.category_name()
    }
    /// Checks if this skill matches a file path based on its globs or always_apply rule.
    pub fn matches_file(&self, workspace_root: &Path, file_path: &Path) -> bool {
        if self.frontmatter.always_apply {
            return true;
        }
        if self.frontmatter.globs.is_empty() {
            return false;
        }

        let rel_path = if file_path.is_absolute() {
            file_path.strip_prefix(workspace_root).unwrap_or(file_path)
        } else {
            file_path
        };

        let mut builder = ignore::overrides::OverrideBuilder::new(workspace_root);
        for g in &self.frontmatter.globs {
            let _ = builder.add(g);
        }
        if let Ok(overrides) = builder.build() {
            overrides.matched(rel_path, false).is_whitelist()
        } else {
            false
        }
    }

    /// Checks if this skill matches user prompt keywords via triggers or name.
    pub fn matches_prompt(&self, prompt_lower: &str) -> bool {
        let name_lower = self.name.to_lowercase();
        if crate::utils::has_word(prompt_lower, &name_lower) || prompt_lower.contains(&name_lower) {
            return true;
        }

        for trigger in &self.frontmatter.triggers {
            let trig_lower = trigger.to_lowercase();
            if trig_lower.is_empty() {
                continue;
            }
            if trig_lower.contains(' ') || trig_lower.contains('-') || trig_lower.contains('.') {
                if prompt_lower.contains(&trig_lower) {
                    return true;
                }
            } else {
                let matched = prompt_lower
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|w| {
                        w.eq_ignore_ascii_case(&trig_lower)
                            || (trig_lower.len() >= 4
                                && w.get(..trig_lower.len())
                                    .is_some_and(|p| p.eq_ignore_ascii_case(&trig_lower)))
                    });
                if matched {
                    return true;
                }
            }
        }

        false
    }

    /// Discovers Level-3 progressive disclosure resources (`scripts/`, `references/`, `assets/`, `templates/`, `themes/`, `examples/`, `ooxml/`)
    /// bundled alongside a directory-backed `SKILL.md`.
    pub fn discover_bundled_resources(&self) -> Vec<String> {
        let Some(ref skill_path) = self.path else {
            return Vec::new();
        };
        if skill_path.file_name().and_then(|s| s.to_str()) != Some("SKILL.md") {
            return Vec::new();
        }
        let Some(skill_dir) = skill_path.parent() else {
            return Vec::new();
        };

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

        let mut resources = Vec::new();
        for subdir_name in RESOURCE_DIRS {
            let subdir = skill_dir.join(subdir_name);
            if !subdir.is_dir() {
                continue;
            }
            for entry in ignore::WalkBuilder::new(&subdir)
                .hidden(false)
                .git_ignore(true)
                .max_depth(Some(4))
                .build()
                .flatten()
            {
                let p = entry.path();
                if p.is_file() {
                    if let Ok(rel) = p.strip_prefix(skill_dir) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        if rel_str.contains("__pycache__/")
                            || rel_str.ends_with(".pyc")
                            || rel_str.ends_with(".pyo")
                            || rel_str.ends_with(".DS_Store")
                        {
                            continue;
                        }
                        resources.push(rel_str);
                    }
                }
            }
        }
        resources.sort();
        resources.dedup();
        resources
    }
}

/// Automatically infers relevant trigger keywords from a skill's `name` and `description`
/// when explicit `triggers` are omitted during skill creation or import.
pub fn infer_triggers_from_metadata(name: &str, description: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "the", "and", "for", "with", "when", "use", "this", "skill", "that", "from", "into",
        "your", "are", "can", "will", "should", "user", "asks", "guide", "patterns", "using",
        "create", "creating", "build", "building", "modern", "how", "what", "any", "all",
    ];
    let mut triggers = Vec::new();
    let clean_name = sanitize_skill_name(name);
    if !clean_name.is_empty() {
        triggers.push(clean_name.clone());
        for part in clean_name.split('-') {
            if part.len() >= 3 && !STOP_WORDS.contains(&part) && !triggers.iter().any(|t| t == part)
            {
                triggers.push(part.to_string());
            }
        }
    }

    for word in description
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '.')
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
    {
        if word.len() >= 4
            && !STOP_WORDS.contains(&word.as_str())
            && !triggers.iter().any(|t| t == &word)
        {
            triggers.push(word);
            if triggers.len() >= 8 {
                break;
            }
        }
    }

    triggers
}

/// Sanitizes a string into a safe skill identifier (kebab-case alphanumeric).
pub fn sanitize_skill_name(name: &str) -> String {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    // Deduplicate consecutive dashes and strip leading/trailing dashes
    let mut deduped = String::new();
    let mut last_dash = false;
    for c in clean.chars() {
        if c == '-' {
            if !last_dash && !deduped.is_empty() {
                deduped.push('-');
            }
            last_dash = true;
        } else {
            deduped.push(c);
            last_dash = false;
        }
    }
    deduped.trim_matches('-').to_string()
}

/// Parses raw markdown with optional YAML frontmatter into `SkillFrontmatter` and `instructions`.
pub fn parse_skill_markdown(content: &str, fallback_name: &str) -> (SkillFrontmatter, String) {
    let mut fm = SkillFrontmatter::default();
    let mut instructions = content.to_string();

    if let Some(stripped) = content.strip_prefix("---") {
        if let Some((end_idx, delim_len)) = stripped
            .find("\n---")
            .map(|i| (i, 4))
            .or_else(|| stripped.find("---").map(|i| (i, 3)))
        {
            let frontmatter_str = &stripped[..end_idx];
            instructions = stripped[end_idx + delim_len..].trim().to_string();

            parse_yaml_frontmatter_fields(frontmatter_str, &mut fm);
        }
    }

    if fm.name.is_empty() {
        fm.name = sanitize_skill_name(fallback_name);
    }

    if fm.description.is_empty() {
        // Fall back to first non-empty line or heading
        fm.description = instructions
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().trim_start_matches('#').trim().to_string())
            .unwrap_or_else(|| format!("Domain skill for {}", fm.name));
    }

    (fm, instructions)
}

fn unquote_yaml(v: &str) -> &str {
    v.trim().trim_matches('"').trim_matches('\'').trim()
}

fn parse_yaml_list_value(
    v: &str,
    key: &'static str,
    target: &mut Vec<String>,
    current_list_key: &mut Option<&'static str>,
) {
    if v.starts_with('[') && v.ends_with(']') {
        *current_list_key = None;
        *target = parse_inline_list(v);
    } else if v.is_empty() {
        *current_list_key = Some(key);
    } else {
        *current_list_key = None;
        let clean = unquote_yaml(v);
        if !clean.is_empty() {
            target.push(clean.to_string());
        }
    }
}

/// Parses standard YAML frontmatter fields into `SkillFrontmatter`.
fn parse_yaml_frontmatter_fields(frontmatter: &str, fm: &mut SkillFrontmatter) {
    let mut current_list_key: Option<&'static str> = None;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // List item continuation (- item)
        if trimmed.starts_with('-') && current_list_key.is_some() {
            let clean_item = unquote_yaml(trimmed.trim_start_matches('-')).to_string();
            if !clean_item.is_empty() {
                match current_list_key {
                    Some("globs") => fm.globs.push(clean_item),
                    Some("triggers") => fm.triggers.push(clean_item),
                    Some("allowed_tools") => fm.allowed_tools.push(clean_item),
                    _ => {}
                }
            }
            continue;
        }

        // Key-value pair
        if let Some((key, val)) = line.split_once(':') {
            let k = key.trim().to_lowercase();
            let v = val.trim();
            let clean = unquote_yaml(v);

            match k.as_str() {
                "name" => {
                    current_list_key = None;
                    if !clean.is_empty() {
                        fm.name = clean.to_string();
                    }
                }
                "description" => {
                    current_list_key = None;
                    if !clean.is_empty() {
                        fm.description = clean.to_string();
                    }
                }
                "version" => {
                    current_list_key = None;
                    if !clean.is_empty() {
                        fm.version = clean.to_string();
                    }
                }
                "author" => {
                    current_list_key = None;
                    if !clean.is_empty() {
                        fm.author = Some(clean.to_string());
                    }
                }
                "category" => {
                    current_list_key = None;
                    if !clean.is_empty() {
                        fm.category = Some(clean.to_lowercase());
                    }
                }
                "kind" | "type" => {
                    current_list_key = None;
                    if !clean.is_empty() {
                        fm.kind = Some(clean.to_lowercase());
                    }
                }
                "always_apply" => {
                    current_list_key = None;
                    fm.always_apply = v.eq_ignore_ascii_case("true");
                }
                "globs" => {
                    parse_yaml_list_value(v, "globs", &mut fm.globs, &mut current_list_key);
                }
                "triggers" => {
                    parse_yaml_list_value(v, "triggers", &mut fm.triggers, &mut current_list_key);
                }
                "allowed_tools" | "allowed-tools" => {
                    if !v.starts_with('[') && (v.contains(' ') || v.contains(',')) {
                        current_list_key = None;
                        for token in v.split(|c: char| c == ',' || c.is_whitespace()) {
                            let tok = unquote_yaml(token);
                            if !tok.is_empty() && !fm.allowed_tools.iter().any(|t| t == tok) {
                                fm.allowed_tools.push(tok.to_string());
                            }
                        }
                    } else {
                        parse_yaml_list_value(
                            v,
                            "allowed_tools",
                            &mut fm.allowed_tools,
                            &mut current_list_key,
                        );
                    }
                }
                _ => {
                    current_list_key = None;
                }
            }
        }
    }
}

/// Parses inline bracketed list `["a", "b", "c"]`.
fn parse_inline_list(val: &str) -> Vec<String> {
    let inner = val.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| unquote_yaml(s).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn quote_yaml_scalar(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s.replace('"', "\\\"")))
}

/// Formats a skill back into a canonical `SKILL.md` string with YAML frontmatter.
pub fn format_skill_markdown(fm: &SkillFrontmatter, instructions: &str) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("name: {}\n", quote_yaml_scalar(&fm.name)));
    out.push_str(&format!(
        "description: {}\n",
        quote_yaml_scalar(&fm.description)
    ));
    out.push_str(&format!("version: {}\n", quote_yaml_scalar(&fm.version)));
    if let Some(author) = &fm.author {
        out.push_str(&format!("author: {}\n", quote_yaml_scalar(author)));
    }
    if let Some(category) = &fm.category {
        out.push_str(&format!("category: {}\n", quote_yaml_scalar(category)));
    }
    if let Some(kind) = &fm.kind {
        out.push_str(&format!("kind: {}\n", quote_yaml_scalar(kind)));
    }
    if fm.always_apply {
        out.push_str("always_apply: true\n");
    }
    for (key, list) in [
        ("globs", &fm.globs),
        ("triggers", &fm.triggers),
        ("allowed_tools", &fm.allowed_tools),
    ] {
        if !list.is_empty() {
            let list_json = serde_json::to_string(list).unwrap_or_else(|_| "[]".to_string());
            out.push_str(&format!("{}: {}\n", key, list_json));
        }
    }
    out.push_str("---\n\n");
    out.push_str(instructions.trim());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_markdown() {
        let md = r#"---
name: "tailwind-v4"
description: "Modern Tailwind CSS v4 patterns"
version: "1.2.0"
globs: ["*.css", "*.tsx"]
triggers: ["tailwind", "styling", "css"]
always_apply: false
---

# Tailwind CSS v4 Guidelines
Always use `@import "tailwindcss";`.
"#;
        let (fm, instructions) = parse_skill_markdown(md, "fallback");
        assert_eq!(fm.name, "tailwind-v4");
        assert_eq!(fm.description, "Modern Tailwind CSS v4 patterns");
        assert_eq!(fm.version, "1.2.0");
        assert_eq!(fm.globs, vec!["*.css", "*.tsx"]);
        assert_eq!(fm.triggers, vec!["tailwind", "styling", "css"]);
        assert!(!fm.always_apply);
        assert!(instructions.contains("Always use `@import \"tailwindcss\";`."));
    }

    #[test]
    fn test_format_and_roundtrip() {
        let fm = SkillFrontmatter {
            name: "react-19".to_string(),
            description: "React 19 Server Actions and Hooks".to_string(),
            version: "1.0.0".to_string(),
            author: Some("agent".to_string()),
            category: Some("frontend".to_string()),
            kind: Some("reference".to_string()),
            globs: vec!["*.tsx".to_string()],
            triggers: vec!["react".to_string()],
            always_apply: true,
            allowed_tools: vec!["exec_cmd".to_string()],
        };
        let body = "Use useActionState and Server Actions.";
        let formatted = format_skill_markdown(&fm, body);
        let (parsed_fm, parsed_body) = parse_skill_markdown(&formatted, "fallback");

        assert_eq!(parsed_fm.name, "react-19");
        assert_eq!(parsed_fm.description, "React 19 Server Actions and Hooks");
        assert_eq!(parsed_fm.globs, vec!["*.tsx"]);
        assert_eq!(parsed_fm.triggers, vec!["react"]);
        assert!(parsed_fm.always_apply);
        assert_eq!(parsed_body.trim(), body);
    }

    #[test]
    fn test_sanitize_skill_name() {
        assert_eq!(
            sanitize_skill_name("React 19 / Next.js"),
            "react-19-next-js"
        );
        assert_eq!(sanitize_skill_name("  --Tailwind__v4-- "), "tailwind-v4");
        assert_eq!(sanitize_skill_name("graphql_schema!"), "graphql-schema");
    }

    #[test]
    fn test_category_and_bundle() {
        let md = r#"---
name: "fastapi-auth"
description: "FastAPI JWT authentication"
category: "backend"
kind: "reference"
triggers: ["auth", "fastapi"]
---
Use OAuth2PasswordBearer.
"#;
        let (fm, _) = parse_skill_markdown(md, "fallback");
        assert_eq!(fm.category, Some("backend".to_string()));
        assert_eq!(fm.category_name(), "backend");
        assert_eq!(fm.kind, Some("reference".to_string()));
        assert_eq!(fm.skill_kind(), SkillKind::Reference);
        assert_eq!(fm.skill_kind().badge(), "Doc");

        // Test workflow skill fallback based on category
        let wf_md = r#"---
name: "tdd-workflow"
description: "TDD Red Green Refactor"
category: "testing"
---
Write tests first.
"#;
        let (wf_fm, _) = parse_skill_markdown(wf_md, "fallback");
        assert_eq!(wf_fm.skill_kind(), SkillKind::Workflow);
        assert_eq!(wf_fm.skill_kind().badge(), "Skill");

        let bundle = VaultBundle {
            name: "fullstack-nextjs".to_string(),
            description: "Fullstack Next.js stack".to_string(),
            category: "fullstack".to_string(),
            skills: vec!["react".to_string(), "nextjs".to_string()],
            tags: vec!["web".to_string()],
            scope: SkillScope::Builtin,
            path: None,
        };
        assert!(bundle.matches_query("nextjs"));
        assert!(bundle.matches_query("fullstack"));
        assert!(!bundle.matches_query("kubernetes"));
    }

    #[test]
    fn test_sources_and_gotchas_models() {
        let source = LearnedSource {
            id: "src-1".to_string(),
            uri: "https://docs.rs/tokio".to_string(),
            title: "Tokio Async Runtime".to_string(),
            kind: SourceKind::WebDoc,
            summary: "Async channels and tasks in Rust".to_string(),
            tags: vec!["rust".to_string(), "async".to_string()],
            extracted_skill_or_doc: Some("rust-tokio".to_string()),
            created_at: "2026-09-29T12:00:00Z".to_string(),
            last_referenced: "2026-09-29T12:00:00Z".to_string(),
        };

        assert_eq!(source.kind.as_str(), "web_doc");
        assert_eq!(source.kind.badge(), "WebDoc");
        assert!(source.matches_query("tokio"));
        assert!(source.matches_query("rust"));
        assert!(!source.matches_query("python"));

        let gotcha = GlobalGotcha {
            id: "gotcha-1".to_string(),
            trigger: "broadcast channel capacity saturation".to_string(),
            context_scope: "rust".to_string(),
            failed_attempt: "Calling send() on unbounded or undersized broadcast channel"
                .to_string(),
            verified_fix: "Use bounded channel with Lagged error handling or mpsc".to_string(),
            created_at: "2026-09-29T12:00:00Z".to_string(),
            occurrence_count: 2,
        };

        assert!(gotcha.matches_query("broadcast"));
        assert!(gotcha.matches_query("rust"));
        assert!(!gotcha.matches_query("django"));
        let block = gotcha.format_prompt_block();
        assert!(block.contains("Avoid: Calling send()"));
        assert!(block.contains("Fix:   Use bounded channel"));
    }
}
