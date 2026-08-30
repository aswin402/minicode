use crate::agent::provider::ToolSchema;
use crate::context::explorer::CodeExploreEngine;
use crate::context::graph::CodeGraph;
use crate::context::layers::LayerClassifier;
use crate::error::{Result, ToolError};
use serde_json::json;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "code_explore".to_string(),
            description: "Surgically explore codebase AST symbols, source code definitions, incoming callers, outgoing callees, and change blast radius in a single dense call.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "High-level question, feature area, or symbol name (e.g. 'execute_turn', 'auth session validation', 'scaffold')"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Optional specific target function, struct, class, or trait name to pinpoint"
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Call graph traversal depth (default: 2)"
                    },
                    "include_source": {
                        "type": "boolean",
                        "description": "Whether to include verbatim source code snippets in the result (default: true)"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSchema {
            name: "diff_impact".to_string(),
            description: "Analyze uncommitted git diffs against the codebase AST dependency graph to compute blast radius, affected architectural layers, and test coverage before committing.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "staged_only": {
                        "type": "boolean",
                        "description": "If true, inspects only staged git changes (git diff --staged)"
                    },
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional explicit list of files to analyze instead of reading current git diff"
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
        "code_explore" => {
            let query = match args.get("query").and_then(|v| v.as_str()) {
                Some(q) => q,
                None => {
                    return Some(Err(ToolError::InvalidArguments {
                        name: "code_explore".to_string(),
                        reason: "Missing required 'query' argument".to_string(),
                    }
                    .into()));
                }
            };

            let symbol = args.get("symbol").and_then(|v| v.as_str());
            let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
            let include_source = args
                .get("include_source")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let mut graph = CodeGraph::new();
            if let Err(e) = graph.build_graph(workspace_root) {
                return Some(Err(e));
            }

            match CodeExploreEngine::explore(
                workspace_root,
                &graph,
                query,
                symbol,
                max_depth,
                include_source,
            ) {
                Ok(res) => Some(Ok(res.summary)),
                Err(e) => Some(Err(e)),
            }
        }
        "diff_impact" => {
            let staged_only = args
                .get("staged_only")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let explicit_files: Option<Vec<String>> =
                args.get("files").and_then(|v| v.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str().map(|str_val| str_val.to_string()))
                        .collect()
                });

            let modified_files = if let Some(files) = explicit_files {
                files
            } else {
                // Run git diff to find modified files
                let mut cmd = Command::new("git");
                cmd.arg("diff").arg("--name-only");
                if staged_only {
                    cmd.arg("--staged");
                }
                cmd.current_dir(workspace_root);

                match cmd.output() {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        stdout
                            .lines()
                            .map(|l| l.trim().to_string())
                            .filter(|l| !l.is_empty())
                            .collect()
                    }
                    Err(_) => Vec::new(),
                }
            };

            if modified_files.is_empty() {
                return Some(Ok("### 📊 Diff Impact Analysis\n\nNo uncommitted file changes detected in workspace git status.".to_string()));
            }

            let mut graph = CodeGraph::new();
            if let Err(e) = graph.build_graph(workspace_root) {
                return Some(Err(e));
            }

            let mut out = format!(
                "### 📊 Diff Impact & Blast Radius Report ({} modified files)\n\n",
                modified_files.len()
            );

            let mut affected_layers = HashSet::new();
            let mut total_dependents = HashSet::new();
            let mut total_tests = HashSet::new();

            for f in &modified_files {
                let layer = LayerClassifier::classify_path(Path::new(f));
                affected_layers.insert(layer);

                out.push_str(&format!("#### File: `{}` ({})\n", f, layer.badge()));

                match graph.get_blast_radius(f, workspace_root) {
                    Ok(report) => {
                        out.push_str(&format!("- **Risk Assessment**: `{}`\n", report.risk_level));
                        out.push_str(&format!(
                            "- **Direct Callers ({})**: `{}`\n",
                            report.direct_dependents.len(),
                            if report.direct_dependents.is_empty() {
                                "None".to_string()
                            } else {
                                report.direct_dependents.join("`, `")
                            }
                        ));
                        for dep in report.direct_dependents {
                            total_dependents.insert(dep);
                        }
                        for test in report.test_coverage {
                            total_tests.insert(test);
                        }
                    }
                    Err(_) => {
                        out.push_str(
                            "- *File not in indexed AST graph (new or non-source file)*\n",
                        );
                    }
                }
                out.push('\n');
            }

            out.push_str("---\n\n### 🛡️ Overall Change Assessment\n");
            out.push_str(&format!(
                "- **Impacted Architectural Layers**: {}\n",
                affected_layers
                    .iter()
                    .map(|l| l.badge())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            out.push_str(&format!(
                "- **Total Downstream Files Affected**: `{}`\n",
                total_dependents.len()
            ));
            out.push_str(&format!(
                "- **Relevant Test Suites**: `{}`\n",
                if total_tests.is_empty() {
                    "⚠️ None identified — consider running tests for safety".to_string()
                } else {
                    total_tests.into_iter().collect::<Vec<_>>().join("`, `")
                }
            ));

            Some(Ok(out))
        }
        _ => None,
    }
}
