use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "repo_map".to_string(),
            description: "Generate a compact AST repository skeleton map of symbols ranked by PageRank importance, automatically indexing or incrementally syncing per-file AST shards on unindexed or multi-file projects (pass mode='reuse' for a lean 1-line-per-directory reusable symbol map).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["full", "reuse"],
                        "description": "Output mode: 'reuse' returns only the compact PageRank + Inbound-Degree reusable symbol map for fast YAGNI reuse checks; 'full' (default) returns reusable symbols plus the detailed AST repomap"
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Maximum tokens to spend on repomap output (default: 1024)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "ast_refactor".to_string(),
            description: "Perform deterministic Tree-sitter AST refactoring and code intelligence actions (extract_function, rename_symbol, inline_variable, scan_symbol, delete_symbol, find_unused, move_symbol, rename_file, css_class_rename, css_var_rename) with SyntaxGuard verification, dry_run previews, and outcome tags ([Preview], [Applied], [Refused], [Empty], [Result]).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "extract_function",
                            "rename_symbol",
                            "inline_variable",
                            "scan_symbol",
                            "delete_symbol",
                            "find_unused",
                            "move_symbol",
                            "rename_file",
                            "css_class_rename",
                            "css_var_rename"
                        ],
                        "description": "Refactoring or AST inspection action to execute"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Target or source file path relative to workspace root"
                    },
                    "to_file": {
                        "type": "string",
                        "description": "Destination file path relative to workspace root (for move_symbol and rename_file)"
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
                        "description": "New identifier, class name, CSS variable, or file path"
                    },
                    "target_symbol": {
                        "type": "string",
                        "description": "Target symbol, CSS class, or CSS variable name"
                    },
                    "role_filter": {
                        "type": "string",
                        "enum": ["all", "references", "declarations", "imports", "calls", "types", "properties", "identifiers", "strings"],
                        "description": "Optional AST role filter for scan_symbol (default: 'all')"
                    },
                    "exports_filter": {
                        "type": "string",
                        "enum": ["all", "exported", "local"],
                        "description": "Optional visibility filter for find_unused (default: 'all')"
                    },
                    "include_mermaid": {
                        "type": "boolean",
                        "description": "Whether scan_symbol should append a Mermaid caller/callee graph (default: false)"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "When true, validate syntax and return unified diff [Preview] without modifying files on disk (default: false)"
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
                "required": ["action"]
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
            let target = param::require_str_with_aliases(
                args,
                &["target", "symbol", "path", "file_path"],
                "impact_analysis",
            )?;
            let graph = crate::context::graph::CodeGraph::get_or_build_cached(workspace_root)?;
            let report = graph.get_blast_radius(target, workspace_root)?;
            Ok(report.summary)
        })()),
        "repo_map" => Some((|| {
            let mode = param::opt_str(args, "mode").unwrap_or("full");
            let max_tokens =
                param::opt_usize(args, "max_tokens", crate::constants::DEFAULT_MAP_TOKENS);
            let graph = crate::context::graph::CodeGraph::get_or_build_cached(workspace_root)?;
            let reuse_budget = if mode.eq_ignore_ascii_case("reuse") {
                (max_tokens * 4).max(crate::constants::REUSE_MAP_DEFAULT_CHAR_BUDGET)
            } else {
                crate::constants::REUSE_MAP_DEFAULT_CHAR_BUDGET
            };
            let reuse_map = graph.format_compact_reuse_map(workspace_root, &[], reuse_budget);
            if mode.eq_ignore_ascii_case("reuse") {
                if reuse_map.is_empty() {
                    Ok("No indexed workspace symbols found for reuse map.".to_string())
                } else {
                    Ok(format!(
                        "## Reusable Symbols by Directory (PageRank + Inbound Graph Degree)\n{}",
                        reuse_map
                    ))
                }
            } else {
                let detailed = graph.format_repomap(workspace_root, &[], max_tokens);
                if reuse_map.is_empty() {
                    Ok(detailed)
                } else {
                    Ok(format!(
                        "## Reusable Symbols by Directory (PageRank + Inbound Graph Degree)\n{}\n\n{}",
                        reuse_map, detailed
                    ))
                }
            }
        })()),
        "graph_visualize" => Some((|| {
            let target = param::require_str_with_aliases(
                args,
                &["target", "symbol", "path", "file_path"],
                "graph_visualize",
            )?;
            let mode_str = param::opt_str(args, "mode").unwrap_or("both");
            let max_depth = param::opt_usize(args, "max_depth", 3).clamp(1, 6);

            let mode = crate::context::graph_visualizer::VisualizeMode::from_str(mode_str);

            let graph = crate::context::graph::CodeGraph::get_or_build_cached(workspace_root)?;

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
            let file_opt = param::get_str_with_aliases(
                args,
                &["file_path", "from_file", "path", "file", "target_file"],
            );
            let dry_run = param::opt_bool(args, "dry_run", false);

            let res = match action {
                "extract_function" => {
                    let file_path = file_opt.ok_or_else(|| ToolError::InvalidArguments {
                        name: "ast_refactor".to_string(),
                        reason: "Missing required argument 'file_path'".to_string(),
                    })?;
                    let start_line = param::require_usize(args, "start_line", "ast_refactor")?;
                    let end_line = param::require_usize(args, "end_line", "ast_refactor")?;
                    let new_fn_name =
                        param::opt_str(args, "new_name").unwrap_or("extracted_helper");
                    let params = param::opt_str(args, "params").unwrap_or("");
                    let call_args = param::opt_str(args, "call_args").unwrap_or("");
                    let return_type = param::opt_str(args, "return_type");
                    let is_public = param::opt_bool(args, "is_public", false);

                    crate::context::ast_refactor::AstRefactorer::extract_function_with_options(
                        workspace_root,
                        file_path,
                        start_line,
                        end_line,
                        new_fn_name,
                        params,
                        call_args,
                        return_type,
                        is_public,
                        dry_run,
                    )?
                }
                "rename_symbol" => {
                    let target_symbol = param::require_str_with_aliases(
                        args,
                        &["target_symbol", "symbol", "target", "old_name"],
                        "ast_refactor",
                    )?;
                    let new_name = param::require_str(args, "new_name", "ast_refactor")?;

                    crate::context::ast_refactor::AstRefactorer::rename_symbol_with_options(
                        workspace_root,
                        target_symbol,
                        new_name,
                        file_opt,
                        dry_run,
                    )?
                }
                "inline_variable" => {
                    let file_path = file_opt.ok_or_else(|| ToolError::InvalidArguments {
                        name: "ast_refactor".to_string(),
                        reason: "Missing required argument 'file_path'".to_string(),
                    })?;
                    let var_name = param::require_str_with_aliases(
                        args,
                        &[
                            "target_symbol",
                            "symbol",
                            "target",
                            "variable",
                            "var_name",
                            "new_name",
                        ],
                        "ast_refactor",
                    )?;

                    crate::context::ast_refactor::AstRefactorer::inline_variable_with_options(
                        workspace_root,
                        file_path,
                        var_name,
                        dry_run,
                    )?
                }
                "scan_symbol" => {
                    let target_symbol = param::require_str_with_aliases(
                        args,
                        &["target_symbol", "symbol", "target"],
                        "ast_refactor",
                    )?;
                    let role_filter = param::opt_str(args, "role_filter");
                    let include_mermaid = param::opt_bool(args, "include_mermaid", false);

                    crate::context::ast_refactor::AstRefactorer::scan_symbol(
                        workspace_root,
                        target_symbol,
                        role_filter,
                        file_opt,
                        include_mermaid,
                    )?
                }
                "delete_symbol" => {
                    let file_path = file_opt.ok_or_else(|| ToolError::InvalidArguments {
                        name: "ast_refactor".to_string(),
                        reason: "Missing required argument 'file_path'".to_string(),
                    })?;
                    let target_symbol = param::require_str_with_aliases(
                        args,
                        &["target_symbol", "symbol", "target"],
                        "ast_refactor",
                    )?;

                    crate::context::ast_refactor::AstRefactorer::delete_symbol(
                        workspace_root,
                        file_path,
                        target_symbol,
                        dry_run,
                    )?
                }
                "find_unused" => {
                    let exports_filter = param::opt_str(args, "exports_filter");
                    crate::context::ast_refactor::AstRefactorer::find_unused(
                        workspace_root,
                        file_opt,
                        exports_filter,
                    )?
                }
                "move_symbol" => {
                    let from_file = file_opt.ok_or_else(|| ToolError::InvalidArguments {
                        name: "ast_refactor".to_string(),
                        reason: "Missing required argument 'file_path' (source file)".to_string(),
                    })?;
                    let to_file = param::require_str_with_aliases(
                        args,
                        &["to_file", "destination", "target_path"],
                        "ast_refactor",
                    )?;
                    let target_symbol = param::require_str_with_aliases(
                        args,
                        &["target_symbol", "symbol", "target"],
                        "ast_refactor",
                    )?;

                    crate::context::ast_refactor::AstRefactorer::move_symbol(
                        workspace_root,
                        target_symbol,
                        from_file,
                        to_file,
                        dry_run,
                    )?
                }
                "rename_file" => {
                    let from_file = file_opt.ok_or_else(|| ToolError::InvalidArguments {
                        name: "ast_refactor".to_string(),
                        reason: "Missing required argument 'file_path' (source file)".to_string(),
                    })?;
                    let to_file = param::require_str_with_aliases(
                        args,
                        &["to_file", "new_name", "destination", "target_path"],
                        "ast_refactor",
                    )?;

                    crate::context::ast_refactor::AstRefactorer::rename_file(
                        workspace_root,
                        from_file,
                        to_file,
                        dry_run,
                    )?
                }
                "css_class_rename" => {
                    let old_class = param::require_str_with_aliases(
                        args,
                        &["target_symbol", "old_class", "old_name", "target"],
                        "ast_refactor",
                    )?;
                    let new_class = param::require_str_with_aliases(
                        args,
                        &["new_name", "new_class"],
                        "ast_refactor",
                    )?;
                    let mut rename_map = std::collections::HashMap::new();
                    rename_map.insert(old_class.to_string(), new_class.to_string());

                    crate::context::ast_refactor::AstRefactorer::css_class_rename(
                        workspace_root,
                        &rename_map,
                        file_opt,
                        dry_run,
                    )?
                }
                "css_var_rename" => {
                    let old_var = param::require_str_with_aliases(
                        args,
                        &["target_symbol", "old_var", "old_name", "target"],
                        "ast_refactor",
                    )?;
                    let new_var = param::require_str_with_aliases(
                        args,
                        &["new_name", "new_var"],
                        "ast_refactor",
                    )?;

                    crate::context::ast_refactor::AstRefactorer::css_var_rename(
                        workspace_root,
                        old_var,
                        new_var,
                        file_opt,
                        dry_run,
                    )?
                }
                other => {
                    return Err(ToolError::InvalidArguments {
                        name: "ast_refactor".to_string(),
                        reason: format!("Unknown refactoring action: '{}'", other),
                    }
                    .into());
                }
            };

            if res.outcome_tag == "Result"
                || res.outcome_tag == "Empty"
                || res.outcome_tag == "Refused"
            {
                Ok(format!(
                    "[{}] {}\n{}",
                    res.outcome_tag, res.action, res.diff_preview
                ))
            } else {
                Ok(format!(
                    "[{}] `{}` ({} file(s)):\n```diff\n{}\n```",
                    res.outcome_tag,
                    res.action,
                    res.files_modified.len(),
                    res.diff_preview
                ))
            }
        })()),
        "semantic_code_search" => Some((|| {
            let query = param::require_query(args, "semantic_code_search")?;
            let limit = param::opt_limit(args, 5).clamp(1, 20);
            let target_layer = param::opt_str(args, "target_layer");
            let fts_anchors = param::opt_string_array(args, "fts").unwrap_or_default();
            let file_pattern =
                param::opt_str(args, "file_pattern").or_else(|| param::opt_str(args, "glob"));

            let result =
                crate::context::reranker::CrossEncoderReranker::search_and_rerank_with_options(
                    workspace_root,
                    query,
                    limit,
                    target_layer,
                    &fts_anchors,
                    file_pattern,
                )?;

            let markdown = result.format_markdown();
            Ok(markdown)
        })()),
        "sync_code_graph" => Some((|| {
            let target_file = param::opt_path(args);
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
            let target_symbol = param::require_str_with_aliases(
                args,
                &["target_symbol", "symbol", "target"],
                "trace_dataflow",
            )?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_repo_map_reuse_mode_and_power_diagnose_ponytail_telemetry() {
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("shared.rs"),
            "pub fn format_currency(cents: i64) -> String { format!(\"${:.2}\", cents as f64 / 100.0) }\n",
        )
        .unwrap();
        std::fs::write(
            src_dir.join("main.rs"),
            "mod shared;\nfn main() { let _ = shared::format_currency(250); }\n",
        )
        .unwrap();

        let reuse_only = dispatch("repo_map", &json!({ "mode": "reuse" }), tmp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(reuse_only.contains("Reusable Symbols by Directory"));
        assert!(reuse_only.contains("format_currency"));

        let full_map = dispatch("repo_map", &json!({ "mode": "full" }), tmp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(
            full_map.len() > reuse_only.len(),
            "mode='reuse' must stay strictly leaner than mode='full'"
        );

        let diag = crate::tools::registry::minipower_tools::dispatch(
            "power_diagnose",
            &json!({}),
            tmp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(diag.contains("Ponytail Session Quality Telemetry"));
        assert!(diag.contains("Symbol Reuse Ratio:"));
        assert!(diag.contains("Open Callers (`open_callers`):"));
        assert!(diag.contains("4-State Test Freshness:"));
    }
}
