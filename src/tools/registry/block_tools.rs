//! MiniBlocks UI component & design token warehouse tools.
//!
//! Provides fast local-first component search, retrieval, code injection,
//! curation, palettes, gradients, scaffolding, and warehouse analytics.

use crate::agent::provider::ToolSchema;
use crate::blocks::models::{BlockCategory, BlockComponent, BlockFramework};
use crate::blocks::seed::detect_project_framework;
use crate::blocks::store::{get_global_block_store, BlockSearchFilter};
use crate::error::{Result, ToolError};
use crate::tools::param::*;
use serde_json::json;
use std::path::Path;
use uuid::Uuid;

/// Returns the 10 tool schemas for the MiniBlocks UI component & design token warehouse.
pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "block_search".to_string(),
            description: "Search UI components in the MiniBlocks warehouse by keywords, category, framework, and tags with relevance scoring.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Keyword search query across component name, description, tags, and code"
                    },
                    "category": {
                        "type": "string",
                        "description": "Category filter (e.g. navbar, hero, footer, card, modal, pricing, form, table, etc.)"
                    },
                    "framework": {
                        "type": "string",
                        "description": "Framework filter (react, tailwind, svelte, shadcn, css). If omitted, auto-detects from workspace."
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tag filters (e.g. ['dark', 'responsive', 'minimal'])"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 10, max: 50)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "block_get".to_string(),
            description: "Retrieve full details, code, dependencies, and metadata of a UI component by UUID or slug/name.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "UUID of the component"
                    },
                    "name": {
                        "type": "string",
                        "description": "Exact slug or name of the component"
                    }
                }
            }),
        },
        ToolSchema {
            name: "block_insert".to_string(),
            description: "Inject a MiniBlocks UI component or raw code snippet into a target file in the workspace with configurable insertion mode.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_file": {
                        "type": "string",
                        "description": "Path to file in workspace to inject component into"
                    },
                    "component_id": {
                        "type": "string",
                        "description": "UUID or name of component to inject"
                    },
                    "code": {
                        "type": "string",
                        "description": "Raw code snippet to insert if component_id is omitted"
                    },
                    "mode": {
                        "type": "string",
                        "description": "Insertion mode: 'append', 'prepend', 'create', or 'replace' (default: 'append')",
                        "enum": ["append", "prepend", "create", "replace"]
                    }
                },
                "required": ["target_file"]
            }),
        },
        ToolSchema {
            name: "block_save".to_string(),
            description: "Save a new custom UI component to the MiniBlocks warehouse.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Unique component name or slug"
                    },
                    "description": {
                        "type": "string",
                        "description": "Description of UI component and usage"
                    },
                    "category": {
                        "type": "string",
                        "description": "Category name (e.g. navbar, hero, footer, card, modal, etc.)"
                    },
                    "code": {
                        "type": "string",
                        "description": "Source code of the component"
                    },
                    "framework": {
                        "type": "string",
                        "description": "Target framework (react, tailwind, svelte, shadcn, css). Defaults to workspace detection or tailwind."
                    },
                    "dependencies": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Required npm or crate packages"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Search and classification tags"
                    }
                },
                "required": ["name", "description", "category", "code"]
            }),
        },
        ToolSchema {
            name: "block_update".to_string(),
            description: "Update code, description, or tags of an existing UI component in the MiniBlocks warehouse, incrementing its version.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "UUID of the component to update"
                    },
                    "code": {
                        "type": "string",
                        "description": "New source code for the component"
                    },
                    "description": {
                        "type": "string",
                        "description": "Updated description"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Updated search tags"
                    }
                },
                "required": ["id"]
            }),
        },
        ToolSchema {
            name: "block_delete".to_string(),
            description: "Delete a UI component from the MiniBlocks warehouse by UUID.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "UUID of the component to delete"
                    }
                },
                "required": ["id"]
            }),
        },
        ToolSchema {
            name: "block_palettes".to_string(),
            description: "Search and list 4-hex curated color palettes with background, surface, accent, text tokens, and export suggestions in CSS, Tailwind, SCSS, or JSON format.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Tag or name search filter (e.g. 'dark', 'nordic', 'cyberpunk', 'warm')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of palettes to return (default: 10)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["css", "tailwind", "scss", "json"],
                        "description": "Export format for color tokens: 'css' (CSS variables), 'tailwind' (theme.extend.colors config), 'scss' ($color variables), or 'json' (design token dictionary). Defaults to 'css'."
                    }
                }
            }),
        },
        ToolSchema {
            name: "block_gradients".to_string(),
            description: "Search and list modern CSS gradients with CSS rules, hex color stops, and tags.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Tag or name search filter (e.g. 'purple', 'sunset', 'ocean', 'mesh')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of gradients to return (default: 10)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "block_scaffold".to_string(),
            description: "Scaffold a complete UI layout template (landing page, portfolio, dashboard) with component assembly into the workspace.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "template_name": {
                        "type": "string",
                        "description": "Template name or ID (e.g. 'landing', 'portfolio', 'dashboard', or full template name)"
                    },
                    "target_dir": {
                        "type": "string",
                        "description": "Target directory in workspace (default: 'src/components')"
                    },
                    "framework": {
                        "type": "string",
                        "description": "Framework override (react, tailwind, svelte, shadcn, css)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "block_stats".to_string(),
            description: "Overview statistics of the MiniBlocks warehouse: total components, palettes, gradients, templates, category breakdown, and framework breakdown.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

/// Sanitizes a string into a safe filename slug.
fn sanitize_filename(name: &str) -> String {
    let clean: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = clean.trim_matches('_');
    if trimmed.is_empty() {
        "component".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Dispatches a tool invocation to the corresponding MiniBlocks handler.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "block_search" | "miniblock_search" => Some(
            async {
                let query = opt_query(args).map(|s| s.to_string());
                let category = opt_str(args, "category").map(BlockCategory::from_str_loose);
                let framework = if let Some(fw_str) = opt_str(args, "framework") {
                    let clean = fw_str.trim().to_lowercase();
                    if clean == "all" || clean == "any" {
                        None
                    } else {
                        Some(BlockFramework::from_str_loose(fw_str))
                    }
                } else {
                    detect_project_framework(workspace_root)
                };
                let tags = opt_string_array(args, "tags");
                let limit = opt_limit(args, 10).clamp(1, 50);

                let store = get_global_block_store().read().map_err(|e| {
                    ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                })?;
                let filter = BlockSearchFilter {
                    query,
                    category,
                    framework,
                    tags,
                    limit,
                };
                let results = store.search_components(&filter);
                if results.is_empty() {
                    return Ok("No components found matching the search criteria.".to_string());
                }

                let mut out = format!("Found {} matching component(s):\n\n", results.len());
                out.push_str("| Name | Category | Framework | Version | Tags | Score | ID | Description |\n");
                out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
                for r in results {
                    let tags_str = if r.tags.is_empty() {
                        "-".to_string()
                    } else {
                        r.tags.join(", ").replace('|', "\\|")
                    };
                    let name_escaped = r.name.replace('|', "\\|");
                    let desc_escaped = r.description.replace('|', "\\|").replace('\n', " ");
                    out.push_str(&format!(
                        "| {} | {} | {} | {} | {} | {:.1} | `{}` | {} |\n",
                        name_escaped,
                        r.category,
                        r.framework,
                        r.version,
                        tags_str,
                        r.score,
                        r.id,
                        desc_escaped
                    ));
                }
                Ok(out)
            }
            .await,
        ),

        "block_get" | "miniblock_get" => Some(
            async {
                let id_arg = opt_str(args, "id").or_else(|| opt_str(args, "component_id"));
                let name_arg = opt_str(args, "name").or_else(|| opt_str(args, "slug"));

                if id_arg.is_none() && name_arg.is_none() {
                    return Err(ToolError::InvalidArguments {
                        name: "block_get".to_string(),
                        reason: "Either 'id' or 'name' must be provided".to_string(),
                    }
                    .into());
                }

                let store = get_global_block_store().read().map_err(|e| {
                    ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                })?;

                let comp = if let Some(id_str) = id_arg {
                    if let Ok(uuid) = Uuid::parse_str(id_str.trim()) {
                        store
                            .get_component(&uuid)
                            .or_else(|| store.get_component_by_name(id_str))
                    } else {
                        store.get_component_by_name(id_str)
                    }
                } else {
                    None
                };

                let comp = comp.or_else(|| {
                    name_arg.and_then(|name| store.get_component_by_name(name))
                });

                let comp = comp.ok_or_else(|| {
                    let target = id_arg.or(name_arg).unwrap_or("unknown");
                    ToolError::ExecutionFailed(format!(
                        "Component '{}' not found in MiniBlocks warehouse",
                        target
                    ))
                })?;

                let deps = if comp.dependencies.is_empty() {
                    "None".to_string()
                } else {
                    comp.dependencies.join(", ")
                };
                let tags = if comp.tags.is_empty() {
                    "None".to_string()
                } else {
                    comp.tags.join(", ")
                };

                let out = format!(
                    "# Component: {}\n\n- **ID:** `{}`\n- **Category:** {}\n- **Framework:** {}\n- **Version:** {}\n- **Dependencies:** {}\n- **Tags:** {}\n- **Description:** {}\n\n## Source Code\n```{}\n{}\n```\n",
                    comp.name,
                    comp.id,
                    comp.category,
                    comp.framework,
                    comp.version,
                    deps,
                    tags,
                    comp.description,
                    comp.framework,
                    comp.code
                );
                Ok(out)
            }
            .await,
        ),

        "block_insert" | "miniblock_insert" => Some(
            async {
                let target_file = get_str_with_aliases(
                    args,
                    &["target_file", "path", "file_path", "file", "target"],
                )
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "block_insert".to_string(),
                    reason: "Missing required argument 'target_file'".to_string(),
                })?;

                let comp_id_arg = opt_str(args, "component_id")
                    .or_else(|| opt_str(args, "id"))
                    .or_else(|| opt_str(args, "name"));
                let raw_code_arg = opt_str(args, "code");

                if comp_id_arg.is_none() && raw_code_arg.is_none() {
                    return Err(ToolError::InvalidArguments {
                        name: "block_insert".to_string(),
                        reason: "Either 'component_id' or 'code' must be provided".to_string(),
                    }
                    .into());
                }

                let mode = opt_str(args, "mode").unwrap_or("append");

                let (code_to_insert, comp_name, deps) = if let Some(cid) = comp_id_arg {
                    let store = get_global_block_store().read().map_err(|e| {
                        ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                    })?;
                    let comp = if let Ok(uuid) = Uuid::parse_str(cid.trim()) {
                        store
                            .get_component(&uuid)
                            .or_else(|| store.get_component_by_name(cid))
                    } else {
                        store.get_component_by_name(cid)
                    };
                    let comp = comp.ok_or_else(|| {
                        ToolError::ExecutionFailed(format!("Component '{}' not found", cid))
                    })?;
                    (
                        comp.code.clone(),
                        Some(comp.name.clone()),
                        comp.dependencies.clone(),
                    )
                } else {
                    let code = raw_code_arg.unwrap_or_default().to_string();
                    (code, None, Vec::new())
                };

                let file_path = crate::sandbox::path::validate_path_in_workspace(
                    workspace_root,
                    Path::new(target_file),
                )?;

                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
                        path: parent.display().to_string(),
                        source: e,
                    })?;
                }

                let mode_clean = mode.to_ascii_lowercase();
                let final_content = match mode_clean.as_str() {
                    "create" | "replace" => code_to_insert.clone(),
                    "prepend" => {
                        if file_path.exists() {
                            let existing = std::fs::read_to_string(&file_path).map_err(|e| {
                                ToolError::FileOp {
                                    path: file_path.display().to_string(),
                                    source: e,
                                }
                            })?;
                            format!("{}\n\n{}", code_to_insert, existing)
                        } else {
                            code_to_insert.clone()
                        }
                    }
                    "append" => {
                        if file_path.exists() {
                            let existing = std::fs::read_to_string(&file_path).map_err(|e| {
                                ToolError::FileOp {
                                    path: file_path.display().to_string(),
                                    source: e,
                                }
                            })?;
                            if existing.ends_with('\n') {
                                format!("{}{}", existing, code_to_insert)
                            } else {
                                format!("{}\n\n{}", existing, code_to_insert)
                            }
                        } else {
                            code_to_insert.clone()
                        }
                    }
                    other => {
                        return Err(ToolError::InvalidArguments {
                            name: "block_insert".to_string(),
                            reason: format!(
                                "Invalid mode '{}'. Supported modes are: append, prepend, create, replace",
                                other
                            ),
                        }
                        .into());
                    }
                };

                std::fs::write(&file_path, &final_content).map_err(|e| ToolError::FileOp {
                    path: file_path.display().to_string(),
                    source: e,
                })?;

                let lines_inserted = code_to_insert.lines().count();
                let deps_info = if deps.is_empty() {
                    "None".to_string()
                } else {
                    deps.join(", ")
                };
                let comp_info = comp_name
                    .map(|n| format!(" ({})", n))
                    .unwrap_or_default();

                Ok(format!(
                    "Successfully inserted component{} into `{}`.\n- Mode: {}\n- Lines inserted: {}\n- Dependencies required: {}\n",
                    comp_info, target_file, mode, lines_inserted, deps_info
                ))
            }
            .await,
        ),

        "block_save" | "miniblock_save" => Some(
            async {
                let name = require_str(args, "name", "block_save")?;
                let description = require_str(args, "description", "block_save")?;
                let cat_str = require_str(args, "category", "block_save")?;
                let category = BlockCategory::from_str_loose(cat_str);
                let code = require_str(args, "code", "block_save")?;

                let framework = if let Some(fw_str) = opt_str(args, "framework") {
                    BlockFramework::from_str_loose(fw_str)
                } else {
                    detect_project_framework(workspace_root)
                        .unwrap_or(BlockFramework::Tailwind)
                };

                let dependencies = opt_string_array(args, "dependencies").unwrap_or_default();
                let tags = opt_string_array(args, "tags").unwrap_or_default();

                let comp = BlockComponent::new(
                    name,
                    description,
                    category,
                    framework,
                    code,
                    dependencies.clone(),
                    tags.clone(),
                );
                let comp_id = comp.id;

                let mut store = get_global_block_store().write().map_err(|e| {
                    ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                })?;
                store
                    .insert_component(comp)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                let deps_str = if dependencies.is_empty() {
                    "None".to_string()
                } else {
                    dependencies.join(", ")
                };
                let tags_str = if tags.is_empty() {
                    "None".to_string()
                } else {
                    tags.join(", ")
                };

                Ok(format!(
                    "Component '{}' saved successfully.\n- ID: `{}`\n- Version: 1\n- Category: {}\n- Framework: {}\n- Dependencies: {}\n- Tags: {}\n",
                    name, comp_id, category, framework, deps_str, tags_str
                ))
            }
            .await,
        ),

        "block_update" | "miniblock_update" => Some(
            async {
                let id_str = require_str(args, "id", "block_update")?;
                let uuid = Uuid::parse_str(id_str.trim()).map_err(|e| ToolError::InvalidArguments {
                    name: "block_update".to_string(),
                    reason: format!("Invalid UUID '{}': {}", id_str, e),
                })?;

                let code = opt_str(args, "code");
                let description = opt_str(args, "description");
                let tags = opt_string_array(args, "tags");

                let mut store = get_global_block_store().write().map_err(|e| {
                    ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                })?;
                let updated = store
                    .update_component(&uuid, code, description, tags)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                let tags_str = if updated.tags.is_empty() {
                    "None".to_string()
                } else {
                    updated.tags.join(", ")
                };

                Ok(format!(
                    "Component '{}' updated successfully.\n- ID: `{}`\n- Version: {}\n- Category: {}\n- Framework: {}\n- Description: {}\n- Tags: {}\n- Updated At: {}\n",
                    updated.name,
                    updated.id,
                    updated.version,
                    updated.category,
                    updated.framework,
                    updated.description,
                    tags_str,
                    updated.updated_at
                ))
            }
            .await,
        ),

        "block_delete" | "miniblock_delete" => Some(
            async {
                let id_str = require_str(args, "id", "block_delete")?;
                let uuid = Uuid::parse_str(id_str.trim()).map_err(|e| ToolError::InvalidArguments {
                    name: "block_delete".to_string(),
                    reason: format!("Invalid UUID '{}': {}", id_str, e),
                })?;

                let mut store = get_global_block_store().write().map_err(|e| {
                    ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                })?;
                store
                    .delete_component(&uuid)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                Ok(format!(
                    "Component `{}` deleted successfully from MiniBlocks warehouse.\n",
                    uuid
                ))
            }
            .await,
        ),

        "block_palettes" | "miniblock_palettes" => Some(
            async {
                let query = opt_str(args, "query").unwrap_or("");
                let limit = opt_limit(args, 10).clamp(1, 50);
                let format_arg = opt_str(args, "format").unwrap_or("css");

                let store = get_global_block_store().read().map_err(|e| {
                    ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                })?;
                let mut palettes = store.search_palettes(query);
                if palettes.is_empty() {
                    return Ok("No color palettes found matching the search criteria.".to_string());
                }
                palettes.truncate(limit);

                let mut out = format!("Found {} color palette(s):\n\n", palettes.len());
                for pal in palettes {
                    let tags_str = if pal.tags.is_empty() {
                        "None".to_string()
                    } else {
                        pal.tags.join(", ")
                    };
                    let (tokens_code, lang) = pal.format_tokens(format_arg);
                    out.push_str(&format!(
                        "### {}\n- **ID:** `{}`\n- **Tags:** {}\n- **Tokens:**\n  - Background: `{}`\n  - Surface: `{}`\n  - Accent: `{}`\n  - Text: `{}`\n\n```{}\n{}\n```\n\n",
                        pal.name,
                        pal.id,
                        tags_str,
                        pal.colors[0],
                        pal.colors[1],
                        pal.colors[2],
                        pal.colors[3],
                        lang,
                        tokens_code
                    ));
                }
                Ok(out)
            }
            .await,
        ),

        "block_gradients" | "miniblock_gradients" => Some(
            async {
                let query = opt_str(args, "query").unwrap_or("");
                let limit = opt_limit(args, 10).clamp(1, 50);

                let store = get_global_block_store().read().map_err(|e| {
                    ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                })?;
                let mut gradients = store.search_gradients(query);
                if gradients.is_empty() {
                    return Ok("No CSS gradients found matching the search criteria.".to_string());
                }
                gradients.truncate(limit);

                let mut out = format!("Found {} CSS gradient(s):\n\n", gradients.len());
                for grad in gradients {
                    let tags_str = if grad.tags.is_empty() {
                        "None".to_string()
                    } else {
                        grad.tags.join(", ")
                    };
                    let colors_str = if grad.colors.is_empty() {
                        "None".to_string()
                    } else {
                        grad.colors.join(", ")
                    };
                    out.push_str(&format!(
                        "### {}\n- **ID:** `{}`\n- **Colors:** {}\n- **Tags:** {}\n\n```css\nbackground: {};\n```\n\n",
                        grad.name, grad.id, colors_str, tags_str, grad.css
                    ));
                }
                Ok(out)
            }
            .await,
        ),

        "block_scaffold" | "miniblock_scaffold" => Some(
            async {
                let template_name_arg = opt_str(args, "template_name")
                    .or_else(|| opt_str(args, "template"))
                    .unwrap_or("landing");
                let target_dir_arg = opt_str(args, "target_dir").unwrap_or("src/components");
                let fw_override = opt_str(args, "framework").map(BlockFramework::from_str_loose);

                let store = get_global_block_store().read().map_err(|e| {
                    ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                })?;
                let templates = store.list_templates();
                if templates.is_empty() {
                    return Err(ToolError::ExecutionFailed(
                        "No layout templates found in warehouse".to_string(),
                    )
                    .into());
                }

                let q = template_name_arg.trim().to_lowercase();
                let tmpl = if let Ok(uuid) = Uuid::parse_str(&q) {
                    store.get_template(&uuid)
                } else if !q.is_empty() {
                    templates
                        .iter()
                        .find(|t| t.name.to_lowercase().contains(&q))
                        .copied()
                        .or_else(|| {
                            templates
                                .iter()
                                .find(|t| {
                                    let name = t.name.to_lowercase();
                                    (q.contains("landing") && name.contains("landing"))
                                        || (q.contains("portfolio") && name.contains("portfolio"))
                                        || (q.contains("dashboard") && name.contains("dashboard"))
                                })
                                .copied()
                        })
                } else {
                    templates.first().copied()
                };

                let tmpl = tmpl.ok_or_else(|| {
                    ToolError::ExecutionFailed(format!(
                        "Template '{}' not found in MiniBlocks warehouse. Available templates: {}",
                        template_name_arg,
                        templates
                            .iter()
                            .map(|t| t.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })?;

                let base_path = crate::sandbox::path::validate_path_in_workspace(
                    workspace_root,
                    Path::new(target_dir_arg),
                )?;

                std::fs::create_dir_all(&base_path).map_err(|e| ToolError::FileOp {
                    path: base_path.display().to_string(),
                    source: e,
                })?;

                let mut written_files = Vec::new();
                let mut section_snippets = Vec::new();
                let mut all_deps = std::collections::HashSet::new();

                for comp_id in &tmpl.component_ids {
                    if let Some(comp) = store.get_component(comp_id) {
                        let fw = fw_override.unwrap_or(comp.framework);
                        let ext = match fw {
                            BlockFramework::React | BlockFramework::Shadcn => "tsx",
                            BlockFramework::Svelte => "svelte",
                            _ => "html",
                        };
                        let file_stem = sanitize_filename(&comp.name);
                        let filename = format!("{}.{}", file_stem, ext);
                        let dest = crate::sandbox::path::validate_path_in_workspace(
                            workspace_root,
                            &base_path.join(&filename),
                        )?;
                        std::fs::write(&dest, &comp.code).map_err(|e| ToolError::FileOp {
                            path: dest.display().to_string(),
                            source: e,
                        })?;
                        written_files.push((
                            format!("{}/{}", target_dir_arg.trim_end_matches('/'), filename),
                            comp.name.clone(),
                        ));
                        section_snippets.push(comp.code.clone());
                        for dep in &comp.dependencies {
                            all_deps.insert(dep.clone());
                        }
                    }
                }

                let base_template =
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&tmpl.base_layout) {
                        v.get("base_template")
                            .and_then(|s| s.as_str())
                            .unwrap_or(&tmpl.base_layout)
                            .to_string()
                    } else {
                        tmpl.base_layout.clone()
                    };

                let sections_joined = section_snippets.join("\n\n");
                let title = tmpl
                    .default_variables
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or(&tmpl.name);
                let assembled = base_template
                    .replace("{{ sections }}", &sections_joined)
                    .replace("{{ title | default('SaaS Landing Page') }}", title)
                    .replace("{{ title }}", title);

                let layout_filename = match fw_override {
                    Some(BlockFramework::React) | Some(BlockFramework::Shadcn) => "App.tsx",
                    Some(BlockFramework::Svelte) => "App.svelte",
                    _ => "index.html",
                };
                let layout_dest = crate::sandbox::path::validate_path_in_workspace(
                    workspace_root,
                    &base_path.join(layout_filename),
                )?;
                std::fs::write(&layout_dest, &assembled).map_err(|e| ToolError::FileOp {
                    path: layout_dest.display().to_string(),
                    source: e,
                })?;
                written_files.push((
                    format!(
                        "{}/{}",
                        target_dir_arg.trim_end_matches('/'),
                        layout_filename
                    ),
                    format!("Complete {} Page Layout", tmpl.name),
                ));

                let mut out = format!("# Scaffolding Completed: {}\n\n", tmpl.name);
                out.push_str(&format!("- **Template:** {}\n", tmpl.name));
                out.push_str(&format!("- **Target Directory:** `{}`\n", target_dir_arg));
                if let Some(fw) = fw_override {
                    out.push_str(&format!("- **Framework Override:** {}\n", fw));
                }
                out.push_str("\n## Files Written:\n");
                for (file, desc) in &written_files {
                    out.push_str(&format!("- `{}` ({})\n", file, desc));
                }
                let mut deps_list: Vec<String> = all_deps.into_iter().collect();
                deps_list.sort();
                let deps_str = if deps_list.is_empty() {
                    "None".to_string()
                } else {
                    deps_list.join(", ")
                };
                out.push_str(&format!("\n## Dependencies Required:\n- {}\n", deps_str));
                Ok(out)
            }
            .await,
        ),

        "block_stats" | "miniblock_stats" => Some(
            async {
                let store = get_global_block_store().read().map_err(|e| {
                    ToolError::ExecutionFailed(format!("BlockStore lock error: {}", e))
                })?;
                let stats = store.stats();

                let mut out = String::from("# MiniBlocks UI Warehouse Statistics\n\n");
                out.push_str(&format!(
                    "- **Total Components:** {}\n",
                    stats.total_components
                ));
                out.push_str(&format!("- **Total Palettes:** {}\n", stats.total_palettes));
                out.push_str(&format!(
                    "- **Total Gradients:** {}\n",
                    stats.total_gradients
                ));
                out.push_str(&format!(
                    "- **Total Templates:** {}\n\n",
                    stats.total_templates
                ));

                out.push_str("## Category Distribution\n");
                out.push_str("| Category | Components |\n");
                out.push_str("| --- | --- |\n");
                let mut cats: Vec<(&BlockCategory, &usize)> = stats.category_counts.iter().collect();
                cats.sort_by(|a, b| {
                    b.1.cmp(a.1)
                        .then_with(|| a.0.to_string().cmp(&b.0.to_string()))
                });
                for (cat, count) in cats {
                    out.push_str(&format!("| {} | {} |\n", cat, count));
                }

                out.push_str("\n## Framework Distribution\n");
                out.push_str("| Framework | Components |\n");
                out.push_str("| --- | --- |\n");
                let mut fws: Vec<(&BlockFramework, &usize)> =
                    stats.framework_counts.iter().collect();
                fws.sort_by(|a, b| {
                    b.1.cmp(a.1)
                        .then_with(|| a.0.to_string().cmp(&b.0.to_string()))
                });
                for (fw, count) in fws {
                    out.push_str(&format!("| {} | {} |\n", fw, count));
                }

                Ok(out)
            }
            .await,
        ),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::concurrency::{classify_tool, ToolSafetyLevel};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn test_block_schemas_count() {
        let schemas = get_schemas();
        assert_eq!(schemas.len(), 10);
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"block_search"));
        assert!(names.contains(&"block_get"));
        assert!(names.contains(&"block_insert"));
        assert!(names.contains(&"block_save"));
        assert!(names.contains(&"block_update"));
        assert!(names.contains(&"block_delete"));
        assert!(names.contains(&"block_palettes"));
        assert!(names.contains(&"block_gradients"));
        assert!(names.contains(&"block_scaffold"));
        assert!(names.contains(&"block_stats"));
    }

    #[test]
    fn test_concurrency_classification() {
        // ReadOnly tools
        assert_eq!(classify_tool("block_search"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("block_get"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("block_palettes"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("block_gradients"), ToolSafetyLevel::ReadOnly);
        assert_eq!(classify_tool("block_stats"), ToolSafetyLevel::ReadOnly);

        // Mutating tools
        assert_eq!(classify_tool("block_insert"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("block_save"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("block_update"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("block_delete"), ToolSafetyLevel::Mutating);
        assert_eq!(classify_tool("block_scaffold"), ToolSafetyLevel::Mutating);
    }

    #[tokio::test]
    async fn test_block_stats_dispatch() {
        let temp = tempdir().unwrap();
        let res = dispatch("block_stats", &json!({}), temp.path()).await;
        assert!(res.is_some());
        let output = res.unwrap().unwrap();
        assert!(output.contains("MiniBlocks UI Warehouse Statistics"));
        assert!(output.contains("Total Components:"));
    }

    #[tokio::test]
    async fn test_block_search_and_get() {
        let temp = tempdir().unwrap();
        let search_res = dispatch(
            "block_search",
            &json!({
                "category": "navbar",
                "limit": 5
            }),
            temp.path(),
        )
        .await;
        assert!(search_res.is_some());
        let search_out = search_res.unwrap().unwrap();
        assert!(search_out.contains("navbar"));

        let get_res = dispatch(
            "block_get",
            &json!({
                "name": "Simple Dark Navbar"
            }),
            temp.path(),
        )
        .await;
        assert!(get_res.is_some());
        let get_out = get_res.unwrap().unwrap();
        assert!(get_out.contains("Simple Dark Navbar"));
        assert!(get_out.contains("<nav"));
    }

    #[tokio::test]
    async fn test_block_insert_modes() {
        let temp = tempdir().unwrap();
        let test_file = "test_component.html";

        // Mode: create
        let res1 = dispatch(
            "block_insert",
            &json!({
                "target_file": test_file,
                "code": "<div>Initial Content</div>",
                "mode": "create"
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(res1.contains("Successfully inserted"));
        let content1 = std::fs::read_to_string(temp.path().join(test_file)).unwrap();
        assert_eq!(content1, "<div>Initial Content</div>");

        // Mode: append
        let _ = dispatch(
            "block_insert",
            &json!({
                "target_file": test_file,
                "code": "<div>Appended Content</div>",
                "mode": "append"
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        let content2 = std::fs::read_to_string(temp.path().join(test_file)).unwrap();
        assert!(content2.contains("Initial Content"));
        assert!(content2.contains("Appended Content"));

        // Mode: prepend
        let _ = dispatch(
            "block_insert",
            &json!({
                "target_file": test_file,
                "code": "<header>Prepended Header</header>",
                "mode": "prepend"
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        let content3 = std::fs::read_to_string(temp.path().join(test_file)).unwrap();
        assert!(content3.starts_with("<header>Prepended Header</header>"));

        // Mode: replace
        let _ = dispatch(
            "block_insert",
            &json!({
                "target_file": test_file,
                "code": "<footer>Replaced Footer</footer>",
                "mode": "replace"
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        let content4 = std::fs::read_to_string(temp.path().join(test_file)).unwrap();
        assert_eq!(content4, "<footer>Replaced Footer</footer>");
    }

    #[tokio::test]
    async fn test_block_save_update_delete_lifecycle() {
        let temp = tempdir().unwrap();

        // 1. Save
        let save_res = dispatch(
            "block_save",
            &json!({
                "name": "Custom Test Button",
                "description": "A reusable interactive button",
                "category": "button",
                "code": "<button class=\"btn-primary\">Click Me</button>",
                "framework": "tailwind",
                "dependencies": ["tailwindcss"],
                "tags": ["button", "primary", "test"]
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(save_res.contains("Custom Test Button"));
        assert!(save_res.contains("saved successfully"));

        // Extract ID
        let id_line = save_res
            .lines()
            .find(|l| l.contains("- ID: `"))
            .expect("ID line");
        let id_str = id_line.split('`').nth(1).expect("UUID between backticks");

        // 2. Update
        let update_res = dispatch(
            "block_update",
            &json!({
                "id": id_str,
                "description": "Updated button description",
                "code": "<button class=\"btn-primary active\">Clicked</button>"
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(update_res.contains("Version: 2"));
        assert!(update_res.contains("Updated button description"));

        // 3. Delete
        let delete_res = dispatch(
            "block_delete",
            &json!({
                "id": id_str
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(delete_res.contains("deleted successfully"));
    }

    #[tokio::test]
    async fn test_block_palettes_and_gradients() {
        let temp = tempdir().unwrap();

        let pal_res = dispatch(
            "block_palettes",
            &json!({
                "limit": 3
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(pal_res.contains("Found"));
        assert!(pal_res.contains("--bg:"));

        let grad_res = dispatch(
            "block_gradients",
            &json!({
                "limit": 3
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(grad_res.contains("Found"));
        assert!(grad_res.contains("background:"));
    }

    #[tokio::test]
    async fn test_block_scaffold() {
        let temp = tempdir().unwrap();
        let target_dir = temp.path().join("components");

        let scaffold_res = dispatch(
            "block_scaffold",
            &json!({
                "template_name": "landing",
                "target_dir": target_dir.to_str().unwrap()
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();

        assert!(scaffold_res.contains("Scaffolding Completed"));
        assert!(scaffold_res.contains("Files Written"));
        assert!(target_dir.join("index.html").exists());
    }

    #[tokio::test]
    async fn test_block_insert_rejects_path_traversal() {
        let temp = tempdir().unwrap();

        // 1. Relative traversal attempting to escape workspace
        let res1 = dispatch(
            "block_insert",
            &json!({
                "target_file": "../outside.html",
                "code": "<div>test</div>"
            }),
            temp.path(),
        )
        .await;
        assert!(res1.is_some());
        assert!(res1.unwrap().is_err());

        // 2. Absolute path attempting to escape workspace
        let res2 = dispatch(
            "block_insert",
            &json!({
                "target_file": "/tmp/arbitrary_file.html",
                "code": "<div>test</div>"
            }),
            temp.path(),
        )
        .await;
        assert!(res2.is_some());
        assert!(res2.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_block_insert_rejects_invalid_mode() {
        let temp = tempdir().unwrap();
        let res = dispatch(
            "block_insert",
            &json!({
                "target_file": "src/App.tsx",
                "code": "<div>test</div>",
                "mode": "invalid_mode_overwrite"
            }),
            temp.path(),
        )
        .await;
        assert!(res.is_some());
        let err = res.unwrap().unwrap_err();
        assert!(err.to_string().contains("Invalid mode"));
    }

    #[tokio::test]
    async fn test_block_scaffold_rejects_path_traversal() {
        let temp = tempdir().unwrap();
        let res = dispatch(
            "block_scaffold",
            &json!({
                "template_name": "landing",
                "target_dir": "../../../escaped_components"
            }),
            temp.path(),
        )
        .await;
        assert!(res.is_some());
        assert!(res.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_block_scaffold_rejects_unknown_template() {
        let temp = tempdir().unwrap();
        let res = dispatch(
            "block_scaffold",
            &json!({
                "template_name": "non_existent_super_template_12345",
                "target_dir": "components"
            }),
            temp.path(),
        )
        .await;
        assert!(res.is_some());
        let err = res.unwrap().unwrap_err();
        assert!(err
            .to_string()
            .contains("not found in MiniBlocks warehouse"));
    }

    #[tokio::test]
    async fn test_block_palettes_multi_format_export() {
        let temp = tempdir().unwrap();

        // 1. Default (CSS)
        let res_css = dispatch(
            "block_palettes",
            &json!({ "query": "dark", "limit": 1 }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(res_css.contains("```css"));
        assert!(res_css.contains("/* CSS Variables Export */"));
        assert!(res_css.contains("--bg:"));

        // 2. Tailwind
        let res_tw = dispatch(
            "block_palettes",
            &json!({ "query": "dark", "limit": 1, "format": "tailwind" }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(res_tw.contains("```javascript"));
        assert!(res_tw.contains("// Tailwind CSS Theme Colors"));
        assert!(res_tw.contains("bg:"));

        // 3. SCSS
        let res_scss = dispatch(
            "block_palettes",
            &json!({ "query": "dark", "limit": 1, "format": "scss" }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(res_scss.contains("```scss"));
        assert!(res_scss.contains("// SCSS Variables Export"));
        assert!(res_scss.contains("$color-bg:"));

        // 4. JSON
        let res_json = dispatch(
            "block_palettes",
            &json!({ "query": "dark", "limit": 1, "format": "json" }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(res_json.contains("```json"));
        assert!(res_json.contains("\"tokens\":"));
        assert!(res_json.contains("\"bg\":"));
    }
}
