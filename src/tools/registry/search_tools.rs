use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::parse_u64_param;
use crate::tools::search;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "grep_search".to_string(),
            description: "Search for regex patterns or text across workspace files respecting .gitignore.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The text or regular expression to search for"
                    },
                    "is_regex": {
                        "type": "boolean",
                        "description": "Whether to treat query as a regex (default: false)"
                    },
                    "file_pattern": {
                        "type": "string",
                        "description": "Optional glob filter for file names (e.g. '*.rs')"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSchema {
            name: "locate_symbol".to_string(),
            description: "Instantly locate symbol declarations, signatures, and doc comments across the workspace without full grep scans.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The exact or partial symbol name to locate"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of matches to return (default: 10)"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "semantic_search".to_string(),
            description: "Perform fast sub-millisecond offline semantic vector code search using intent queries across all indexed source files.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language query describing the logic, concept, or feature to find"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of ranked code snippets to return (default 5)"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSchema {
            name: "search_symbols_semantic".to_string(),
            description: "Semantically search specifically for AST symbol definitions (functions, structs, classes, interfaces) matching an intent or concept.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Symbol name or concept description"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of symbols to return (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSchema {
            name: "ast_query".to_string(),
            description: "Query Tree-sitter AST syntax tree nodes for a file (e.g. functions, structs, classes, impls) with optional kind and name filters.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Relative workspace file path (e.g. 'src/app.rs', 'server.ts', 'main.py')"
                    },
                    "node_kind": {
                        "type": "string",
                        "description": "Optional AST node kind filter (e.g. 'function_item', 'struct_item', 'class_definition')"
                    },
                    "name_filter": {
                        "type": "string",
                        "description": "Optional substring filter for symbol names"
                    }
                },
                "required": ["file_path"]
            }),
        },
        ToolSchema {
            name: "ast_extract_symbol".to_string(),
            description: "Extract the exact AST code definition and line boundaries for a named symbol in a file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Relative workspace file path"
                    },
                    "symbol_name": {
                        "type": "string",
                        "description": "Exact name of the function, struct, class, or method"
                    }
                },
                "required": ["file_path", "symbol_name"]
            }),
        },
        ToolSchema {
            name: "ast_diff".to_string(),
            description: "Compute semantic AST structural diff (added/removed/modified functions, classes, structs, signature changes, breaking changes) between versions of a file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Relative workspace file path (e.g. 'src/lib.rs', 'app.py', 'index.ts')"
                    },
                    "new_content": {
                        "type": "string",
                        "description": "Optional proposed new content to diff against existing on-disk file. If omitted, diffs working copy against git HEAD."
                    }
                },
                "required": ["file_path"]
            }),
        },
    ]
}

pub fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "grep_search" => Some((|| {
            let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "grep_search".to_string(),
                    reason: "Missing required argument 'query'".to_string(),
                }
            })?;
            let is_regex = args
                .get("is_regex")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let pattern = args.get("file_pattern").and_then(|v| v.as_str());
            search::grep_search(workspace_root, query, is_regex, pattern)
        })()),
        "locate_symbol" => Some((|| {
            let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "locate_symbol".to_string(),
                    reason: "Missing required argument 'name'".to_string(),
                }
            })?;
            let limit = parse_u64_param(args.get("limit"))
                .unwrap_or(crate::constants::DEFAULT_LOCATE_SYMBOL_LIMIT as u64)
                as usize;
            let mut index = crate::context::index::SymbolIndex::new();
            index.build_index(workspace_root)?;
            let matches = if name.contains(' ') {
                index.search_symbols(name, limit)
            } else {
                let mut res = index.locate_symbol(name);
                if res.is_empty() {
                    res = index.search_symbols(name, limit);
                }
                res.truncate(limit);
                res
            };
            Ok(index.format_matches(&matches, workspace_root))
        })()),
        "semantic_search" => Some((|| {
            let query = args["query"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "semantic_search".to_string(),
                    reason: "Missing 'query'".to_string(),
                })?;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

            let mut index = crate::context::semantic::SemanticIndex::new();
            let _ = index.build_index(workspace_root)?;
            let results = index.search(query, limit);

            if results.is_empty() {
                Ok(format!(
                    "ℹ No semantic matches found for query `{}`.",
                    query
                ))
            } else {
                let mut out = format!(
                    "🔍 Semantic Search Results for `{}` ({} matches):\n\n",
                    query,
                    results.len()
                );
                for (i, r) in results.iter().enumerate() {
                    out.push_str(&format!(
                        "{}. `{}:{}-{}` (Score: {:.2})\n```\n{}\n```\n\n",
                        i + 1,
                        r.file_path,
                        r.start_line,
                        r.end_line,
                        r.similarity_score,
                        r.snippet.trim()
                    ));
                }
                Ok(out)
            }
        })()),
        "search_symbols_semantic" => Some((|| {
            let query = args["query"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "search_symbols_semantic".to_string(),
                    reason: "Missing 'query'".to_string(),
                })?;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

            let mut index = crate::context::semantic::SemanticIndex::new();
            let _ = index.build_index(workspace_root)?;
            let results = index.search_symbols(query, limit);

            if results.is_empty() {
                Ok(format!(
                    "ℹ No semantic symbol matches found for `{}`.",
                    query
                ))
            } else {
                let mut out = format!(
                    "🔍 Semantic Symbol Search Results for `{}` ({} matches):\n\n",
                    query,
                    results.len()
                );
                for (i, r) in results.iter().enumerate() {
                    let sym_tag = match (&r.symbol_kind, &r.symbol_name) {
                        (Some(k), Some(n)) => format!(" [{}:{}]", k, n),
                        _ => String::new(),
                    };
                    out.push_str(&format!(
                        "{}. `{}:{}-{}`{} (Score: {:.2})\n```\n{}\n```\n\n",
                        i + 1,
                        r.file_path,
                        r.start_line,
                        r.end_line,
                        sym_tag,
                        r.similarity_score,
                        r.snippet.trim()
                    ));
                }
                Ok(out)
            }
        })()),
        "ast_query" => Some((|| {
            let file_path =
                args["file_path"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "ast_query".to_string(),
                        reason: "Missing 'file_path'".to_string(),
                    })?;
            let node_kind = args.get("node_kind").and_then(|v| v.as_str());
            let name_filter = args.get("name_filter").and_then(|v| v.as_str());

            let nodes = crate::context::ast_transform::AstTransformer::query_nodes(
                workspace_root,
                file_path,
                node_kind,
                name_filter,
            )?;

            if nodes.is_empty() {
                Ok(format!("ℹ No matching AST nodes found in `{}`.", file_path))
            } else {
                let mut out = format!(
                    "🌳 AST Query Results for `{}` ({} nodes):\n\n",
                    file_path,
                    nodes.len()
                );
                for (i, node) in nodes.iter().enumerate() {
                    let pub_marker = if node.is_public { " [pub]" } else { "" };
                    out.push_str(&format!(
                        "{}. `{}` **{}**{} (Lines {}-{})\n```\n{}\n```\n\n",
                        i + 1,
                        node.kind,
                        node.name,
                        pub_marker,
                        node.start_line,
                        node.end_line,
                        node.snippet.trim()
                    ));
                }
                Ok(out)
            }
        })()),
        "ast_extract_symbol" => Some((|| {
            let file_path =
                args["file_path"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "ast_extract_symbol".to_string(),
                        reason: "Missing 'file_path'".to_string(),
                    })?;
            let symbol_name =
                args["symbol_name"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "ast_extract_symbol".to_string(),
                        reason: "Missing 'symbol_name'".to_string(),
                    })?;

            let node = crate::context::ast_transform::AstTransformer::extract_symbol(
                workspace_root,
                file_path,
                symbol_name,
            )?;

            let pub_marker = if node.is_public { " [public]" } else { "" };
            let report = format!(
                "🌳 Extracted AST Symbol: **{}**{} (Kind: `{}`)\n📁 File: `{}` (Lines {}-{})\n\n```\n{}\n```",
                node.name,
                pub_marker,
                node.kind,
                file_path,
                node.start_line,
                node.end_line,
                node.snippet.trim()
            );
            Ok(report)
        })()),
        "ast_diff" => Some((|| {
            let file_path =
                args["file_path"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "ast_diff".to_string(),
                        reason: "Missing 'file_path'".to_string(),
                    })?;
            let new_content = args.get("new_content").and_then(|v| v.as_str());

            let report = crate::context::ast_diff::AstDiffEngine::diff_file(
                workspace_root,
                file_path,
                new_content,
            )?;

            Ok(report.format_markdown())
        })()),
        _ => None,
    }
}
