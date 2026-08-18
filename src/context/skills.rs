use crate::constants::{AGENTS_MD_FILE, SKILLS_DIR_NAME, SKILL_MD_FILE};
use crate::error::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub content: String,
    pub instructions: String,
}

#[allow(dead_code)]
pub struct SkillLoader;

/// Top-level convenience wrapper for skill discovery.
pub fn discover_skills(workspace_root: &Path) -> Vec<Skill> {
    SkillLoader::discover_skills(workspace_root).unwrap_or_default()
}

/// Parses raw markdown with optional YAML frontmatter into a structured `Skill`.
pub fn parse_skill_markdown(content: &str, path: &Path) -> Result<Skill> {
    let (frontmatter_name, desc, instructions) = if let Some(stripped) = content.strip_prefix("---")
    {
        if let Some(end_idx) = stripped.find("---") {
            let frontmatter = &stripped[..end_idx];
            let body = stripped[end_idx + 3..].trim();
            let name = extract_frontmatter_field(frontmatter, "name");
            let desc = extract_frontmatter_field(frontmatter, "description")
                .unwrap_or_else(|| "Custom workspace skill".to_string());
            (name, desc, body.to_string())
        } else {
            (
                None,
                "Custom workspace skill".to_string(),
                content.to_string(),
            )
        }
    } else {
        (
            None,
            "Custom workspace skill".to_string(),
            content.to_string(),
        )
    };

    let name = frontmatter_name.unwrap_or_else(|| {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed-skill".to_string());
        if stem.eq_ignore_ascii_case("skill") {
            path.parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or(stem)
        } else {
            stem
        }
    });

    Ok(Skill {
        name,
        description: desc,
        path: path.to_path_buf(),
        content: content.to_string(),
        instructions,
    })
}

/// Extracts a string field value from a YAML frontmatter block.
pub fn extract_frontmatter_field(frontmatter: &str, field: &str) -> Option<String> {
    let prefix = format!("{}:", field);
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&prefix) {
            let val = trimmed.trim_start_matches(&prefix).trim();
            let clean = val.trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}

/// Extracts the `description:` field value from a YAML frontmatter block.
pub fn extract_frontmatter_description(frontmatter: &str) -> Option<String> {
    extract_frontmatter_field(frontmatter, "description")
}

impl SkillLoader {
    /// Discovers all available guidelines, agent rules, and skill definitions in the workspace.
    #[allow(dead_code)]
    pub fn discover_skills(workspace_root: &Path) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();

        // 1. Check AGENTS.md in workspace root
        let agents_md = workspace_root.join(AGENTS_MD_FILE);
        match std::fs::read_to_string(&agents_md) {
            Ok(content) => {
                skills.push(Skill {
                    name: "agents-rules".to_string(),
                    description: "Repository rules, architecture instructions, and conventions"
                        .to_string(),
                    path: agents_md,
                    instructions: content.clone(),
                    content,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %agents_md.display(), error = %e, "Failed to read AGENTS.md");
            }
        }

        // 2. Check SKILL.md in workspace root
        let skill_md = workspace_root.join(SKILL_MD_FILE);
        match std::fs::read_to_string(&skill_md) {
            Ok(content) => {
                skills.push(Skill {
                    name: "root-skill".to_string(),
                    description: "Root skill instructions for current workspace".to_string(),
                    path: skill_md,
                    instructions: content.clone(),
                    content,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %skill_md.display(), error = %e, "Failed to read SKILL.md");
            }
        }

        // 3. Scan .skills/ directory
        let skills_dir = workspace_root.join(SKILLS_DIR_NAME);
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let name = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let desc = extract_frontmatter_description(&content)
                            .unwrap_or_else(|| "Workspace skill definition".to_string());
                        skills.push(Skill {
                            name,
                            description: desc,
                            path,
                            instructions: content.clone(),
                            content,
                        });
                    }
                }
            }
        }

        // 4. Scan .minicode/skills/ directory
        let minicode_skills_dir = workspace_root.join(".minicode").join("skills");
        if let Ok(entries) = std::fs::read_dir(&minicode_skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let skill_md = path.join("SKILL.md");
                    if skill_md.exists() {
                        if let Ok(content) = std::fs::read_to_string(&skill_md) {
                            let name = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            let desc = extract_frontmatter_description(&content)
                                .unwrap_or_else(|| "Dynamic repository skill".to_string());
                            skills.push(Skill {
                                name,
                                description: desc,
                                path: skill_md,
                                instructions: content.clone(),
                                content,
                            });
                        }
                    }
                } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let name = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let desc = extract_frontmatter_description(&content)
                            .unwrap_or_else(|| "Dynamic repository skill".to_string());
                        skills.push(Skill {
                            name,
                            description: desc,
                            path,
                            instructions: content.clone(),
                            content,
                        });
                    }
                }
            }
        }

        Ok(skills)
    }

    /// Finds a specific skill by name.
    #[allow(dead_code)]
    pub fn get_skill_by_name(workspace_root: &Path, name: &str) -> Option<Skill> {
        let skills = match Self::discover_skills(workspace_root) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to discover skills");
                return None;
            }
        };
        skills
            .into_iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_discovery() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_skills_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let agents_file = temp_dir.join(crate::constants::AGENTS_MD_FILE);
        std::fs::write(&agents_file, "# Rules\n1. Always be fast.").unwrap();

        let skills = SkillLoader::discover_skills(&temp_dir).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "agents-rules");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
