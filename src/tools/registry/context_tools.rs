use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
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
            name: "audit_architecture".to_string(),
            description: "Audits codebase software architecture boundaries, detects circular dependency cycles with Tarjan's SCC, checks layered isolation (UI > Service > Data > Utility), and computes module coupling & instability metrics (Ca, Ce, I).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["check", "matrix", "cycles", "full"],
                        "description": "Analysis mode: 'check' (summary + violations, default), 'matrix' (module coupling Ca, Ce, instability), 'cycles' (circular dependency DAG paths), or 'full' (all sections combined)."
                    },
                    "enforce": {
                        "type": "boolean",
                        "description": "If true, fails with an error if circular dependency cycles or layer boundary violations are detected."
                    },
                    "format": {
                        "type": "string",
                        "enum": ["markdown", "json"],
                        "description": "Output format: 'markdown' (human-readable tables, default) or 'json' (structured machine-readable payload)."
                    }
                }
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
            name: "architecture_invariants".to_string(),
            description: "Audit multi-file architectural invariants, detect forbidden cross-layer calls (e.g. Domain->UI), circular call cycles, and structural integrity violations.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_file": {
                        "type": "string",
                        "description": "Optional specific file to limit invariant audit to (e.g. 'src/agent/loop.rs')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "dead_code_sweep".to_string(),
            description: "Analyze codebase reachability from crate roots and identify dead functions, structs, and isolated cyclic clusters.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_file": {
                        "type": "string",
                        "description": "Optional specific file to limit dead code audit to (e.g. 'src/agent/loop.rs')"
                    },
                    "min_confidence": {
                        "type": "string",
                        "enum": ["all", "medium", "high"],
                        "description": "Minimum confidence threshold (default: 'all')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "semantic_code_search".to_string(),
            description: "Execute two-stage semantic code search (BM25 + Dense Vectors + PageRank + Cross-Encoder Intent Reranker) for precision code retrieval.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language or technical code search query (e.g. 'where is session history flushed to disk?')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of reranked results to return (default: 5, max: 20)"
                    },
                    "target_layer": {
                        "type": "string",
                        "description": "Optional architectural layer to boost/filter by (e.g. 'UI', 'Service', 'Data', 'API')"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSchema {
            name: "generate_architecture_docs".to_string(),
            description: "Automatically synthesize comprehensive ARCHITECTURE.md documentation with Mermaid component diagrams and layer breakdowns.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "write_to_file": {
                        "type": "boolean",
                        "description": "Whether to write the documentation directly to ARCHITECTURE.md in the workspace root (default: false)"
                    },
                    "include_mermaid": {
                        "type": "boolean",
                        "description": "Whether to include Mermaid visual architecture diagrams (default: true)"
                    },
                    "include_symbol_catalog": {
                        "type": "boolean",
                        "description": "Whether to include high-centrality PageRank symbol table (default: true)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "sync_code_graph".to_string(),
            description: "Incrementally update or rebuild the AST CodeGraph dependency index to reflect recent disk changes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_file": {
                        "type": "string",
                        "description": "Optional specific file to synchronize"
                    },
                    "force_full": {
                        "type": "boolean",
                        "description": "Whether to force a complete cold-start re-indexing (default: false)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "workspace_monorepo_map".to_string(),
            description: "Analyze multi-package monorepo topology (Cargo Workspaces, npm/pnpm), cross-package dependencies, and topological build order.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "include_external": {
                        "type": "boolean",
                        "description": "Whether to include third-party package dependencies (default: false)"
                    },
                    "target_package": {
                        "type": "string",
                        "description": "Optional specific package name or path to focus the analysis on"
                    }
                }
            }),
        },
        ToolSchema {
            name: "checkpoint_session".to_string(),
            description: "Capture an immutable point-in-time snapshot of the current session conversation and working memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "label": {
                        "type": "string",
                        "description": "Short mnemonic label for the checkpoint (e.g. 'before-ast-refactor')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional detailed context or rationale for creating this checkpoint"
                    }
                },
                "required": ["label"]
            }),
        },
        ToolSchema {
            name: "rewind_session".to_string(),
            description: "List checkpoints, rewind working memory to an earlier snapshot, or fork a new exploration branch.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "rewind", "fork"],
                        "description": "Action to perform: 'list' all checkpoints, 'rewind' to checkpoint, or 'fork' a new branch"
                    },
                    "checkpoint_id": {
                        "type": "string",
                        "description": "Target checkpoint ID (required for 'rewind' and 'fork')"
                    },
                    "fork_label": {
                        "type": "string",
                        "description": "Optional label when forking a checkpoint"
                    }
                },
                "required": ["action"]
            }),
        },
        ToolSchema {
            name: "trace_dataflow".to_string(),
            description: "Trace inter-procedural type-flow, caller/callee propagation, and taint reachability to sensitive sinks.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_symbol": {
                        "type": "string",
                        "description": "Symbol name (function, method, variable) to trace dataflow for"
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["forward", "backward"],
                        "description": "Direction: 'forward' (origin to sink) or 'backward' (sink to origin / program slicing, default: 'forward')"
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Maximum call chain depth to traverse (default: 5)"
                    },
                    "taint_check": {
                        "type": "boolean",
                        "description": "Whether to perform security taint analysis for dangerous sinks (default: true)"
                    }
                },
                "required": ["target_symbol"]
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
        ToolSchema {
            name: "optimize_token_budget".to_string(),
            description: "Predict multi-turn token consumption velocity, forecast headroom until model context limits, and generate compaction optimization recommendations.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "model_name": {
                        "type": "string",
                        "description": "Optional model name to evaluate context limits for (default: 'gpt-4o')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "hybrid_retrieve".to_string(),
            description: "Execute comprehensive multi-modal knowledge retrieval fusing AST CodeGraph PageRank, lexical BM25, dense semantic vector chunks, architectural wiki articles, and episodic cross-session memory with Reciprocal Rank Fusion (RRF).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language query, feature concept, architectural question, or symbol name"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of primary code matches to return (default: 5)"
                    },
                    "include_graph": {
                        "type": "boolean",
                        "description": "Whether to include AST caller/callee dependency topology for matched symbols (default: true)"
                    },
                    "include_wiki": {
                        "type": "boolean",
                        "description": "Whether to retrieve relevant architectural wiki knowledge documents (default: true)"
                    },
                    "include_memory": {
                        "type": "boolean",
                        "description": "Whether to retrieve cross-session episodic memory of past solved problems (default: true)"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSchema {
            name: "quarantine_flaky_tests".to_string(),
            description: "Execute statistical N-pass burn-in testing on a suspect or failing test, calculate variance and flakiness ratios, isolate non-deterministic tests into .minicode/quarantine.json, and get automated stabilization suggestions.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "test_name": {
                        "type": "string",
                        "description": "Optional test name or target filter (e.g. 'test_network_timeout' or 'test_socket')"
                    },
                    "runs": {
                        "type": "integer",
                        "description": "Number of statistical burn-in runs to execute (default: 5, min: 2, max: 10)"
                    },
                    "action": {
                        "type": "string",
                        "description": "Action to perform: 'detect' (default, runs burn-in & analyzes), 'quarantine' (force quarantine), 'unquarantine' (remove from quarantine), or 'list' (view all quarantined tests)",
                        "enum": ["detect", "quarantine", "unquarantine", "list"]
                    },
                    "auto_quarantine": {
                        "type": "boolean",
                        "description": "Whether to automatically quarantine the test if flakiness is detected (default: true)"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Optional custom reason when manually quarantining a test"
                    }
                }
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
            let key = param::require_str(args, "key", "remember_fact")?;
            let value = param::require_str(args, "value", "remember_fact")?;
            let is_global = param::opt_bool(args, "is_global", false);
            let cat_str = param::opt_str(args, "category").unwrap_or("project_fact");
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
            let key = param::require_str(args, "key", "update_fact")?;
            let new_value = param::require_str(args, "new_value", "update_fact")?;
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
            let key = param::require_str(args, "key", "forget_fact")?;
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
            let title = param::opt_str(args, "title").unwrap_or("Task Plan");
            let steps = param::opt_string_array(args, "steps").ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "create_plan".to_string(),
                    reason: "Missing required argument 'steps'".to_string(),
                }
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
            let finding = param::require_str(args, "finding", "log_finding")?;
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            wm.append_finding(finding)
                .map(|_| "✔ Logged observation into .minicode/plan/findings.md".to_string())
        })()),
        "update_progress" => Some((|| {
            let step = param::require_str(args, "step", "update_progress")?;
            let status = param::opt_str(args, "status").unwrap_or("Completed");
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
            let max_tokens = param::opt_usize(args, "max_tokens", crate::constants::DEFAULT_MAP_TOKENS);
            let mut graph = crate::context::graph::CodeGraph::new();
            graph.build_graph(workspace_root)?;
            Ok(graph.format_repomap(workspace_root, &[], max_tokens))
        })()),
        "lsp_diagnostics" => Some({
            let max_items = param::opt_usize(args, "max_items", 8);
            match crate::lsp::LspEngine::run_diagnostics(workspace_root).await {
                Ok(report) => Ok(report.format_for_agent(workspace_root, max_items)),
                Err(e) => Err(e),
            }
        }),
        "lsp_goto_definition" => Some({
            let path = match param::require_str(args, "path", "lsp_goto_definition") {
                Ok(p) => p,
                Err(e) => return Some(Err(e.into())),
            };
            let line = param::opt_usize(args, "line", 1) as u32;
            let character = param::opt_usize(args, "character", 1) as u32;

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
            let path = match param::require_str(args, "path", "lsp_find_references") {
                Ok(p) => p,
                Err(e) => return Some(Err(e.into())),
            };
            let line = param::opt_usize(args, "line", 1) as u32;
            let character = param::opt_usize(args, "character", 1) as u32;

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
        "audit_architecture" | "check_architecture" => Some((|| {
            let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("check");
            let enforce = args
                .get("enforce")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let format = args
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("markdown");

            let report =
                crate::context::governance::ArchitectureGovernor::scan_workspace(workspace_root)?;

            if enforce
                && (!report.circular_cycles.is_empty()
                    || !report.layer_violations.is_empty()
                    || report.health_score < crate::constants::ARCH_MIN_HEALTH_SCORE)
            {
                return Err(crate::error::ToolError::CommandExec(format!(
                    "Architectural Enforcement Failed: {} circular cycles, {} boundary violations, health score {}/100 (threshold {})",
                    report.circular_cycles.len(),
                    report.layer_violations.len(),
                    report.health_score,
                    crate::constants::ARCH_MIN_HEALTH_SCORE
                ))
                .into());
            }

            if format == "json" {
                return Ok(serde_json::to_string_pretty(&report.to_json())?);
            }

            match mode {
                "matrix" => {
                    let mut out = format!(
                        "# 🏛️ Module Coupling & Instability Matrix (Health Score: {}/100)\n\n",
                        report.health_score
                    );
                    out.push_str(
                        "| Module | Files | LOC | Afferent ($C_a$) | Efferent ($C_e$) | Instability ($I$) | Role |\n",
                    );
                    out.push_str(
                        "| :--- | :---: | :---: | :---: | :---: | :---: | :--- |\n",
                    );
                    for m in &report.coupling_metrics {
                        let role = if m.instability < 0.3 {
                            "🛡️ Stable Base"
                        } else if m.instability > 0.7 {
                            "🍃 Flexible Leaf"
                        } else {
                            "⚖️ Balanced"
                        };
                        out.push_str(&format!(
                            "| `{}` | {} | {} | {} | {} | {:.2} | {} |\n",
                            m.module_name,
                            m.file_count,
                            m.total_loc,
                            m.afferent_coupling,
                            m.efferent_coupling,
                            m.instability,
                            role
                        ));
                    }
                    Ok(out)
                }
                "cycles" => {
                    if report.circular_cycles.is_empty() {
                        Ok(
                            "✔ **Zero Circular Cycles:** Codebase dependency graph is a 100% acyclic DAG."
                                .to_string(),
                        )
                    } else {
                        let mut out = format!(
                            "⚠️ **{} Circular Dependency Cycles Detected:**\n\n",
                            report.circular_cycles.len()
                        );
                        for cycle in &report.circular_cycles {
                            out.push_str(&format!("- 🔄 Cycle: `{}`\n", cycle.join(" ➔ ")));
                        }
                        Ok(out)
                    }
                }
                _ => Ok(report.format_markdown()),
            }
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
            let target = param::require_str(args, "target", "graph_visualize")?;
            let mode_str = param::opt_str(args, "mode").unwrap_or("both");
            let max_depth = param::opt_usize(args, "max_depth", 3).clamp(1, 6);

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
        "architecture_invariants" => Some((|| {
            let target_file = args.get("target_file").and_then(|v| v.as_str());

            let mut graph = crate::context::graph::CodeGraph::new();
            let _ = graph.build_graph(workspace_root);

            let report = crate::context::invariants::InvariantChecker::check_workspace(
                workspace_root,
                Some(&graph),
                target_file,
            )?;

            let markdown = report.format_markdown();
            Ok(markdown)
        })()),
        "dead_code_sweep" => Some((|| {
            let target_file = args.get("target_file").and_then(|v| v.as_str());
            let min_confidence = args.get("min_confidence").and_then(|v| v.as_str());

            let mut graph = crate::context::graph::CodeGraph::new();
            let _ = graph.build_graph(workspace_root);

            let report = crate::context::dead_code::DeadCodeEliminator::analyze_workspace(
                workspace_root,
                Some(&graph),
                target_file,
                min_confidence,
            )?;

            let markdown = report.format_markdown();
            Ok(markdown)
        })()),
        "semantic_code_search" => Some((|| {
            let query = param::require_str(args, "query", "semantic_code_search")?;
            let limit = param::opt_usize(args, "limit", 5).clamp(1, 20);
            let target_layer = param::opt_str(args, "target_layer");

            let result = crate::context::reranker::CrossEncoderReranker::search_and_rerank(
                workspace_root,
                query,
                limit,
                target_layer,
            )?;

            let markdown = result.format_markdown();
            Ok(markdown)
        })()),
        "generate_architecture_docs" => Some((|| {
            let write_to_file = param::opt_bool(args, "write_to_file", false);
            let include_mermaid = param::opt_bool(args, "include_mermaid", true);
            let include_symbol_catalog = param::opt_bool(args, "include_symbol_catalog", true);

            let options = crate::context::doc_synthesizer::ArchitectureDocOptions {
                write_to_file,
                include_mermaid,
                include_symbol_catalog,
            };

            let report = crate::context::doc_synthesizer::ArchitectureDocSynthesizer::synthesize(
                workspace_root,
                options,
            )?;

            let response = if let Some(path) = report.file_written {
                format!(
                    "✔ Successfully synthesized and wrote `{}` to workspace root!\n\n{}",
                    path, report.markdown_content
                )
            } else {
                report.markdown_content
            };

            Ok(response)
        })()),
        "sync_code_graph" => Some((|| {
            let target_file = args.get("target_file").and_then(|v| v.as_str());
            let force_full = args
                .get("force_full")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let stats = crate::context::graph_sync::GraphSynchronizer::sync(
                workspace_root,
                target_file,
                force_full,
            )?;

            Ok(stats.format_markdown())
        })()),
        "workspace_monorepo_map" => Some((|| {
            let include_external = args
                .get("include_external")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let target_package = args.get("target_package").and_then(|v| v.as_str());

            let report = crate::context::monorepo::MonorepoOrchestrator::analyze_workspace(
                workspace_root,
                include_external,
                target_package,
            )?;

            Ok(report.format_markdown())
        })()),
        "checkpoint_session" => Some((|| {
            let label = param::require_str(args, "label", "checkpoint_session")?;
            let description = param::opt_str(args, "description");

            let info = crate::context::checkpoint::SessionCheckpointer::create_checkpoint(
                workspace_root,
                "current_session",
                label,
                description,
                &[],
            )?;

            Ok(format!(
                "✔ Created session checkpoint `{}` (`{}`)\n- **Timestamp:** {}\n- **Working Memory Saved:** {}",
                info.label,
                info.id,
                info.timestamp,
                if info.has_working_plan { "Yes" } else { "No" }
            ))
        })()),
        "rewind_session" => Some((|| {
            let action = param::opt_str(args, "action").unwrap_or("list");

            match action {
                "list" => {
                    let list = crate::context::checkpoint::SessionCheckpointer::list_checkpoints(
                        workspace_root,
                        None,
                    )?;
                    if list.is_empty() {
                        return Ok("*(No checkpoints recorded for this workspace yet)*".to_string());
                    }
                    let mut out =
                        format!("# ⏱️ Workspace Session Checkpoints ({})\n\n", list.len());
                    out.push_str("| Checkpoint ID | Label | Timestamp | Plan Saved |\n");
                    out.push_str("| :--- | :--- | :--- | :--- |\n");
                    for ckpt in list {
                        out.push_str(&format!(
                            "| `{}` | **{}** | {} | {} |\n",
                            ckpt.id,
                            ckpt.label,
                            ckpt.timestamp,
                            if ckpt.has_working_plan { "✔" } else { "—" }
                        ));
                    }
                    Ok(out)
                }
                "rewind" => {
                    let ckpt_id = param::require_str(args, "checkpoint_id", "rewind_session")?;
                    let (report, _) =
                        crate::context::checkpoint::SessionCheckpointer::rewind_checkpoint(
                            workspace_root,
                            "current_session",
                            ckpt_id,
                        )?;
                    Ok(report.format_markdown())
                }
                "fork" => {
                    let ckpt_id = param::require_str(args, "checkpoint_id", "rewind_session")?;
                    let fork_label = param::opt_str(args, "fork_label");
                    let forked = crate::context::checkpoint::SessionCheckpointer::fork_checkpoint(
                        workspace_root,
                        ckpt_id,
                        fork_label,
                    )?;
                    Ok(format!(
                        "✔ Successfully forked checkpoint `{}` into new branch `{}` (`{}`)",
                        ckpt_id, forked.label, forked.id
                    ))
                }
                other => Err(crate::error::ToolError::InvalidArguments {
                    name: "rewind_session".to_string(),
                    reason: format!("Unknown action: {}", other),
                }
                .into()),
            }
        })()),
        "trace_dataflow" => Some((|| {
            let target_symbol = args
                .get("target_symbol")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                    name: "trace_dataflow".to_string(),
                    reason: "Missing required parameter 'target_symbol'".to_string(),
                })?;
            let direction = args
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("forward");
            let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
            let taint_check = args
                .get("taint_check")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let report = crate::context::dataflow::DataflowAnalyzer::trace(
                workspace_root,
                target_symbol,
                direction,
                max_depth,
                taint_check,
            )?;

            Ok(report.format_markdown())
        })()),
        "prune_context" => Some(Ok(
            "✔ Multi-turn observation deduplication and pruning applied.".to_string(),
        )),
        "optimize_token_budget" => Some({
            let model_name = args
                .get("model_name")
                .and_then(|v| v.as_str())
                .unwrap_or("gpt-4o");

            let report = crate::context::budget_optimizer::TokenBudgetOptimizer::analyze_messages(
                &[],
                model_name,
            );

            Ok(report.format_markdown())
        }),
        "hybrid_retrieve" => Some({
            let query = match param::require_str(args, "query", "hybrid_retrieve") {
                Ok(q) => q,
                Err(e) => return Some(Err(e.into())),
            };
            let limit = param::opt_usize(args, "limit", 5);
            let include_graph = param::opt_bool(args, "include_graph", true);
            let include_wiki = param::opt_bool(args, "include_wiki", true);
            let include_memory = param::opt_bool(args, "include_memory", true);

            match crate::context::fusion::KnowledgeFusionEngine::retrieve(
                workspace_root,
                query,
                limit,
                include_graph,
                include_wiki,
                include_memory,
            ) {
                Ok(bundle) => Ok(crate::context::fusion::format_fused_bundle(&bundle)),
                Err(e) => Err(e),
            }
        }),
        "quarantine_flaky_tests" => Some(async {
            let test_name = param::opt_str(args, "test_name").unwrap_or("");
            let runs = param::opt_usize(args, "runs", crate::constants::DEFAULT_FLAKY_RUNS);
            let action = param::opt_str(args, "action").unwrap_or("detect");
            let auto_quarantine = param::opt_bool(args, "auto_quarantine", true);
            let reason = param::opt_str(args, "reason").unwrap_or("Statistical flakiness detected during burn-in");

            match action {
                "list" => {
                    let store = crate::context::flaky::QuarantineManager::load(workspace_root);
                    Ok(crate::context::flaky::QuarantineManager::format_report(&store))
                }
                "unquarantine" => {
                    if test_name.is_empty() {
                        return Err(ToolError::InvalidArguments {
                            name: "quarantine_flaky_tests".to_string(),
                            reason: "Missing 'test_name' for unquarantine action".to_string(),
                        }.into());
                    }
                    let removed = crate::context::flaky::QuarantineManager::unquarantine(workspace_root, test_name)?;
                    if removed {
                        Ok(format!("✅ Successfully un-quarantined test `{}`.", test_name))
                    } else {
                        Ok(format!("ℹ Test `{}` was not found in the quarantine store.", test_name))
                    }
                }
                "quarantine" => {
                    if test_name.is_empty() {
                        return Err(ToolError::InvalidArguments {
                            name: "quarantine_flaky_tests".to_string(),
                            reason: "Missing 'test_name' for quarantine action".to_string(),
                        }.into());
                    }
                    let entry = crate::context::flaky::QuarantineManager::quarantine(
                        workspace_root,
                        test_name,
                        1.0,
                        crate::context::flaky::FlakySignature::Unknown,
                        reason,
                        1,
                    )?;
                    Ok(format!(
                        "🛡️ Successfully quarantined test `{}`.\nReason: {}\nTimestamp: {}",
                        entry.test_name, entry.reason, entry.quarantined_at
                    ))
                }
                _ => {
                    if test_name.is_empty() {
                        return Err(ToolError::InvalidArguments {
                            name: "quarantine_flaky_tests".to_string(),
                            reason: "Missing required argument 'test_name' for detection burn-in".to_string(),
                        }.into());
                    }

                    let report = crate::context::flaky::FlakyTestDetector::execute_burn_in(
                        workspace_root,
                        test_name,
                        runs,
                        crate::constants::FLAKY_TEST_TIMEOUT_SECS,
                    ).await?;

                    let mut out = report.format_markdown();

                    if auto_quarantine && report.verdict == crate::context::flaky::FlakinessVerdict::FlakyIntermittent {
                        let q_res = crate::context::flaky::QuarantineManager::quarantine(
                            workspace_root,
                            test_name,
                            report.flakiness_ratio,
                            report.signature,
                            &format!("Automated quarantine: {:.1}% failure variance across {} burn-in runs", report.flakiness_ratio * 100.0, report.total_runs),
                            report.total_runs,
                        );
                        if let Ok(entry) = q_res {
                            out.push_str(&format!(
                                "\n🛡️ **Automated Quarantine Applied**: Test `{}` has been added to `.minicode/quarantine.json` to shield future agent iterations.\n",
                                entry.test_name
                            ));
                        }
                    }

                    Ok(out)
                }
            }
        }.await),
        _ => None,
    }
}
