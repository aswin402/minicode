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
    pub globs: Vec<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub always_apply: bool,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
}

impl Default for SkillFrontmatter {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            version: default_version(),
            author: None,
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
        if prompt_lower.contains(&name_lower) {
            return true;
        }

        for trigger in &self.frontmatter.triggers {
            let trig_lower = trigger.to_lowercase();
            if !trig_lower.is_empty() && prompt_lower.contains(&trig_lower) {
                return true;
            }
        }

        false
    }
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
        if let Some(end_idx) = stripped.find("---") {
            let frontmatter_str = &stripped[..end_idx];
            instructions = stripped[end_idx + 3..].trim().to_string();

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

/// Parses standard YAML frontmatter fields into `SkillFrontmatter`.
fn parse_yaml_frontmatter_fields(frontmatter: &str, fm: &mut SkillFrontmatter) {
    let mut current_list_key: Option<String> = None;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // List item continuation (- item)
        if trimmed.starts_with('-') && current_list_key.is_some() {
            let item = trimmed.trim_start_matches('-').trim();
            let clean_item = item.trim_matches('"').trim_matches('\'').to_string();
            if !clean_item.is_empty() {
                match current_list_key.as_deref() {
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

            match k.as_str() {
                "name" => {
                    current_list_key = None;
                    let clean = v.trim_matches('"').trim_matches('\'').trim();
                    if !clean.is_empty() {
                        fm.name = clean.to_string();
                    }
                }
                "description" => {
                    current_list_key = None;
                    let clean = v.trim_matches('"').trim_matches('\'').trim();
                    if !clean.is_empty() {
                        fm.description = clean.to_string();
                    }
                }
                "version" => {
                    current_list_key = None;
                    let clean = v.trim_matches('"').trim_matches('\'').trim();
                    if !clean.is_empty() {
                        fm.version = clean.to_string();
                    }
                }
                "author" => {
                    current_list_key = None;
                    let clean = v.trim_matches('"').trim_matches('\'').trim();
                    if !clean.is_empty() {
                        fm.author = Some(clean.to_string());
                    }
                }
                "always_apply" => {
                    current_list_key = None;
                    fm.always_apply = v.eq_ignore_ascii_case("true");
                }
                "globs" => {
                    if v.starts_with('[') && v.ends_with(']') {
                        current_list_key = None;
                        fm.globs = parse_inline_list(v);
                    } else if v.is_empty() {
                        current_list_key = Some("globs".to_string());
                    } else {
                        current_list_key = None;
                        let clean = v.trim_matches('"').trim_matches('\'').trim();
                        if !clean.is_empty() {
                            fm.globs.push(clean.to_string());
                        }
                    }
                }
                "triggers" => {
                    if v.starts_with('[') && v.ends_with(']') {
                        current_list_key = None;
                        fm.triggers = parse_inline_list(v);
                    } else if v.is_empty() {
                        current_list_key = Some("triggers".to_string());
                    } else {
                        current_list_key = None;
                        let clean = v.trim_matches('"').trim_matches('\'').trim();
                        if !clean.is_empty() {
                            fm.triggers.push(clean.to_string());
                        }
                    }
                }
                "allowed_tools" => {
                    if v.starts_with('[') && v.ends_with(']') {
                        current_list_key = None;
                        fm.allowed_tools = parse_inline_list(v);
                    } else if v.is_empty() {
                        current_list_key = Some("allowed_tools".to_string());
                    } else {
                        current_list_key = None;
                        let clean = v.trim_matches('"').trim_matches('\'').trim();
                        if !clean.is_empty() {
                            fm.allowed_tools.push(clean.to_string());
                        }
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
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Formats a skill back into a canonical `SKILL.md` string with YAML frontmatter.
pub fn format_skill_markdown(fm: &SkillFrontmatter, instructions: &str) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("name: \"{}\"\n", fm.name));
    out.push_str(&format!("description: \"{}\"\n", fm.description));
    out.push_str(&format!("version: \"{}\"\n", fm.version));
    if let Some(author) = &fm.author {
        out.push_str(&format!("author: \"{}\"\n", author));
    }
    if fm.always_apply {
        out.push_str("always_apply: true\n");
    }
    if !fm.globs.is_empty() {
        let globs_json = serde_json::to_string(&fm.globs).unwrap_or_else(|_| "[]".to_string());
        out.push_str(&format!("globs: {}\n", globs_json));
    }
    if !fm.triggers.is_empty() {
        let triggers_json =
            serde_json::to_string(&fm.triggers).unwrap_or_else(|_| "[]".to_string());
        out.push_str(&format!("triggers: {}\n", triggers_json));
    }
    if !fm.allowed_tools.is_empty() {
        let tools_json =
            serde_json::to_string(&fm.allowed_tools).unwrap_or_else(|_| "[]".to_string());
        out.push_str(&format!("allowed_tools: {}\n", tools_json));
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
}
