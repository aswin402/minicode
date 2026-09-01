use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::parse_u64_param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "remember_fact".to_string(),
            description: "Save a persistent fact, convention, or developer preference to Core Memory (survives across sessions).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Unique identifier for this memory (e.g. 'code_style', 'architecture')"
                    },
                    "value": {
                        "type": "string",
                        "description": "The fact, convention, or preference to remember"
                    },
                    "is_global": {
                        "type": "boolean",
                        "description": "Whether to store globally across all projects (~/.config/minicode/memory.json) or locally (.minicode/memory.json). Default: false (local)"
                    },
                    "category": {
                        "type": "string",
                        "enum": ["preference", "project_fact", "pattern"],
                        "description": "Category of memory (default: 'project_fact')"
                    }
                },
                "required": ["key", "value"]
            }),
        },
        ToolSchema {
            name: "update_fact".to_string(),
            description: "Update an existing fact or preference in Core Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key of the memory to update"
                    },
                    "new_value": {
                        "type": "string",
                        "description": "The updated fact or preference text"
                    }
                },
                "required": ["key", "new_value"]
            }),
        },
        ToolSchema {
            name: "forget_fact".to_string(),
            description: "Remove a fact or preference from Core Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key of the memory to remove"
                    }
                },
                "required": ["key"]
            }),
        },
        ToolSchema {
            name: "create_plan".to_string(),
            description: "Initialize an active multi-step task plan in Working Memory (.minicode/plan/task_plan.md).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Short, descriptive title of the task"
                    },
                    "steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Ordered list of action steps to complete the task"
                    }
                },
                "required": ["steps"]
            }),
        },
        ToolSchema {
            name: "read_plan".to_string(),
            description: "Read the active task plan, progress tracker, and findings from Working Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "log_finding".to_string(),
            description: "Record an architectural discovery, symbol location, or observation into findings.md.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "finding": {
                        "type": "string",
                        "description": "The observation, discovery, or architectural note to record"
                    }
                },
                "required": ["finding"]
            }),
        },
        ToolSchema {
            name: "update_progress".to_string(),
            description: "Update the status of a specific task step in progress.md (e.g. 'Completed', 'Blocked', 'In Progress').".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "step": {
                        "type": "string",
                        "description": "The description of the step matching the task plan"
                    },
                    "status": {
                        "type": "string",
                        "description": "The status (e.g. 'Completed', 'In Progress', 'Blocked')"
                    }
                },
                "required": ["step"]
            }),
        },
        ToolSchema {
            name: "archive_plan".to_string(),
            description: "Archive the completed task plan and clear the active Working Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "impact_analysis".to_string(),
            description: "Analyze the architectural blast radius and downstream dependencies of modifying a symbol or file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Symbol name (e.g. 'verify_token') or relative file path (e.g. 'src/auth.rs') to analyze"
                    }
                },
                "required": ["target"]
            }),
        },
        ToolSchema {
            name: "repo_map".to_string(),
            description: "Generate a compact AST repository skeleton map of symbols ranked by PageRank importance.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "max_tokens": {
                        "type": "integer",
                        "description": "Maximum tokens to spend on repomap output (default: 1024)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "lsp_diagnostics".to_string(),
            description: "Fetch compiler and linter diagnostics across the workspace (or specific files) to check for compile errors and warnings.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "max_items": {
                        "type": "integer",
                        "description": "Maximum number of error items to display in detail (default: 8)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "lsp_goto_definition".to_string(),
            description: "Resolve the exact file path and line location where a code symbol (function, struct, type) is defined using LSP.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative file path containing the symbol usage"
                    },
                    "line": {
                        "type": "integer",
                        "description": "Line number (1-indexed) where the symbol appears"
                    },
                    "character": {
                        "type": "integer",
                        "description": "Column character offset (1-indexed) of the symbol"
                    }
                },
                "required": ["path", "line", "character"]
            }),
        },
        ToolSchema {
            name: "lsp_find_references".to_string(),
            description: "Locate all reference usages and call sites of a symbol across the workspace using LSP.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative file path containing the symbol"
                    },
                    "line": {
                        "type": "integer",
                        "description": "Line number (1-indexed)"
                    },
                    "character": {
                        "type": "integer",
                        "description": "Column character offset (1-indexed)"
                    }
                },
                "required": ["path", "line", "character"]
            }),
        },
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
        ToolSchema {
            name: "check_architecture".to_string(),
            description: "Run architectural governance sensor across the codebase to validate DAG acyclicity, detect circular dependency cycles, check layer boundaries, and compute modularity score.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "test_coverage_gaps".to_string(),
            description: "Analyze codebase AST call-graph reachability from test entrypoints to identify untested symbols, missing test coverage gaps, and composite risk scores.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_file": {
                        "type": "string",
                        "description": "Optional specific file to limit test gap analysis to (e.g. 'src/context/graph.rs')"
                    },
                    "untested_only": {
                        "type": "boolean",
                        "description": "If true, only returns symbols that have zero test reachability (default: false)"
                    },
                    "min_risk": {
                        "type": "number",
                        "description": "Minimum composite risk threshold 0.0 to 1.0 (e.g. 0.5 for high-risk only)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "code_smells".to_string(),
            description: "Run AST code smell and anti-pattern linter to detect god functions (>80 lines), excessive parameters, deep nesting, dead public exports, and complex boolean expressions.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_file": {
                        "type": "string",
                        "description": "Optional specific file to limit code smell audit to (e.g. 'src/agent/loop.rs')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "graph_visualize".to_string(),
            description: "Render visual ASCII and Unicode call-graph trees, upstream callers, downstream callees, and architectural box summaries for a symbol or file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Symbol name (e.g. 'CodeGraph', 'execute_turn') or file path (e.g. 'src/agent/loop.rs')"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["both", "upstream", "downstream", "box"],
                        "description": "Visualization mode: 'both' (callers + callees), 'upstream' (callers only), 'downstream' (callees only), 'box' (architectural card only). Default: 'both'"
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Maximum tree depth to traverse (default: 3, max: 6)"
                    }
                },
                "required": ["target"]
            }),
        },
        ToolSchema {
            name: "ast_refactor".to_string(),
            description: "Perform deterministic AST-aware refactoring actions (extract_function, rename_symbol, inline_variable) with unified diff previews.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["extract_function", "rename_symbol", "inline_variable"],
                        "description": "Refactoring action to execute"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Target file path relative to workspace root (e.g. 'src/agent/loop.rs')"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Starting line number (1-indexed, for extract_function)"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Ending line number (1-indexed, for extract_function)"
                    },
                    "new_name": {
                        "type": "string",
                        "description": "New function name (for extract_function) or replacement identifier (for rename_symbol)"
                    },
                    "target_symbol": {
                        "type": "string",
                        "description": "Target symbol to rename (for rename_symbol) or variable to inline (for inline_variable)"
                    },
                    "params": {
                        "type": "string",
                        "description": "Function parameter signature for extract_function (e.g. 'a: i32, b: &str')"
                    },
                    "call_args": {
                        "type": "string",
                        "description": "Arguments to pass at the extracted call site (e.g. 'a, b')"
                    },
                    "return_type": {
                        "type": "string",
                        "description": "Optional return type for extract_function (e.g. 'Result<()>', 'bool')"
                    },
                    "is_public": {
                        "type": "boolean",
                        "description": "Whether extracted function should be public (default: false)"
                    }
                },
                "required": ["action", "file_path"]
            }),
        },
        ToolSchema {
            name: "prune_context".to_string(),
            description: "Manually trigger observation deduplication across conversational turns to save tokens and eliminate redundant file reads.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
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
        "remember_fact" => Some((|| {
            let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "remember_fact".to_string(),
                    reason: "Missing required argument 'key'".to_string(),
                }
            })?;
            let value = args.get("value").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "remember_fact".to_string(),
                    reason: "Missing required argument 'value'".to_string(),
                }
            })?;
            let is_global = args
                .get("is_global")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let cat_str = args
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("project_fact");
            let category = match cat_str {
                "preference" => crate::context::memory::MemoryCategory::Preference,
                "pattern" => crate::context::memory::MemoryCategory::Pattern,
                _ => crate::context::memory::MemoryCategory::ProjectFact,
            };
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            mem.remember(workspace_root, key, value, is_global, category)
                .map(|_| {
                    format!(
                        "✔ Remembered '{}' ({})",
                        key,
                        if is_global { "global" } else { "local" }
                    )
                })
        })()),
        "update_fact" => Some((|| {
            let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "update_fact".to_string(),
                    reason: "Missing required argument 'key'".to_string(),
                }
            })?;
            let new_value = args
                .get("new_value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "update_fact".to_string(),
                    reason: "Missing required argument 'new_value'".to_string(),
                })?;
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            mem.update(workspace_root, key, new_value).map(|updated| {
                if updated {
                    format!("✔ Updated fact '{}'", key)
                } else {
                    format!("ℹ Fact '{}' not found to update", key)
                }
            })
        })()),
        "forget_fact" => Some((|| {
            let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "forget_fact".to_string(),
                    reason: "Missing required argument 'key'".to_string(),
                }
            })?;
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            mem.forget(workspace_root, key).map(|forgotten| {
                if forgotten {
                    format!("✔ Removed fact '{}' from memory", key)
                } else {
                    format!("ℹ Fact '{}' not found in memory", key)
                }
            })
        })()),
        "create_plan" => Some((|| {
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Task Plan");
            let steps: Vec<String> = args
                .get("steps")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "create_plan".to_string(),
                    reason: "Missing required argument 'steps'".to_string(),
                })?;
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            wm.init_plan(title, &steps).map(|_| {
                format!(
                    "✔ Created active task plan with {} steps in .minicode/plan/task_plan.md",
                    steps.len()
                )
            })
        })()),
        "read_plan" => Some({
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            match wm.read_plan() {
                Ok(Some(plan)) => Ok(plan),
                Ok(None) => Ok("ℹ No active task plan found in .minicode/plan/".to_string()),
                Err(e) => Err(e),
            }
        }),
        "log_finding" => Some((|| {
            let finding = args
                .get("finding")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "log_finding".to_string(),
                    reason: "Missing required argument 'finding'".to_string(),
                })?;
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            wm.append_finding(finding)
                .map(|_| "✔ Logged observation into .minicode/plan/findings.md".to_string())
        })()),
        "update_progress" => Some((|| {
            let step = args.get("step").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "update_progress".to_string(),
                    reason: "Missing required argument 'step'".to_string(),
                }
            })?;
            let status = args
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("Completed");
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            wm.update_progress(step, status)
                .map(|_| format!("✔ Updated step '{}' status to '{}'", step, status))
        })()),
        "archive_plan" => Some({
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            match wm.archive_plan() {
                Ok(Some(archive_path)) => Ok(format!(
                    "✔ Archived completed task plan to {}",
                    archive_path.display()
                )),
                Ok(None) => Ok("ℹ No active task plan to archive".to_string()),
                Err(e) => Err(e),
            }
        }),
        "impact_analysis" => Some((|| {
            let target = args.get("target").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "impact_analysis".to_string(),
                    reason: "Missing required argument 'target'".to_string(),
                }
            })?;
            let mut graph = crate::context::graph::CodeGraph::new();
            graph.build_graph(workspace_root)?;
            let report = graph.get_blast_radius(target, workspace_root)?;
            Ok(report.summary)
        })()),
        "repo_map" => Some((|| {
            let max_tokens = parse_u64_param(args.get("max_tokens"))
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(crate::constants::DEFAULT_MAP_TOKENS);
            let mut graph = crate::context::graph::CodeGraph::new();
            graph.build_graph(workspace_root)?;
            Ok(graph.format_repomap(workspace_root, &[], max_tokens))
        })()),
        "lsp_diagnostics" => Some({
            let max_items = parse_u64_param(args.get("max_items")).unwrap_or(8) as usize;
            match crate::lsp::LspEngine::run_diagnostics(workspace_root).await {
                Ok(report) => Ok(report.format_for_agent(workspace_root, max_items)),
                Err(e) => Err(e),
            }
        }),
        "lsp_goto_definition" => Some({
            let path = match args.get("path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => {
                    return Some(Err(ToolError::InvalidArguments {
                        name: "lsp_goto_definition".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                    .into()));
                }
            };
            let line = parse_u64_param(args.get("line")).unwrap_or(1) as u32;
            let character = parse_u64_param(args.get("character")).unwrap_or(1) as u32;

            let lsp_line = line.saturating_sub(1);
            let lsp_col = character.saturating_sub(1);

            match crate::lsp::LspEngine::goto_definition(
                workspace_root,
                Path::new(path),
                lsp_line,
                lsp_col,
            )
            .await
            {
                Ok(locations) => {
                    if locations.is_empty() {
                        Ok(format!(
                            "ℹ No definition found for '{}:{}:{}' via LSP",
                            path, line, character
                        ))
                    } else {
                        let mut out =
                            format!("✔ Found {} definition location(s):\n", locations.len());
                        for loc in locations {
                            let rel = loc
                                .file_path
                                .strip_prefix(workspace_root)
                                .unwrap_or(&loc.file_path);
                            out.push_str(&format!(
                                "  • {}:{}:{}\n",
                                rel.display(),
                                loc.line + 1,
                                loc.character + 1
                            ));
                        }
                        Ok(out)
                    }
                }
                Err(e) => Err(e),
            }
        }),
        "lsp_find_references" => Some({
            let path = match args.get("path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => {
                    return Some(Err(ToolError::InvalidArguments {
                        name: "lsp_find_references".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                    .into()));
                }
            };
            let line = parse_u64_param(args.get("line")).unwrap_or(1) as u32;
            let character = parse_u64_param(args.get("character")).unwrap_or(1) as u32;

            let lsp_line = line.saturating_sub(1);
            let lsp_col = character.saturating_sub(1);

            match crate::lsp::LspEngine::find_references(
                workspace_root,
                Path::new(path),
                lsp_line,
                lsp_col,
            )
            .await
            {
                Ok(locations) => {
                    if locations.is_empty() {
                        Ok(format!(
                            "ℹ No references found for '{}:{}:{}' via LSP",
                            path, line, character
                        ))
                    } else {
                        let mut out = format!("✔ Found {} reference usage(s):\n", locations.len());
                        for loc in locations {
                            let rel = loc
                                .file_path
                                .strip_prefix(workspace_root)
                                .unwrap_or(&loc.file_path);
                            out.push_str(&format!(
                                "  • {}:{}:{}\n",
                                rel.display(),
                                loc.line + 1,
                                loc.character + 1
                            ));
                        }
                        Ok(out)
                    }
                }
                Err(e) => Err(e),
            }
        }),
        "wiki_write" => Some((|| {
            let topic = args["topic"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "wiki_write".to_string(),
                    reason: "Missing 'topic'".to_string(),
                })?;
            let title = args["title"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "wiki_write".to_string(),
                    reason: "Missing 'title'".to_string(),
                })?;
            let content = args["content"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "wiki_write".to_string(),
                    reason: "Missing 'content'".to_string(),
                })?;
            let tags: Vec<String> = args["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let references: Vec<String> = args["references"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

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
            let topic = args["topic"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "wiki_read".to_string(),
                    reason: "Missing 'topic'".to_string(),
                })?;
            let content = crate::context::wiki::WikiManager::read_entry(workspace_root, topic)?;
            Ok(content)
        })()),
        "wiki_search" => Some((|| {
            let query = args["query"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "wiki_search".to_string(),
                    reason: "Missing 'query'".to_string(),
                })?;
            let results = crate::context::wiki::WikiManager::search_entries(workspace_root, query)?;
            Ok(results)
        })()),
        "create_skill" => Some((|| {
            let name = args["name"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "create_skill".to_string(),
                    reason: "Missing 'name'".to_string(),
                })?;
            let description =
                args["description"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "create_skill".to_string(),
                        reason: "Missing 'description'".to_string(),
                    })?;
            let instructions =
                args["instructions"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "create_skill".to_string(),
                        reason: "Missing 'instructions'".to_string(),
                    })?;
            let allowed_tools: Vec<String> = args["allowed_tools"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

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
            let name = args["name"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "inspect_skill".to_string(),
                    reason: "Missing 'name'".to_string(),
                })?;
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
        "check_architecture" => Some((|| {
            let report =
                crate::context::governance::ArchitectureGovernor::scan_workspace(workspace_root)?;
            Ok(report.format_markdown())
        })()),
        "test_coverage_gaps" => Some((|| {
            let target_file = args.get("target_file").and_then(|v| v.as_str());
            let untested_only = args
                .get("untested_only")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let min_risk = args.get("min_risk").and_then(|v| v.as_f64());

            let mut graph = crate::context::graph::CodeGraph::new();
            let _ = graph.build_graph(workspace_root);

            let report = crate::context::test_gap::TestGapAnalyzer::analyze(
                workspace_root,
                &graph,
                target_file,
                untested_only,
                min_risk,
            )?;

            let markdown =
                crate::context::test_gap::TestGapAnalyzer::format_markdown(&report, target_file);
            Ok(markdown)
        })()),
        "code_smells" => Some((|| {
            let target_file = args.get("target_file").and_then(|v| v.as_str());

            let mut graph = crate::context::graph::CodeGraph::new();
            let _ = graph.build_graph(workspace_root);

            let report = crate::context::smell_detector::AstSmellDetector::scan_workspace(
                workspace_root,
                Some(&graph),
                target_file,
            )?;

            let markdown = crate::context::smell_detector::AstSmellDetector::format_markdown(
                &report,
                target_file,
            );
            Ok(markdown)
        })()),
        "graph_visualize" => Some((|| {
            let target = args.get("target").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "graph_visualize".to_string(),
                    reason: "Missing required argument 'target'".to_string(),
                }
            })?;
            let mode_str = args.get("mode").and_then(|v| v.as_str()).unwrap_or("both");
            let max_depth = parse_u64_param(args.get("max_depth"))
                .map(|v| (v as usize).clamp(1, 6))
                .unwrap_or(3);

            let mode = crate::context::graph_visualizer::VisualizeMode::from_str(mode_str);

            let mut graph = crate::context::graph::CodeGraph::new();
            let _ = graph.build_graph(workspace_root);

            let diagram = crate::context::graph_visualizer::GraphVisualizer::render(
                workspace_root,
                &graph,
                target,
                mode,
                max_depth,
            )?;
            Ok(diagram)
        })()),
        "ast_refactor" => Some((|| {
            let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "ast_refactor".to_string(),
                    reason: "Missing required argument 'action'".to_string(),
                }
            })?;
            let file_path = args
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "ast_refactor".to_string(),
                    reason: "Missing required argument 'file_path'".to_string(),
                })?;

            match action {
                "extract_function" => {
                    let start_line = parse_u64_param(args.get("start_line"))
                        .map(|v| v as usize)
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "ast_refactor".to_string(),
                            reason: "Missing 'start_line'".to_string(),
                        })?;
                    let end_line = parse_u64_param(args.get("end_line"))
                        .map(|v| v as usize)
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "ast_refactor".to_string(),
                            reason: "Missing 'end_line'".to_string(),
                        })?;
                    let new_fn_name = args
                        .get("new_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("extracted_helper");
                    let params = args.get("params").and_then(|v| v.as_str()).unwrap_or("");
                    let call_args = args.get("call_args").and_then(|v| v.as_str()).unwrap_or("");
                    let return_type = args.get("return_type").and_then(|v| v.as_str());
                    let is_public = args
                        .get("is_public")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let res = crate::context::ast_refactor::AstRefactorer::extract_function(
                        workspace_root,
                        file_path,
                        start_line,
                        end_line,
                        new_fn_name,
                        params,
                        call_args,
                        return_type,
                        is_public,
                    )?;
                    Ok(format!(
                        "✔ Refactored `{}`:\n```diff\n{}\n```",
                        res.action, res.diff_preview
                    ))
                }
                "rename_symbol" => {
                    let target_symbol = args
                        .get("target_symbol")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "ast_refactor".to_string(),
                            reason: "Missing 'target_symbol'".to_string(),
                        })?;
                    let new_name =
                        args.get("new_name")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| ToolError::InvalidArguments {
                                name: "ast_refactor".to_string(),
                                reason: "Missing 'new_name'".to_string(),
                            })?;

                    let res = crate::context::ast_refactor::AstRefactorer::rename_symbol(
                        workspace_root,
                        target_symbol,
                        new_name,
                        Some(file_path),
                    )?;
                    Ok(format!(
                        "✔ Refactored `{}` across {} file(s):\n```diff\n{}\n```",
                        res.action,
                        res.files_modified.len(),
                        res.diff_preview
                    ))
                }
                "inline_variable" => {
                    let var_name = args
                        .get("target_symbol")
                        .or_else(|| args.get("new_name"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "ast_refactor".to_string(),
                            reason: "Missing 'target_symbol' (variable name)".to_string(),
                        })?;

                    let res = crate::context::ast_refactor::AstRefactorer::inline_variable(
                        workspace_root,
                        file_path,
                        var_name,
                    )?;
                    Ok(format!(
                        "✔ Refactored `{}`:\n```diff\n{}\n```",
                        res.action, res.diff_preview
                    ))
                }
                other => Err(ToolError::InvalidArguments {
                    name: "ast_refactor".to_string(),
                    reason: format!("Unknown refactoring action: '{}'", other),
                }
                .into()),
            }
        })()),
        "prune_context" => Some(Ok(
            "✔ Multi-turn observation deduplication and pruning applied.".to_string(),
        )),
        _ => None,
    }
}
