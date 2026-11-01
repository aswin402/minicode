use crate::context::skills::Skill;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Frontmatter metadata for dynamic skills created by the agent or developer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(dead_code)]
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

    /// Creates and validates a new skill package in `.minicode/skills/<skill_name>/SKILL.md`
    /// via the unified [`crate::vault::store::VaultStore`].
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

        let store = crate::vault::store::VaultStore::new(workspace_root);
        let body = format!("# {}\n\n{}", clean_name, instructions.trim());
        store
            .create_skill_with_tools(
                crate::vault::models::SkillScope::Project,
                &clean_name,
                description.trim(),
                Some("workflow"),
                Some("workflow"),
                &body,
                Vec::new(),
                Vec::new(),
                allowed_tools.to_vec(),
            )
            .map_err(|e| {
                ToolError::ExecutionFailed(format!(
                    "create_skill failed for '{}': {}",
                    clean_name, e
                ))
            })?;

        Ok(format!(
            "✔ Successfully forged new skill `{}` at `.minicode/skills/{}/SKILL.md`",
            clean_name, clean_name
        ))
    }

    /// Reads and parses an individual skill across Project, Global, and Built-in MiniVault tiers.
    pub fn inspect_skill(workspace_root: &Path, name: &str) -> Result<Skill> {
        let clean_name = sanitize_skill_name(name);
        let store = crate::vault::store::VaultStore::new(workspace_root);
        if let Ok(vault_skill) = store
            .get_skill(&clean_name)
            .or_else(|_| store.get_skill(name.trim()))
        {
            let path = vault_skill.path.unwrap_or_else(|| {
                Self::skills_dir(workspace_root)
                    .join(&vault_skill.name)
                    .join("SKILL.md")
            });
            return Ok(Skill {
                name: vault_skill.name,
                description: vault_skill.description,
                path,
                content: vault_skill.raw_content,
                instructions: vault_skill.instructions,
            });
        }

        if let Some(local_skill) =
            crate::context::skills::SkillLoader::get_skill_by_name(workspace_root, name.trim())
        {
            return Ok(local_skill);
        }

        Err(ToolError::NotFound {
            name: format!("skill:{}", name),
        }
        .into())
    }

    /// Lists all skills discovered in `.minicode/skills/` and the MiniVault catalog.
    pub fn list_all_skills(workspace_root: &Path) -> Result<String> {
        let local_skills = crate::context::skills::discover_skills(workspace_root);
        let store = crate::vault::store::VaultStore::new(workspace_root);
        let vault_skills = store.list_all_skills();

        if local_skills.is_empty() && vault_skills.is_empty() {
            return Ok(
                "ℹ No skills found in `.minicode/skills/` or skill directories.".to_string(),
            );
        }

        let mut out = String::new();
        if !local_skills.is_empty() {
            out.push_str(&format!(
                "🛠️ Workspace Discovered Skills ({} active):\n\n",
                local_skills.len()
            ));
            for (i, skill) in local_skills.iter().enumerate() {
                out.push_str(&format!(
                    "{}. **{}** (`{}`)\n   _{}_\n   📁 `{}`\n\n",
                    i + 1,
                    skill.name,
                    skill.name,
                    skill.description,
                    skill.path.display()
                ));
            }
        }

        let active_vault: Vec<_> = vault_skills
            .iter()
            .filter(|s| s.is_active_in_project)
            .collect();
        let catalog_vault: Vec<_> = vault_skills
            .iter()
            .filter(|s| !s.is_active_in_project)
            .collect();

        if !active_vault.is_empty() {
            out.push_str(&format!(
                "🧠 Active MiniVault Skills ({} loaded in project):\n",
                active_vault.len()
            ));
            for s in active_vault {
                out.push_str(&format!(
                    "- **{}** (`{}`): {}\n",
                    s.name,
                    s.scope.as_str(),
                    s.description
                ));
            }
            out.push('\n');
        }

        if !catalog_vault.is_empty() {
            out.push_str(&format!(
                "📦 Available MiniVault Library ({} ready via `inspect_skill` / `vault_show` / `vault_load`):\n",
                catalog_vault.len()
            ));
            for s in catalog_vault {
                out.push_str(&format!("- `{}`: {}\n", s.name, s.description));
            }
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

        // Inspecting built-in skills via SkillForge resolves cleanly through VaultStore
        let builtin_react = SkillForge::inspect_skill(ws, "react").unwrap();
        assert_eq!(builtin_react.name, "react");
        let builtin_caveman = SkillForge::inspect_skill(ws, "caveman").unwrap();
        assert_eq!(builtin_caveman.name, "caveman");

        let list_str = SkillForge::list_all_skills(ws).unwrap();
        assert!(list_str.contains("rust-testing"));
        assert!(list_str.contains("MiniVault"));
    }
}
