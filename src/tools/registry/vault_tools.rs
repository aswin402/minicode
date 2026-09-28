//! MiniVault multi-tier agent skills storage and lifecycle tools.
//!
//! Provides fast skill discovery, retrieval, project installation/loading,
//! authoring, updating, deletion, and HTTP URL importing for AI agents.

use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param::*;
use crate::vault::models::SkillScope;
use crate::vault::store::VaultStore;
use serde_json::json;
use std::path::Path;

/// Returns the 8 tool schemas for MiniVault.
pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "vault_search".to_string(),
            description: "Search available agent skills across Built-in, Global, and Project vaults by keyword, name, description, or triggers.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Keyword search query across skill names, descriptions, and trigger tags (leave empty to list all)"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["all", "project", "global", "builtin"],
                        "description": "Filter skills by vault scope (default: 'all')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "vault_show".to_string(),
            description: "Inspect the full instructions, guidelines, triggers, globs, and metadata of a specific skill.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name or identifier of the skill to inspect (e.g. 'tailwind-v4', 'react', 'rust-tokio')"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "vault_load".to_string(),
            description: "Load/install a skill from the Built-in or Global vault into the active project (.minicode/skills/<name>/SKILL.md) and enable it for the orchestrator.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the skill to load into the active project"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "vault_unload".to_string(),
            description: "Unload/remove a skill from the active project workspace (.minicode/skills/<name>).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the skill to unload from the project"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "vault_create".to_string(),
            description: "Forge and register a brand-new skill with YAML frontmatter in either the project (.minicode/skills/) or global vault.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Kebab-case name of the new skill"
                    },
                    "description": {
                        "type": "string",
                        "description": "One-line summary of what the skill provides"
                    },
                    "instructions": {
                        "type": "string",
                        "description": "Detailed markdown guidelines, best practices, rules, and invariants"
                    },
                    "triggers": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Keywords or intent phrases that trigger this skill (e.g. ['tailwind', 'css'])"
                    },
                    "globs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "File match globs (e.g. ['*.css', '*.tsx'])"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["project", "global"],
                        "description": "Destination scope: 'project' (default) or 'global'"
                    }
                },
                "required": ["name", "description", "instructions"]
            }),
        },
        ToolSchema {
            name: "vault_update".to_string(),
            description: "Update or replace the markdown instructions of an existing skill in the vault.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the skill to update"
                    },
                    "instructions": {
                        "type": "string",
                        "description": "New markdown body/guidelines for the skill"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["project", "global"],
                        "description": "Target scope (default: 'project')"
                    }
                },
                "required": ["name", "instructions"]
            }),
        },
        ToolSchema {
            name: "vault_delete".to_string(),
            description: "Permanently delete a custom skill from either the project or global vault.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the skill to delete"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["project", "global"],
                        "description": "Scope to delete from: 'project' or 'global'"
                    }
                },
                "required": ["name", "scope"]
            }),
        },
        ToolSchema {
            name: "vault_import_url".to_string(),
            description: "Download and import a skill from an HTTP URL (GitHub raw, Gist, or AgentSkills repository) into the vault.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "HTTP/HTTPS URL pointing to raw markdown / SKILL.md file"
                    },
                    "name": {
                        "type": "string",
                        "description": "Optional custom name override for the imported skill"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["project", "global"],
                        "description": "Destination scope: 'project' (default) or 'global'"
                    }
                },
                "required": ["url"]
            }),
        },
    ]
}

/// Dispatches a tool invocation to the corresponding MiniVault handler.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    let store = VaultStore::new(workspace_root);

    match tool_name {
        "vault_search" => Some(async move {
            let query = opt_query(args).unwrap_or("");
            let scope_str = opt_str(args, "scope").unwrap_or("all");

            let scope_filter = match scope_str {
                "project" => Some(SkillScope::Project),
                "global" => Some(SkillScope::Global),
                "builtin" => Some(SkillScope::Builtin),
                _ => None,
            };

            let mut results = if query.is_empty() {
                store.list_by_scope(scope_filter)
            } else {
                let mut searched = store.search_skills(query);
                if let Some(s) = scope_filter {
                    searched.retain(|k| k.scope == s);
                }
                searched
            };

            if results.is_empty() {
                return Ok(format!(
                    "ℹ No skills found matching query '{}' in scope '{}'.",
                    query, scope_str
                ));
            }

            let mut out = format!(
                "📦 **MiniVault Skills** ({} found matching '{}' [scope: {}]):\n\n",
                results.len(),
                query,
                scope_str
            );
            out.push_str("| Scope | Status | Skill Name | Description | Triggers |\n");
            out.push_str("| :--- | :--- | :--- | :--- | :--- |\n");

            for s in results.drain(..) {
                let scope_badge = match s.scope {
                    SkillScope::Project => "Project",
                    SkillScope::Global => "Global",
                    SkillScope::Builtin => "Built-in",
                };
                let status_badge = if s.is_active_in_project {
                    "✅ Active"
                } else {
                    "⚪ Available"
                };
                let triggers = if s.frontmatter.triggers.is_empty() {
                    "-".to_string()
                } else {
                    s.frontmatter.triggers.join(", ")
                };
                out.push_str(&format!(
                    "| `{}` | {} | **`{}`** | {} | `{}` |\n",
                    scope_badge, status_badge, s.name, s.description, triggers
                ));
            }

            out.push_str("\n💡 Use `vault_show(name)` to inspect full rules, or `vault_load(name)` to install into this project.\n");
            Ok(out)
        }.await),

        "vault_show" => Some(async move {
            let name = match require_str(args, "name", "vault_show") {
                Ok(n) => n,
                Err(e) => return Err(e.into()),
            };

            let skill = match store.get_skill(name) {
                Ok(s) => s,
                Err(e) => return Err(ToolError::ExecutionFailed(e.to_string()).into()),
            };

            let scope_str = match skill.scope {
                SkillScope::Project => "Project Workspace Local",
                SkillScope::Global => "Global User Config",
                SkillScope::Builtin => "Built-in Curated Core",
            };

            let status_str = if skill.is_active_in_project {
                "✅ Active in Project"
            } else {
                "⚪ Available in Vault (Not Active)"
            };

            let mut out = format!(
                "📖 **Skill `{}`** [{} | {}]\n",
                skill.name, scope_str, status_str
            );
            out.push_str(&format!("*{}*\n\n", skill.description));

            if !skill.frontmatter.globs.is_empty() {
                out.push_str(&format!("• **Globs**: `{}`\n", skill.frontmatter.globs.join(", ")));
            }
            if !skill.frontmatter.triggers.is_empty() {
                out.push_str(&format!("• **Triggers**: `{}`\n", skill.frontmatter.triggers.join(", ")));
            }
            if let Some(path) = &skill.path {
                out.push_str(&format!("• **File Path**: `{}`\n", path.display()));
            }

            out.push_str("\n---\n\n");
            out.push_str(&skill.instructions);
            out.push('\n');

            Ok(out)
        }.await),

        "vault_load" => Some(async move {
            let name = match require_str(args, "name", "vault_load") {
                Ok(n) => n,
                Err(e) => return Err(e.into()),
            };

            match store.load_to_project(name) {
                Ok(skill) => {
                    let dest = skill.path.map(|p| p.display().to_string()).unwrap_or_else(|| ".minicode/skills/".to_string());
                    Ok(format!(
                        "✔ Successfully loaded skill `{}` into active project at `{}`.\n\
                         Orchestrator context has activated this skill for subsequent turns.",
                        skill.name, dest
                    ))
                }
                Err(e) => Err(ToolError::ExecutionFailed(format!("Failed to load skill: {}", e)).into()),
            }
        }.await),

        "vault_unload" => Some(async move {
            let name = match require_str(args, "name", "vault_unload") {
                Ok(n) => n,
                Err(e) => return Err(e.into()),
            };

            match store.unload_from_project(name) {
                Ok(_) => Ok(format!(
                    "✔ Successfully unloaded skill `{}` from active project workspace.",
                    name
                )),
                Err(e) => Err(ToolError::ExecutionFailed(format!("Failed to unload skill: {}", e)).into()),
            }
        }.await),

        "vault_create" => Some(async move {
            let name = match require_str(args, "name", "vault_create") {
                Ok(n) => n,
                Err(e) => return Err(e.into()),
            };
            let description = match require_str(args, "description", "vault_create") {
                Ok(d) => d,
                Err(e) => return Err(e.into()),
            };
            let instructions = match require_str(args, "instructions", "vault_create") {
                Ok(i) => i,
                Err(e) => return Err(e.into()),
            };

            let triggers = opt_string_array(args, "triggers").unwrap_or_default();
            let globs = opt_string_array(args, "globs").unwrap_or_default();
            let scope_str = opt_str(args, "scope").unwrap_or("project");

            let target_scope = if scope_str.eq_ignore_ascii_case("global") {
                SkillScope::Global
            } else {
                SkillScope::Project
            };

            match store.create_skill(target_scope, name, description, instructions, triggers, globs) {
                Ok(skill) => {
                    let path_str = skill.path.map(|p| p.display().to_string()).unwrap_or_default();
                    Ok(format!(
                        "✔ Successfully forged new skill `{}` [{:?}] at `{}`.",
                        skill.name, skill.scope, path_str
                    ))
                }
                Err(e) => Err(ToolError::ExecutionFailed(format!("Failed to create skill: {}", e)).into()),
            }
        }.await),

        "vault_update" => Some(async move {
            let name = match require_str(args, "name", "vault_update") {
                Ok(n) => n,
                Err(e) => return Err(e.into()),
            };
            let instructions = match require_str(args, "instructions", "vault_update") {
                Ok(i) => i,
                Err(e) => return Err(e.into()),
            };
            let scope_str = opt_str(args, "scope").unwrap_or("project");
            let target_scope = if scope_str.eq_ignore_ascii_case("global") {
                SkillScope::Global
            } else {
                SkillScope::Project
            };

            match store.update_skill(target_scope, name, instructions) {
                Ok(skill) => Ok(format!(
                    "✔ Successfully updated skill `{}` in {:?} scope.",
                    skill.name, skill.scope
                )),
                Err(e) => Err(ToolError::ExecutionFailed(format!("Failed to update skill: {}", e)).into()),
            }
        }.await),

        "vault_delete" => Some(async move {
            let name = match require_str(args, "name", "vault_delete") {
                Ok(n) => n,
                Err(e) => return Err(e.into()),
            };
            let scope_str = match require_str(args, "scope", "vault_delete") {
                Ok(s) => s,
                Err(e) => return Err(e.into()),
            };

            let target_scope = match scope_str {
                "project" => SkillScope::Project,
                "global" => SkillScope::Global,
                _ => {
                    return Err(ToolError::InvalidArguments {
                        name: "vault_delete".to_string(),
                        reason: "Scope must be either 'project' or 'global'".to_string(),
                    }
                    .into());
                }
            };

            match store.delete_skill(target_scope, name) {
                Ok(_) => Ok(format!(
                    "✔ Successfully deleted skill `{}` from {:?} vault.",
                    name, target_scope
                )),
                Err(e) => Err(ToolError::ExecutionFailed(format!("Failed to delete skill: {}", e)).into()),
            }
        }.await),

        "vault_import_url" => Some(async move {
            let url = match require_str(args, "url", "vault_import_url") {
                Ok(u) => u,
                Err(e) => return Err(e.into()),
            };
            let name_override = opt_str(args, "name");
            let scope_str = opt_str(args, "scope").unwrap_or("project");

            let target_scope = if scope_str.eq_ignore_ascii_case("global") {
                SkillScope::Global
            } else {
                SkillScope::Project
            };

            match store.import_from_url(url, target_scope, name_override).await {
                Ok(skill) => {
                    let path_str = skill.path.map(|p| p.display().to_string()).unwrap_or_default();
                    Ok(format!(
                        "✔ Successfully imported skill `{}` from `{}` into {:?} vault at `{}`.",
                        skill.name, url, skill.scope, path_str
                    ))
                }
                Err(e) => Err(ToolError::ExecutionFailed(format!("Failed to import skill from URL: {}", e)).into()),
            }
        }.await),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_vault_tools_search_and_crud() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        // 1. Test search
        let res = dispatch("vault_search", &json!({ "query": "tailwind" }), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(res.contains("tailwind-v4"));

        // 2. Test show
        let show_res = dispatch("vault_show", &json!({ "name": "tailwind-v4" }), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(show_res.contains("Tailwind CSS v4 Guidelines"));

        // 3. Test load to project
        let load_res = dispatch("vault_load", &json!({ "name": "tailwind-v4" }), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(load_res.contains("Successfully loaded skill `tailwind-v4`"));

        // 4. Test create custom skill
        let create_res = dispatch(
            "vault_create",
            &json!({
                "name": "micro-services",
                "description": "Microservice resilience patterns",
                "instructions": "Use circuit breakers and retry policies.",
                "triggers": ["microservice", "resilience"],
                "scope": "project"
            }),
            ws,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(create_res.contains("micro-services"));

        // 5. Test unload from project
        let unload_res = dispatch("vault_unload", &json!({ "name": "micro-services" }), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(unload_res.contains("Successfully unloaded"));
    }
}
