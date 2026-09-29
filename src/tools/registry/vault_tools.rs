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
            description: "Search available agent skills across Built-in, Global, and Project vaults by keyword, name, description, triggers, or kind (reference docs vs workflow skills).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Keyword search query across skill names, descriptions, and trigger tags (leave empty to list all)"
                    },
                    "category": {
                        "type": "string",
                        "description": "Filter skills by category (e.g. 'frontend', 'backend', 'database', 'devops', 'testing', 'security', 'ui-styling')"
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["all", "reference", "workflow", "doc", "skill"],
                        "description": "Filter by skill kind: 'reference' / 'doc' (technical API reference cheatsheets) or 'workflow' / 'skill' (behavioral engineering methodologies & quality gates)"
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
                    "category": {
                        "type": "string",
                        "description": "Category for the skill (e.g. 'frontend', 'backend', 'database', 'devops', 'testing', 'security')"
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["reference", "workflow"],
                        "description": "Kind of skill: 'reference' (technical API cheatsheet / invariants) or 'workflow' (operational engineering methodology / quality gate)"
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
        ToolSchema {
            name: "vault_bundle_list".to_string(),
            description: "List available skill bundles/packs across Built-in, Global, and Project vaults, including their categories, descriptions, and constituent skills.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Optional search filter across bundle names, descriptions, or tags"
                    },
                    "category": {
                        "type": "string",
                        "description": "Optional category filter (e.g. 'fullstack', 'backend', 'frontend', 'quality')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "vault_bundle_load".to_string(),
            description: "Load an entire bundle/pack of skills into the active project (.minicode/skills/) in a single coordinated operation.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the bundle to load (e.g. 'fullstack-nextjs', 'rust-systems', 'frontend-delight')"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "vault_bundle_create".to_string(),
            description: "Create and register a new reusable skill bundle containing a collection of skills in either project or global scope.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Kebab-case name for the new bundle"
                    },
                    "description": {
                        "type": "string",
                        "description": "Description of what this bundle provides"
                    },
                    "skills": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of skill names to include in this bundle"
                    },
                    "category": {
                        "type": "string",
                        "description": "Bundle category (default: 'general')"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tags or keywords for discovery"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["project", "global"],
                        "description": "Destination scope: 'project' (default) or 'global'"
                    }
                },
                "required": ["name", "description", "skills"]
            }),
        },
        ToolSchema {
            name: "vault_ingest_source".to_string(),
            description: "Ingest and bookmark an external knowledge source (HTTP URL or local directory/repo path) into minivault, registering it in sources.json and synthesizing corresponding documentation or workflow skills.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "uri": {
                        "type": "string",
                        "description": "HTTP URL (e.g. 'https://docs.rs/tokio') or local repository directory path to ingest"
                    },
                    "title": {
                        "type": "string",
                        "description": "Optional human-readable title or skill name override"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["project", "global"],
                        "description": "Destination vault scope: 'project' (default) or 'global'"
                    },
                    "instructions": {
                        "type": "string",
                        "description": "Optional custom markdown instructions or extraction summary"
                    }
                },
                "required": ["uri"]
            }),
        },
        ToolSchema {
            name: "vault_gotchas_list".to_string(),
            description: "Search and inspect universal cross-project technical gotchas, compiler traps, and verified fixes learned from past sessions across the machine.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Optional filter keyword or technology name (e.g. 'tokio', 'react', 'borrow checker')"
                    }
                }
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
            let category = opt_str(args, "category");
            let kind_str = opt_str(args, "kind");

            let scope_filter = match scope_str {
                "project" => Some(SkillScope::Project),
                "global" => Some(SkillScope::Global),
                "builtin" => Some(SkillScope::Builtin),
                _ => None,
            };

            let kind_filter: Option<crate::vault::models::SkillKind> = match kind_str {
                Some("all") | None => None,
                Some(k) => k.parse().ok(),
            };

            let mut results = store.search_skills_categorized(query, category);
            if let Some(s) = scope_filter {
                results.retain(|k| k.scope == s);
            }
            if let Some(kf) = kind_filter {
                results.retain(|k| k.kind() == kf);
            }

            if results.is_empty() {
                return Ok(format!(
                    "ℹ No skills found matching query '{}' (scope: '{}', kind: '{}').",
                    query,
                    scope_str,
                    kind_str.unwrap_or("all")
                ));
            }

            let mut out = format!(
                "📦 **MiniVault Skills** ({} found matching '{}' [scope: {}, kind: {}]):\n\n",
                results.len(),
                query,
                scope_str,
                kind_str.unwrap_or("all")
            );
            out.push_str("| Scope | Type | Category | Status | Skill Name | Description | Triggers |\n");
            out.push_str("| :--- | :--- | :--- | :--- | :--- | :--- | :--- |\n");

            for s in results.drain(..) {
                let scope_badge = match s.scope {
                    SkillScope::Project => "Project",
                    SkillScope::Global => "Global",
                    SkillScope::Builtin => "Built-in",
                };
                let type_badge = format!("[{}]", s.kind().badge());
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
                    "| `{}` | `{}` | `{}` | {} | **`{}`** | {} | `{}` |\n",
                    scope_badge, type_badge, s.category(), status_badge, s.name, s.description, triggers
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
            out.push_str(&format!(
                "• **Type**: `[{}] {}` ({})\n",
                skill.kind().badge(),
                skill.kind().as_str(),
                skill.kind().description()
            ));
            out.push_str(&format!("• **Category**: `{}`\n", skill.category()));

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
            let category = opt_str(args, "category");
            let kind = opt_str(args, "kind");
            let scope_str = opt_str(args, "scope").unwrap_or("project");

            let target_scope = if scope_str.eq_ignore_ascii_case("global") {
                SkillScope::Global
            } else {
                SkillScope::Project
            };

            match store.create_skill(target_scope, name, description, category, kind, instructions, triggers, globs) {
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
        "vault_bundle_list" => Some(async move {
            let query = opt_query(args).unwrap_or("").to_lowercase();
            let cat_filter = opt_str(args, "category").map(|c| c.trim().to_lowercase());

            let mut bundles = store.list_all_bundles();
            if let Some(ref cat) = cat_filter {
                if !cat.is_empty() {
                    bundles.retain(|b| b.category.eq_ignore_ascii_case(cat));
                }
            }
            if !query.is_empty() {
                bundles.retain(|b| b.matches_query(&query));
            }

            if bundles.is_empty() {
                return Ok("📦 No skill bundles found matching criteria.".to_string());
            }

            let mut out = format!("📦 **MiniVault Skill Bundles** ({} available):\n\n", bundles.len());
            out.push_str("| Scope | Category | Bundle Name | Included Skills | Description |\n");
            out.push_str("| :--- | :--- | :--- | :--- | :--- |\n");

            for b in bundles {
                let scope_badge = match b.scope {
                    SkillScope::Project => "`Project`",
                    SkillScope::Global => "`Global`",
                    SkillScope::Builtin => "`Built-in`",
                };
                let skills_summary = b.skills.join(", ");
                out.push_str(&format!(
                    "| {} | `{}` | **`{}`** | `{}` | {} |\n",
                    scope_badge, b.category, b.name, skills_summary, b.description
                ));
            }

            out.push_str("\n💡 Use `vault_bundle_load(name)` to install all constituent skills into this project.\n");
            Ok(out)
        }.await),

        "vault_bundle_load" => Some(async move {
            let name = match require_str(args, "name", "vault_bundle_load") {
                Ok(n) => n,
                Err(e) => return Err(e.into()),
            };

            match store.load_bundle_to_project(name) {
                Ok((loaded, failed)) => {
                    let mut msg = format!("✔ Successfully loaded skill bundle `{}` into project!\n", name);
                    msg.push_str(&format!("  • Installed skills ({}): {}\n", loaded.len(), loaded.join(", ")));
                    if !failed.is_empty() {
                        msg.push_str(&format!("  • Failed/skipped skills ({}): {}\n", failed.len(), failed.join(", ")));
                    }
                    msg.push_str("All active bundle skills are now automatically injected into orchestrator context.");
                    Ok(msg)
                }
                Err(e) => Err(ToolError::ExecutionFailed(format!("Failed to load bundle: {}", e)).into()),
            }
        }.await),

        "vault_bundle_create" => Some(async move {
            let name = match require_str(args, "name", "vault_bundle_create") {
                Ok(n) => n,
                Err(e) => return Err(e.into()),
            };
            let description = match require_str(args, "description", "vault_bundle_create") {
                Ok(d) => d,
                Err(e) => return Err(e.into()),
            };
            let skills = match opt_string_array(args, "skills") {
                Some(s) if !s.is_empty() => s,
                _ => return Err(ToolError::invalid_args("vault_bundle_create", "skills array must contain at least 1 skill").into()),
            };
            let category = opt_str(args, "category").unwrap_or("general");
            let tags = opt_string_array(args, "tags").unwrap_or_default();
            let scope_str = opt_str(args, "scope").unwrap_or("project");

            let target_scope = if scope_str.eq_ignore_ascii_case("global") {
                SkillScope::Global
            } else {
                SkillScope::Project
            };

            match store.create_bundle(target_scope, name, description, category, skills, tags) {
                Ok(bundle) => {
                    let path_str = bundle.path.map(|p| p.display().to_string()).unwrap_or_default();
                    Ok(format!(
                        "✔ Successfully created skill bundle `{}` [category: {}] with {} skills at `{}`.",
                        bundle.name, bundle.category, bundle.skills.len(), path_str
                    ))
                }
                Err(e) => Err(ToolError::ExecutionFailed(format!("Failed to create bundle: {}", e)).into()),
            }
        }.await),

        "vault_ingest_source" => Some(async move {
            let uri = match require_str(args, "uri", "vault_ingest_source") {
                Ok(u) => u,
                Err(e) => return Err(e.into()),
            };
            let title = opt_str(args, "title");
            let instructions = opt_str(args, "instructions");
            let scope_str = opt_str(args, "scope").unwrap_or("project");

            let target_scope = if scope_str.eq_ignore_ascii_case("global") {
                SkillScope::Global
            } else {
                SkillScope::Project
            };

            match store.ingest_source(uri, title, target_scope, instructions).await {
                Ok((source, skill)) => {
                    let mut msg = format!(
                        "✔ Successfully ingested external source `{}` as [{}] `{}`!\n",
                        source.uri,
                        source.kind.badge(),
                        source.title
                    );
                    msg.push_str(&format!("  • Registered in: `sources.json` (ID: `{}`)\n", source.id));
                    if let Some(s) = skill {
                        let path_display = s.path.map(|p| p.display().to_string()).unwrap_or_else(|| s.name.clone());
                        msg.push_str(&format!(
                            "  • Synthesized Vault [Doc]: `{}` [{:?}] at `{}`\n",
                            s.name, s.scope, path_display
                        ));
                    }
                    Ok(msg)
                }
                Err(e) => Err(ToolError::ExecutionFailed(format!("Failed to ingest source: {}", e)).into()),
            }
        }.await),

        "vault_gotchas_list" => Some(async move {
            let query = opt_query(args).unwrap_or("");
            let gotchas = store.find_relevant_gotchas(query);

            if gotchas.is_empty() {
                return Ok(format!(
                    "ℹ No universal gotchas found matching query '{}'. As compiler checks and auto-heal barriers succeed, hard-won fixes are automatically recorded here.",
                    query
                ));
            }

            let mut out = format!(
                "💡 **Universal Machine Gotchas & Compiler Traps** ({} found matching '{}'):\n\n",
                gotchas.len(),
                query
            );
            for g in gotchas {
                out.push_str(&format!(
                    "• **[{}]** Trap: `{}` (Encountered {}x)\n  - **Avoid:** {}\n  - **Fix:** {}\n\n",
                    g.context_scope, g.trigger, g.occurrence_count, g.failed_attempt, g.verified_fix
                ));
            }
            Ok(out)
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
        assert!(res.contains("[Doc]"));

        // 1b. Test search filtered by kind="reference"
        let ref_res = dispatch("vault_search", &json!({ "kind": "reference" }), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(ref_res.contains("react"));
        assert!(ref_res.contains("tailwind-v4"));
        assert!(!ref_res.contains("tdd-workflow"));

        // 1c. Test search filtered by kind="workflow"
        let wf_res = dispatch("vault_search", &json!({ "kind": "workflow" }), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(wf_res.contains("tdd-workflow"));
        assert!(wf_res.contains("security-audit"));
        assert!(!wf_res.contains("react"));

        // 2. Test show
        let show_res = dispatch("vault_show", &json!({ "name": "tailwind-v4" }), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(show_res.contains("Tailwind CSS v4 Guidelines"));
        assert!(show_res.contains("[Doc]"));
        assert!(show_res.contains("Technical Reference Guide"));

        // 3. Test load to project
        let load_res = dispatch("vault_load", &json!({ "name": "tailwind-v4" }), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(load_res.contains("Successfully loaded skill `tailwind-v4`"));

        // 4. Test create custom skill with kind="workflow"
        let create_res = dispatch(
            "vault_create",
            &json!({
                "name": "micro-services",
                "description": "Microservice resilience patterns",
                "kind": "workflow",
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

        // 6. Test bundle list
        let bundle_list_res = dispatch("vault_bundle_list", &json!({}), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(bundle_list_res.contains("fullstack-nextjs"));
        assert!(bundle_list_res.contains("rust-systems"));

        // 7. Test bundle load
        let bundle_load_res = dispatch(
            "vault_bundle_load",
            &json!({ "name": "fullstack-nextjs" }),
            ws,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(bundle_load_res.contains("Successfully loaded skill bundle `fullstack-nextjs`"));
        assert!(bundle_load_res.contains("react"));
        assert!(bundle_load_res.contains("nextjs"));

        // 8. Test vault_ingest_source (local repo)
        let fake_repo = ws.join("sample-pkg");
        let _ = std::fs::create_dir_all(&fake_repo);
        let _ = std::fs::write(
            fake_repo.join("README.md"),
            "# Sample Package\n\nFast async queue.",
        );
        let ingest_res = dispatch(
            "vault_ingest_source",
            &json!({
                "uri": fake_repo.to_string_lossy(),
                "title": "sample-pkg",
                "scope": "project"
            }),
            ws,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(ingest_res.contains("sample-pkg"));
        assert!(ingest_res.contains("sources.json"));

        // 9. Test vault_gotchas_list
        let gotchas_res = dispatch("vault_gotchas_list", &json!({ "query": "" }), ws)
            .await
            .unwrap()
            .unwrap();
        assert!(
            gotchas_res.contains("Universal Machine Gotchas")
                || gotchas_res.contains("No universal gotchas found")
        );
    }
}
