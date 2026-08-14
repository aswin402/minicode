#![allow(dead_code)]

use crate::error::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub content: String,
}

pub struct SkillLoader;

impl SkillLoader {
    /// Discovers all available guidelines, agent rules, and skill definitions in the workspace.
    pub fn discover_skills(workspace_root: &Path) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();

        // 1. Check AGENTS.md in workspace root
        let agents_md = workspace_root.join("AGENTS.md");
        if agents_md.exists() {
            if let Ok(content) = std::fs::read_to_string(&agents_md) {
                skills.push(Skill {
                    name: "agents-rules".to_string(),
                    description: "Repository rules, architecture instructions, and conventions"
                        .to_string(),
                    path: agents_md,
                    content,
                });
            }
        }

        // 2. Check SKILL.md in workspace root
        let skill_md = workspace_root.join("SKILL.md");
        if skill_md.exists() {
            if let Ok(content) = std::fs::read_to_string(&skill_md) {
                skills.push(Skill {
                    name: "root-skill".to_string(),
                    description: "Root skill instructions for current workspace".to_string(),
                    path: skill_md,
                    content,
                });
            }
        }

        // 3. Scan .skills/ directory
        let skills_dir = workspace_root.join(".skills");
        if skills_dir.exists() && skills_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(skills_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let name = path
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            skills.push(Skill {
                                name,
                                description: "Workspace skill definition".to_string(),
                                path,
                                content,
                            });
                        }
                    }
                }
            }
        }

        Ok(skills)
    }

    /// Finds a specific skill by name.
    pub fn get_skill_by_name(workspace_root: &Path, name: &str) -> Option<Skill> {
        let skills = Self::discover_skills(workspace_root).ok()?;
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

        let agents_file = temp_dir.join("AGENTS.md");
        std::fs::write(&agents_file, "# Rules\n1. Always be fast.").unwrap();

        let skills = SkillLoader::discover_skills(&temp_dir).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "agents-rules");

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
