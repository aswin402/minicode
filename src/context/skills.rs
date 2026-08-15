#![allow(dead_code)]

use crate::constants::{AGENTS_MD_FILE, SKILLS_DIR_NAME, SKILL_MD_FILE};
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
        let agents_md = workspace_root.join(AGENTS_MD_FILE);
        match std::fs::read_to_string(&agents_md) {
            Ok(content) => {
                skills.push(Skill {
                    name: "agents-rules".to_string(),
                    description: "Repository rules, architecture instructions, and conventions"
                        .to_string(),
                    path: agents_md,
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
        match std::fs::read_dir(&skills_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        match std::fs::read_to_string(&path) {
                            Ok(content) => {
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
                            Err(e) => {
                                tracing::warn!(path = %path.display(), error = %e, "Failed to read skill file");
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %skills_dir.display(), error = %e, "Failed to read .skills directory");
            }
        }

        Ok(skills)
    }

    /// Finds a specific skill by name.
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
