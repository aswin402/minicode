use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
use crate::tools::parse_u64_param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
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
    ]
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "impact_analysis" => Some((|| {
            let target = param::require_str(args, "target", "impact_analysis")?;
            let mut graph = crate::context::graph::CodeGraph::new();
            graph.build_graph(workspace_root)?;
            let report = graph.get_blast_radius(target, workspace_root)?;
            Ok(report.summary)
        })()),
        "repo_map" => Some((|| {
            let max_tokens =
                param::opt_usize(args, "max_tokens", crate::constants::DEFAULT_MAP_TOKENS);
            let mut graph = crate::context::graph::CodeGraph::new();
            graph.build_graph(workspace_root)?;
            Ok(graph.format_repomap(workspace_root, &[], max_tokens))
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
            let action = param::require_str(args, "action", "ast_refactor")?;
            let file_path = param::require_str(args, "file_path", "ast_refactor")?;

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
                    let new_fn_name =
                        param::opt_str(args, "new_name").unwrap_or("extracted_helper");
                    let params = param::opt_str(args, "params").unwrap_or("");
                    let call_args = param::opt_str(args, "call_args").unwrap_or("");
                    let return_type = param::opt_str(args, "return_type");
                    let is_public = param::opt_bool(args, "is_public", false);

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
                    let target_symbol = param::require_str(args, "target_symbol", "ast_refactor")?;
                    let new_name = param::require_str(args, "new_name", "ast_refactor")?;

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
                    let var_name = param::opt_str(args, "target_symbol")
                        .or_else(|| param::opt_str(args, "new_name"))
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
        "sync_code_graph" => Some((|| {
            let target_file = param::opt_str(args, "target_file");
            let force_full = param::opt_bool(args, "force_full", false);

            let stats = crate::context::graph_sync::GraphSynchronizer::sync(
                workspace_root,
                target_file,
                force_full,
            )?;

            Ok(stats.format_markdown())
        })()),
        "workspace_monorepo_map" => Some((|| {
            let include_external = param::opt_bool(args, "include_external", false);
            let target_package = param::opt_str(args, "target_package");

            let report = crate::context::monorepo::MonorepoOrchestrator::analyze_workspace(
                workspace_root,
                include_external,
                target_package,
            )?;

            Ok(report.format_markdown())
        })()),
        "trace_dataflow" => Some((|| {
            let target_symbol = param::require_str(args, "target_symbol", "trace_dataflow")?;
            let direction = param::opt_str(args, "direction").unwrap_or("forward");
            let max_depth = param::opt_usize(args, "max_depth", 5);
            let taint_check = param::opt_bool(args, "taint_check", true);

            let report = crate::context::dataflow::DataflowAnalyzer::trace(
                workspace_root,
                target_symbol,
                direction,
                max_depth,
                taint_check,
            )?;

            Ok(report.format_markdown())
        })()),
        _ => None,
    }
}
