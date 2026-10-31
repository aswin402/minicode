use crate::agent::provider::ToolSchema;
use crate::context::okf::{
    attest_concept_doc, bind_parameters, extract_computation_text, generate_visualization,
    read_existing_doc, regenerate_indexes, validate_bundle_conformance, OkfAttester, OkfExecutor,
    OkfParameter,
};
use crate::context::wiki::{WikiManager, WikiOkfOptions};
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "wiki_write".to_string(),
            description: "Write or update a persistent OKF v0.2 Markdown knowledge concept in the repository knowledge bundle (.minicode/wiki/<concept_id>.md or custom bundle_dir) with provenance sources, trust verification, staleness, schema augmentation guard, and optional Attested Computation contract.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Concept ID or topic slug (supports hierarchical paths like 'tables/events_', 'computations/revenue', or 'architecture-database')"
                    },
                    "title": {
                        "type": "string",
                        "description": "Human-readable title of the concept document"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full Markdown body (including optional # Schema, # Computation, or # Common query patterns sections)"
                    },
                    "concept_type": {
                        "type": "string",
                        "description": "OKF v0.2 concept type (e.g. 'Concept', 'PRD', 'Architecture', 'BigQuery Table', 'Metric', 'Attested Computation', 'Reference'). Defaults to 'Concept'."
                    },
                    "description": {
                        "type": "string",
                        "description": "Concise one-sentence summary for progressive disclosure in index.md"
                    },
                    "resource": {
                        "type": "string",
                        "description": "Canonical URI or file path this concept describes"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of search tags and categorization keywords"
                    },
                    "references": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Provenance source URIs or file paths (stored as OKF v0.2 `sources`)"
                    },
                    "status": {
                        "type": "string",
                        "enum": ["draft", "stable", "deprecated"],
                        "description": "OKF v0.2 lifecycle status (default: 'stable')"
                    },
                    "stale_after": {
                        "type": "string",
                        "description": "Optional ISO 8601 UTC timestamp (e.g. '2026-12-31T00:00:00Z') on/after which content is stale"
                    },
                    "verified_by": {
                        "type": "string",
                        "description": "Optional verifier actor ID (e.g. 'human:alice' for human-reviewed or 'process:ci' for machine-confirmed)"
                    },
                    "runtime": {
                        "type": "string",
                        "description": "Required when concept_type is 'Attested Computation' (e.g. 'bigquery', 'postgres', 'dbt', 'python', 'rust')"
                    },
                    "computation": {
                        "type": "string",
                        "description": "Optional relative path to computation file for 'Attested Computation' (omit when using inline `# Computation` block)"
                    },
                    "parameters": {
                        "type": "array",
                        "description": "Typed parameter holes for 'Attested Computation': [{ name, type, required }]",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "type": { "type": "string" },
                                "required": { "type": "boolean" }
                            },
                            "required": ["name", "type"]
                        }
                    },
                    "augment": {
                        "type": "boolean",
                        "description": "When true, enforces the OKF v0.2 Augmentation Guard (refuses overwrites that drop existing `# Schema` fields or shrink `sources`)"
                    },
                    "bundle_dir": {
                        "type": "string",
                        "description": "Optional relative bundle root directory (defaults to '.minicode/wiki'; e.g. 'minikit_docs')"
                    }
                },
                "required": ["topic", "title", "content"]
            }),
        },
        ToolSchema {
            name: "wiki_read".to_string(),
            description: "Read a specific OKF v0.2 knowledge concept document by topic or concept_id (e.g. 'architecture-overview' or 'tables/events_'), surfacing its derived Trust Tier (unverified / machine-confirmed / human-reviewed) and Freshness status.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Topic slug or hierarchical concept_id to read"
                    },
                    "bundle_dir": {
                        "type": "string",
                        "description": "Optional relative bundle directory (defaults to '.minicode/wiki')"
                    }
                },
                "required": ["topic"]
            }),
        },
        ToolSchema {
            name: "wiki_search".to_string(),
            description: "Search across repository knowledge wiki & OKF concept documents matching concept_id, title, tags, or content keywords.".to_string(),
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
            name: "okf_attest".to_string(),
            description: "Bind typed parameters or deterministically attest a runtime execution receipt against an OKF v0.2 `Attested Computation` concept (verifying SQL/code provenance and result fidelity without an LLM).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "concept_id": {
                        "type": "string",
                        "description": "Concept ID of the Attested Computation (e.g. 'computations/revenue')"
                    },
                    "parameters": {
                        "type": "object",
                        "description": "Optional parameter bindings map (e.g. {\"year\": 2026})"
                    },
                    "receipt": {
                        "type": "object",
                        "description": "Execution receipt object containing `executed_sql` (or `compiled_sql`), `result`, and optional `job_id`/`run_id`"
                    },
                    "claimed_value": {
                        "description": "The scalar value about to be reported to the user, verified against the first cell of `receipt.result`"
                    },
                    "bundle_dir": {
                        "type": "string",
                        "description": "Optional relative bundle directory (defaults to '.minicode/wiki')"
                    }
                },
                "required": ["concept_id"]
            }),
        },
        ToolSchema {
            name: "okf_bundle".to_string(),
            description: "Perform OKF v0.2 bundle-wide operations: generate a self-contained interactive Cytoscape HTML graph viewer (`viz.html`), validate bundle conformance (§11), or regenerate recursive bottom-up `index.md` files.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["visualize", "validate", "index"],
                        "description": "Operation to perform on the OKF bundle"
                    },
                    "bundle_dir": {
                        "type": "string",
                        "description": "Relative path to the bundle root directory (defaults to 'minikit_docs' if present, else '.minicode/wiki')"
                    },
                    "out_path": {
                        "type": "string",
                        "description": "Optional output HTML path for `action=\"visualize\"` (defaults to `<bundle_dir>/viz.html`)"
                    },
                    "bundle_name": {
                        "type": "string",
                        "description": "Optional display name for the viewer header"
                    }
                },
                "required": ["action"]
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

fn resolve_bundle_dir(workspace_root: &Path, args: &Value, default_to_wiki: bool) -> PathBuf {
    if let Some(rel) = param::opt_str(args, "bundle_dir") {
        let clean = rel.trim().trim_start_matches('/');
        if !clean.is_empty() && !clean.contains("..") {
            return workspace_root.join(clean);
        }
    }
    if default_to_wiki {
        WikiManager::wiki_dir(workspace_root)
    } else {
        let minikit_docs = crate::tools::minikit::resolve_docs_dir(workspace_root);
        if minikit_docs.exists() {
            minikit_docs
        } else {
            WikiManager::wiki_dir(workspace_root)
        }
    }
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

            let parameters: Vec<OkfParameter> = args
                .get("parameters")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let name = item.get("name")?.as_str()?.to_string();
                            let param_type = item
                                .get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("string")
                                .to_string();
                            let required = item
                                .get("required")
                                .and_then(|b| b.as_bool())
                                .unwrap_or(true);
                            Some(OkfParameter {
                                name,
                                param_type,
                                required,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let executor = param::opt_str(args, "executor_resource").map(|res| OkfExecutor {
                resource: res.to_string(),
                receipt: param::opt_string_array(args, "executor_receipt").unwrap_or_default(),
            });
            let attester = param::opt_str(args, "attester_resource").map(|res| OkfAttester {
                resource: res.to_string(),
            });

            let opts = WikiOkfOptions {
                concept_type: param::opt_str(args, "concept_type").map(|s| s.to_string()),
                description: param::opt_str(args, "description").map(|s| s.to_string()),
                resource: param::opt_str(args, "resource").map(|s| s.to_string()),
                status: param::opt_str(args, "status").map(|s| s.to_string()),
                stale_after: param::opt_str(args, "stale_after").map(|s| s.to_string()),
                verified_by: param::opt_str(args, "verified_by").map(|s| s.to_string()),
                runtime: param::opt_str(args, "runtime").map(|s| s.to_string()),
                computation: param::opt_str(args, "computation").map(|s| s.to_string()),
                parameters,
                executor,
                attester,
                enforce_augmentation_guard: param::opt_bool(args, "augment", false),
                bundle_dir: param::opt_str(args, "bundle_dir")
                    .map(|_| resolve_bundle_dir(workspace_root, args, true)),
            };

            WikiManager::write_entry_with_okf(
                workspace_root,
                topic,
                title,
                content,
                &tags,
                &references,
                opts,
            )
        })()),
        "wiki_read" => Some((|| {
            let topic = param::require_str(args, "topic", "wiki_read")?;
            let bundle_dir = resolve_bundle_dir(workspace_root, args, true);
            WikiManager::read_entry_from_bundle(workspace_root, topic, Some(&bundle_dir))
        })()),
        "wiki_search" => Some((|| {
            let query = param::require_str(args, "query", "wiki_search")?;
            WikiManager::search_entries(workspace_root, query)
        })()),
        "okf_attest" => Some((|| {
            let concept_id = param::require_str(args, "concept_id", "okf_attest")?;
            let bundle_dir = resolve_bundle_dir(workspace_root, args, true);
            let param_map = args.get("parameters").and_then(|v| v.as_object());

            if let Some(receipt) = args.get("receipt") {
                let claimed = args.get("claimed_value");
                let verdict =
                    attest_concept_doc(&bundle_dir, concept_id, receipt, claimed, param_map)?;
                let status_icon = if verdict.ok { "✔ PASS" } else { "✖ FAIL" };
                let out = serde_json::to_string_pretty(&verdict).unwrap_or_default();
                Ok(format!(
                    "{} — OKF v0.2 Attestation Verdict for `{}`:\n```json\n{}\n```",
                    status_icon, concept_id, out
                ))
            } else {
                // Parameter binding mode: load sanctioned computation and bind parameters
                let Some((fm, body)) = read_existing_doc(&bundle_dir, concept_id)? else {
                    return Err(ToolError::NotFound {
                        name: format!("okf:{}", concept_id),
                    }
                    .into());
                };
                let raw_comp = extract_computation_text(&bundle_dir, &fm, &body)?;
                let runtime = fm.runtime.as_deref().unwrap_or("sql");
                let empty_map = serde_json::Map::new();
                let bound = bind_parameters(
                    &raw_comp,
                    runtime,
                    &fm.parameters,
                    param_map.unwrap_or(&empty_map),
                )?;
                Ok(format!(
                    "✔ Bound Sanctioned Computation for `{}` (runtime: `{}`):\n```{}\n{}\n```",
                    concept_id, runtime, runtime, bound
                ))
            }
        })()),
        "okf_bundle" => Some((|| {
            let action = param::require_str(args, "action", "okf_bundle")?;
            let bundle_dir = resolve_bundle_dir(workspace_root, args, false);
            match action {
                "visualize" => {
                    let out_path = param::opt_str(args, "out_path")
                        .map(|p| workspace_root.join(p.trim_start_matches('/')));
                    let bundle_name = param::opt_str(args, "bundle_name");
                    let stats =
                        generate_visualization(&bundle_dir, out_path.as_deref(), bundle_name)?;
                    Ok(format!(
                        "✔ Generated OKF v0.2 interactive HTML graph viewer:\n• Concepts: {}\n• Cross-Link Edges: {}\n• Size: {} bytes\n• Output: `{}`",
                        stats.concepts, stats.edges, stats.bytes, stats.output_path
                    ))
                }
                "validate" => {
                    let report = validate_bundle_conformance(&bundle_dir)?;
                    let json_str = serde_json::to_string_pretty(&report).unwrap_or_default();
                    let icon = if report.conformant { "✔" } else { "!" };
                    Ok(format!(
                        "{} OKF v0.2 Conformance Report for `{}`:\n```json\n{}\n```",
                        icon,
                        bundle_dir.display(),
                        json_str
                    ))
                }
                "index" => {
                    let written = regenerate_indexes(&bundle_dir, true)?;
                    Ok(format!(
                        "✔ Regenerated {} recursive OKF v0.2 `index.md` file(s) under `{}`.",
                        written.len(),
                        bundle_dir.display()
                    ))
                }
                other => Err(ToolError::InvalidArguments {
                    name: "okf_bundle".to_string(),
                    reason: format!(
                        "Unknown action '{}'. Expected 'visualize', 'validate', or 'index'.",
                        other
                    ),
                }
                .into()),
            }
        })()),
        "create_skill" => Some((|| {
            let name = param::require_str(args, "name", "create_skill")?;
            let description = param::require_str(args, "description", "create_skill")?;
            let instructions = param::require_str(args, "instructions", "create_skill")?;
            let allowed_tools = param::opt_string_array(args, "allowed_tools").unwrap_or_default();

            crate::context::skill_forge::SkillForge::create_skill(
                workspace_root,
                name,
                description,
                instructions,
                &allowed_tools,
            )
        })()),
        "list_skills" => Some((|| {
            crate::context::skill_forge::SkillForge::list_all_skills(workspace_root)
        })()),
        "inspect_skill" => Some((|| {
            let name = param::require_str(args, "name", "inspect_skill")?;
            let skill =
                crate::context::skill_forge::SkillForge::inspect_skill(workspace_root, name)?;
            Ok(format!(
                "🛠️ Skill: **{}**\n📁 Path: `{}`\n📝 Description: _{}_\n\n## Instructions\n\n{}",
                skill.name,
                skill.path.display(),
                skill.description,
                skill.instructions
            ))
        })()),
        _ => None,
    }
}
