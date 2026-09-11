use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "wiki_write".to_string(),
            description: "Write or update a persistent Markdown knowledge document in the repository knowledge wiki (.minicode/wiki/<topic>.md).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Topic slug (e.g. 'architecture-database', 'oauth-flow')"
                    },
                    "title": {
                        "type": "string",
                        "description": "Human-readable title of the wiki document"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full Markdown content, guidelines, decisions, or instructions"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of search tags and categorization keywords"
                    },
                    "references": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Associated file paths or related wiki topic slugs"
                    }
                },
                "required": ["topic", "title", "content"]
            }),
        },
        ToolSchema {
            name: "wiki_read".to_string(),
            description: "Read a specific knowledge wiki document from .minicode/wiki/<topic>.md by topic slug.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Topic slug to read"
                    }
                },
                "required": ["topic"]
            }),
        },
        ToolSchema {
            name: "wiki_search".to_string(),
            description: "Search across repository knowledge wiki documents matching topic, title, tags, or content keywords.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search keyword or phrase"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSchema {
            name: "create_skill".to_string(),
            description: "Create and hot-load a new specialized skill package in .minicode/skills/<name>/SKILL.md with instructions and allowed tools.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Skill name identifier (e.g. 'rust-perf', 'db-migration')"
                    },
                    "description": {
                        "type": "string",
                        "description": "High-level summary of what this skill does"
                    },
                    "instructions": {
                        "type": "string",
                        "description": "Detailed multi-step markdown instructions, rules, and examples"
                    },
                    "allowed_tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of tool names this skill leverages"
                    }
                },
                "required": ["name", "description", "instructions"]
            }),
        },
        ToolSchema {
            name: "list_skills".to_string(),
            description: "List all discovered skills across workspace and user skill directories.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "inspect_skill".to_string(),
            description: "Inspect and read full markdown instructions and execution rules for a specific skill by name.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the skill to inspect"
                    }
                },
                "required": ["name"]
            }),
        },
    ]
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "wiki_write" => Some((|| {
            let topic = param::require_str(args, "topic", "wiki_write")?;
            let title = param::require_str(args, "title", "wiki_write")?;
            let content = param::require_str(args, "content", "wiki_write")?;
            let tags = param::opt_string_array(args, "tags").unwrap_or_default();
            let references = param::opt_string_array(args, "references").unwrap_or_default();

            let res = crate::context::wiki::WikiManager::write_entry(
                workspace_root,
                topic,
                title,
                content,
                &tags,
                &references,
            )?;
            Ok(res)
        })()),
        "wiki_read" => Some((|| {
            let topic = param::require_str(args, "topic", "wiki_read")?;
            let content = crate::context::wiki::WikiManager::read_entry(workspace_root, topic)?;
            Ok(content)
        })()),
        "wiki_search" => Some((|| {
            let query = param::require_str(args, "query", "wiki_search")?;
            let results = crate::context::wiki::WikiManager::search_entries(workspace_root, query)?;
            Ok(results)
        })()),
        "create_skill" => Some((|| {
            let name = param::require_str(args, "name", "create_skill")?;
            let description = param::require_str(args, "description", "create_skill")?;
            let instructions = param::require_str(args, "instructions", "create_skill")?;
            let allowed_tools = param::opt_string_array(args, "allowed_tools").unwrap_or_default();

            let res = crate::context::skill_forge::SkillForge::create_skill(
                workspace_root,
                name,
                description,
                instructions,
                &allowed_tools,
            )?;
            Ok(res)
        })()),
        "list_skills" => Some((|| {
            let res = crate::context::skill_forge::SkillForge::list_all_skills(workspace_root)?;
            Ok(res)
        })()),
        "inspect_skill" => Some((|| {
            let name = param::require_str(args, "name", "inspect_skill")?;
            let skill =
                crate::context::skill_forge::SkillForge::inspect_skill(workspace_root, name)?;
            let report = format!(
                "🛠️ Skill: **{}**\n📁 Path: `{}`\n📝 Description: _{}_\n\n## Instructions\n\n{}",
                skill.name,
                skill.path.display(),
                skill.description,
                skill.instructions
            );
            Ok(report)
        })()),
        _ => None,
    }
}
