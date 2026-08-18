use crate::context::skills::Skill;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Frontmatter metadata for dynamic skills created by the agent or developer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

pub struct SkillForge;

impl SkillForge {
    /// Returns the target directory for workspace skills `.minicode/skills`.
    pub fn skills_dir(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".minicode").join("skills")
    }

    /// Creates and validates a new skill package in `.minicode/skills/<skill_name>/SKILL.md`.
    pub fn create_skill(
        workspace_root: &Path,
        name: &str,
        description: &str,
        instructions: &str,
        allowed_tools: &[String],
    ) -> Result<String> {
        let clean_name = sanitize_skill_name(name);
        if clean_name.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "create_skill".to_string(),
                reason: "Skill name cannot be empty".to_string(),
            }
            .into());
        }

        if description.trim().is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "create_skill".to_string(),
                reason: "Skill description cannot be empty".to_string(),
            }
            .into());
        }

        let skill_dir = Self::skills_dir(workspace_root).join(&clean_name);
        fs::create_dir_all(&skill_dir).map_err(|e| ToolError::FileOp {
            path: skill_dir.display().to_string(),
            source: e,
        })?;

        let meta = SkillMetadata {
            name: clean_name.clone(),
            description: description.trim().to_string(),
            version: "1.0.0".to_string(),
            author: Some("minicode-agent".to_string()),
            allowed_tools: allowed_tools.to_vec(),
        };

        let frontmatter = format_frontmatter(&meta);
        let content = format!(
            "---\n{}\n---\n\n# {}\n\n{}\n",
            frontmatter,
            clean_name,
            instructions.trim()
        );

        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, &content).map_err(|e| ToolError::FileOp {
            path: skill_file.display().to_string(),
            source: e,
        })?;

        Ok(format!(
            "✔ Successfully forged new skill `{}` at `.minicode/skills/{}/SKILL.md`",
            clean_name, clean_name
        ))
    }

    /// Reads and parses an individual skill from `.minicode/skills/<skill_name>/SKILL.md`.
    pub fn inspect_skill(workspace_root: &Path, name: &str) -> Result<Skill> {
        let clean_name = sanitize_skill_name(name);
        let skill_file = Self::skills_dir(workspace_root)
            .join(&clean_name)
            .join("SKILL.md");

        if !skill_file.exists() {
            return Err(ToolError::NotFound {
                name: format!("skill:{}", name),
            }
            .into());
        }

        let raw = fs::read_to_string(&skill_file).map_err(|e| ToolError::FileOp {
            path: skill_file.display().to_string(),
            source: e,
        })?;

        let skill = crate::context::skills::parse_skill_markdown(&raw, &skill_file)?;
        Ok(skill)
    }

    /// Lists all skills discovered in `.minicode/skills/` and workspace skill paths.
    pub fn list_all_skills(workspace_root: &Path) -> Result<String> {
        let skills = crate::context::skills::discover_skills(workspace_root);
        if skills.is_empty() {
            return Ok(
                "ℹ No skills found in `.minicode/skills/` or skill directories.".to_string(),
            );
        }

        let mut out = format!("🛠️ Discovered Skills ({} total):\n\n", skills.len());
        for (i, skill) in skills.iter().enumerate() {
            out.push_str(&format!(
                "{}. **{}** (`{}`)\n   _{}_\n   📁 `{}`\n\n",
                i + 1,
                skill.name,
                skill.name,
                skill.description,
                skill.path.display()
            ));
        }

        Ok(out)
    }
}

fn sanitize_skill_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn format_frontmatter(meta: &SkillMetadata) -> String {
    let mut out = format!("name: \"{}\"\n", meta.name);
    out.push_str(&format!("description: \"{}\"\n", meta.description));
    out.push_str(&format!("version: \"{}\"\n", meta.version));
    if let Some(author) = &meta.author {
        out.push_str(&format!("author: \"{}\"\n", author));
    }
    if !meta.allowed_tools.is_empty() {
        out.push_str("allowed_tools:\n");
        for tool in &meta.allowed_tools {
            out.push_str(&format!("  - \"{}\"\n", tool));
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_skill_forge_create_and_inspect() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        let res = SkillForge::create_skill(
            ws,
            "rust-testing",
            "Expert guidelines for running cargo test with bounded parallelism",
            "Always run `cargo test -j 2 -- --test-threads=2`.",
            &["exec_cmd".to_string()],
        )
        .unwrap();

        assert!(res.contains("rust-testing"));

        let skill = SkillForge::inspect_skill(ws, "rust-testing").unwrap();
        assert_eq!(skill.name, "rust-testing");
        assert!(skill.description.contains("bounded parallelism"));
        assert!(skill.instructions.contains("cargo test -j 2"));

        let list_str = SkillForge::list_all_skills(ws).unwrap();
        assert!(list_str.contains("rust-testing"));
    }
}
