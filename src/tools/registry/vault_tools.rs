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

/// Returns the 13 tool schemas for MiniVault.
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
        ToolSchema {
            name: "vault_gotcha_add".to_string(),
            description: "Record a universal technical gotcha, compiler trap, or non-obvious pitfall and its verified fix into the Universal Vault so future sessions automatically avoid the mistake.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "trigger": {
                        "type": "string",
                        "description": "Error signature, symptom, or task trigger where this pitfall occurs"
                    },
                    "context_scope": {
                        "type": "string",
                        "description": "Technology or language scope (e.g. 'rust', 'mcp', 'nextjs', 'python')"
                    },
                    "failed_attempt": {
                        "type": "string",
                        "description": "What approach failed or caused the bug"
                    },
                    "verified_fix": {
                        "type": "string",
                        "description": "The verified engineering fix that resolved the issue"
                    }
                },
                "required": ["trigger", "context_scope", "failed_attempt", "verified_fix"]
            }),
        },
    ]
}

fn parse_target_scope(args: &serde_json::Value) -> SkillScope {
    if opt_str(args, "scope")
        .unwrap_or("project")
        .eq_ignore_ascii_case("global")
    {
        SkillScope::Global
    } else {
        SkillScope::Project
    }
}

fn parse_strict_scope(args: &serde_json::Value, tool_name: &str) -> Result<SkillScope> {
    let scope_str = require_str(args, "scope", tool_name)?;
    if scope_str.eq_ignore_ascii_case("project") {
        Ok(SkillScope::Project)
    } else if scope_str.eq_ignore_ascii_case("global") {
        Ok(SkillScope::Global)
    } else {
        Err(ToolError::InvalidArguments {
            name: tool_name.to_string(),
            reason: "Scope must be either 'project' or 'global'".to_string(),
        }
        .into())
    }
}

/// Dispatches a tool invocation to the corresponding MiniVault handler.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    let store = VaultStore::new(workspace_root);

    let res: Result<String> = match tool_name {
        "vault_search" => {
            let query = opt_query(args).unwrap_or("");
            let scope_str = opt_str(args, "scope").unwrap_or("all");
            let category = opt_str(args, "category");
            let kind_str = opt_str(args, "kind");

            let scope_filter = if scope_str.eq_ignore_ascii_case("project") {
                Some(SkillScope::Project)
            } else if scope_str.eq_ignore_ascii_case("global") {
                Some(SkillScope::Global)
            } else if scope_str.eq_ignore_ascii_case("builtin") {
                Some(SkillScope::Builtin)
            } else {
                None
            };

            let kind_filter: Option<crate::vault::models::SkillKind> = match kind_str {
                Some(k) if !k.eq_ignore_ascii_case("all") => k.parse().ok(),
                _ => None,
            };

            let mut results = store.search_skills_categorized(query, category);
            if let Some(s) = scope_filter {
                results.retain(|k| k.scope == s);
            }
            if let Some(kf) = kind_filter {
                results.retain(|k| k.kind() == kf);
            }

            if results.is_empty() {
                Ok(format!(
                    "ℹ No skills found matching query '{}' (scope: '{}', kind: '{}').",
                    query,
                    scope_str,
                    kind_str.unwrap_or("all")
                ))
            } else {
                let mut out = format!(
                    "📦 **MiniVault Skills** ({} found matching '{}' [scope: {}, kind: {}]):\n\n",
                    results.len(),
                    query,
                    scope_str,
                    kind_str.unwrap_or("all")
                );
                out.push_str(
                    "| Scope | Type | Category | Status | Skill Name | Description | Triggers |\n",
                );
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
                        scope_badge,
                        type_badge,
                        s.category(),
                        status_badge,
                        s.name,
                        s.description,
                        triggers
                    ));
                }

                out.push_str("\n💡 Use `vault_show(name)` to inspect full rules, or `vault_load(name)` to install into this project.\n");
                Ok(out)
            }
        }

        "vault_show" | "vault_skill_show" => (|| {
            let name = require_str(args, "name", "vault_show")?;
            let skill = store
                .get_skill(name)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

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
                "📖 **Skill `{}`** [{} | {}]\n*{}*\n\n• **Type**: `[{}] {}` ({})\n• **Category**: `{}`\n",
                skill.name,
                scope_str,
                status_str,
                skill.description,
                skill.kind().badge(),
                skill.kind().as_str(),
                skill.kind().description(),
                skill.category()
            );

            if !skill.frontmatter.globs.is_empty() {
                out.push_str(&format!(
                    "• **Globs**: `{}`\n",
                    skill.frontmatter.globs.join(", ")
                ));
            }
            if !skill.frontmatter.triggers.is_empty() {
                out.push_str(&format!(
                    "• **Triggers**: `{}`\n",
                    skill.frontmatter.triggers.join(", ")
                ));
            }
            if !skill.frontmatter.allowed_tools.is_empty() {
                out.push_str(&format!(
                    "• **Allowed Tools**: `{}`\n",
                    skill.frontmatter.allowed_tools.join(", ")
                ));
            }
            if let Some(path) = &skill.path {
                out.push_str(&format!("• **File Path**: `{}`\n", path.display()));
            }
            let bundled = skill.discover_bundled_resources();
            if !bundled.is_empty() {
                out.push_str(&format!(
                    "• **Bundled Level-3 Resources**: `{}`\n",
                    bundled.join("`, `")
                ));
            }

            out.push_str("\n---\n\n");
            out.push_str(&skill.instructions);
            out.push('\n');

            Ok(out)
        })(),

        "vault_load" | "vault_skill_load" => (|| {
            let name = require_str(args, "name", "vault_load")?;
            let skill = store
                .load_to_project(name)
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to load skill: {}", e)))?;
            let dest = skill
                .path
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ".minicode/skills/".to_string());
            Ok(format!(
                "✔ Successfully loaded skill `{}` into active project at `{}`.\n\
                 Orchestrator context has activated this skill for subsequent turns.",
                skill.name, dest
            ))
        })(),

        "vault_unload" => (|| {
            let name = require_str(args, "name", "vault_unload")?;
            store.unload_from_project(name).map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to unload skill: {}", e))
            })?;
            Ok(format!(
                "✔ Successfully unloaded skill `{}` from active project workspace.",
                name
            ))
        })(),

        "vault_create" => (|| {
            let name = require_str(args, "name", "vault_create")?;
            let description = require_str(args, "description", "vault_create")?;
            let instructions = require_str(args, "instructions", "vault_create")?;
            let triggers = opt_string_array(args, "triggers").unwrap_or_default();
            let globs = opt_string_array(args, "globs").unwrap_or_default();
            let category = opt_str(args, "category");
            let kind = opt_str(args, "kind");
            let target_scope = parse_target_scope(args);

            let skill = store
                .create_skill(
                    target_scope,
                    name,
                    description,
                    category,
                    kind,
                    instructions,
                    triggers,
                    globs,
                )
                .map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to create skill: {}", e))
                })?;
            let path_str = skill
                .path
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let line_count = skill.instructions.lines().count();
            let advisory = if line_count > 500 {
                format!(
                    "\n💡 Progressive Disclosure Advisory (`skill-creator`): `SKILL.md` is {} lines (>500). Consider moving deep reference tables or templates into `references/` or `scripts/` inside the skill folder.",
                    line_count
                )
            } else {
                String::new()
            };
            Ok(format!(
                "✔ Successfully forged new skill `{}` [{:?}] at `{}` (triggers: `{}`).{}",
                skill.name,
                skill.scope,
                path_str,
                skill.frontmatter.triggers.join(", "),
                advisory
            ))
        })(),

        "vault_update" => (|| {
            let name = require_str(args, "name", "vault_update")?;
            let instructions = require_str(args, "instructions", "vault_update")?;
            let target_scope = parse_target_scope(args);

            let skill = store
                .update_skill(target_scope, name, instructions)
                .map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to update skill: {}", e))
                })?;
            Ok(format!(
                "✔ Successfully updated skill `{}` in {:?} scope.",
                skill.name, skill.scope
            ))
        })(),

        "vault_delete" => (|| {
            let name = require_str(args, "name", "vault_delete")?;
            let target_scope = parse_strict_scope(args, "vault_delete")?;

            store.delete_skill(target_scope, name).map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to delete skill: {}", e))
            })?;
            Ok(format!(
                "✔ Successfully deleted skill `{}` from {:?} vault.",
                name, target_scope
            ))
        })(),

        "vault_import_url" => {
            async {
                let url = require_str(args, "url", "vault_import_url")?;
                let name_override = opt_str(args, "name");
                let target_scope = parse_target_scope(args);

                let skill = store
                    .import_from_url(url, target_scope, name_override)
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to import skill from URL: {}", e))
                    })?;
                let path_str = skill
                    .path
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                Ok(format!(
                    "✔ Successfully imported skill `{}` from `{}` into {:?} vault at `{}`.",
                    skill.name, url, skill.scope, path_str
                ))
            }
            .await
        }

        "vault_bundle_list" => {
            let query = opt_query(args).unwrap_or("").to_lowercase();
            let cat_filter = opt_str(args, "category").map(str::trim).filter(|c| !c.is_empty());

            let mut bundles = store.list_all_bundles();
            if let Some(cat) = cat_filter {
                bundles.retain(|b| b.category.eq_ignore_ascii_case(cat));
            }
            if !query.is_empty() {
                bundles.retain(|b| b.matches_query(&query));
            }

            if bundles.is_empty() {
                Ok("📦 No skill bundles found matching criteria.".to_string())
            } else {
                let mut out = format!(
                    "📦 **MiniVault Skill Bundles** ({} available):\n\n",
                    bundles.len()
                );
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
            }
        }

        "vault_bundle_load" => (|| {
            let name = require_str(args, "name", "vault_bundle_load")?;
            let (loaded, failed) = store
                .load_bundle_to_project(name)
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to load bundle: {}", e)))?;
            let mut msg = format!(
                "✔ Successfully loaded skill bundle `{}` into project!\n  • Installed skills ({}): {}\n",
                name,
                loaded.len(),
                loaded.join(", ")
            );
            if !failed.is_empty() {
                msg.push_str(&format!(
                    "  • Failed/skipped skills ({}): {}\n",
                    failed.len(),
                    failed.join(", ")
                ));
            }
            msg.push_str(
                "All active bundle skills are now automatically injected into orchestrator context.",
            );
            Ok(msg)
        })(),

        "vault_bundle_create" => (|| {
            let name = require_str(args, "name", "vault_bundle_create")?;
            let description = require_str(args, "description", "vault_bundle_create")?;
            let skills = opt_string_array(args, "skills")
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ToolError::invalid_args(
                        "vault_bundle_create",
                        "skills array must contain at least 1 skill",
                    )
                })?;
            let category = opt_str(args, "category").unwrap_or("general");
            let tags = opt_string_array(args, "tags").unwrap_or_default();
            let target_scope = parse_target_scope(args);

            let bundle = store
                .create_bundle(target_scope, name, description, category, skills, tags)
                .map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to create bundle: {}", e))
                })?;
            let path_str = bundle
                .path
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            Ok(format!(
                "✔ Successfully created skill bundle `{}` [category: {}] with {} skills at `{}`.",
                bundle.name,
                bundle.category,
                bundle.skills.len(),
                path_str
            ))
        })(),

        "vault_ingest_source" => {
            async {
                let uri = require_str(args, "uri", "vault_ingest_source")?;
                let title = opt_str(args, "title");
                let instructions = opt_str(args, "instructions");
                let target_scope = parse_target_scope(args);

                let (source, skill) = store
                    .ingest_source(uri, title, target_scope, instructions)
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to ingest source: {}", e))
                    })?;
                let mut msg = format!(
                    "✔ Successfully ingested external source `{}` as [{}] `{}`!\n  • Registered in: `sources.json` (ID: `{}`)\n",
                    source.uri,
                    source.kind.badge(),
                    source.title,
                    source.id
                );
                if let Some(s) = skill {
                    let path_display = s
                        .path
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| s.name.clone());
                    msg.push_str(&format!(
                        "  • Synthesized Vault [Doc]: `{}` [{:?}] at `{}`\n",
                        s.name, s.scope, path_display
                    ));
                }
                Ok(msg)
            }
            .await
        }

        "vault_gotchas_list" => {
            let query = opt_query(args).unwrap_or("");
            let gotchas = store.find_relevant_gotchas(query);

            if gotchas.is_empty() {
                Ok(format!(
                    "ℹ No universal gotchas found matching query '{}'. As compiler checks and auto-heal barriers succeed, hard-won fixes are automatically recorded here.",
                    query
                ))
            } else {
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
            }
        }

        "vault_gotcha_add" => (|| {
            let trigger = require_str(args, "trigger", "vault_gotcha_add")?;
            let context_scope = require_str(args, "context_scope", "vault_gotcha_add")?;
            let failed_attempt = require_str(args, "failed_attempt", "vault_gotcha_add")?;
            let verified_fix = require_str(args, "verified_fix", "vault_gotcha_add")?;

            let gotcha = crate::vault::models::GlobalGotcha {
                id: format!("gotcha-{}", uuid::Uuid::new_v4().simple()),
                trigger: trigger.to_string(),
                context_scope: context_scope.to_string(),
                failed_attempt: failed_attempt.to_string(),
                verified_fix: verified_fix.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                occurrence_count: 1,
            };
            store.add_or_update_global_gotcha(gotcha).map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to record gotcha: {}", e))
            })?;
            Ok(format!(
                "✔ Recorded universal gotcha `[{}] {}` into Universal Vault.",
                context_scope, trigger
            ))
        })(),

        _ => return None,
    };

    Some(res)
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
