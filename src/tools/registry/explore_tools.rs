use crate::agent::provider::ToolSchema;
use crate::context::explorer::CodeExploreEngine;
use crate::context::graph::CodeGraph;
use crate::context::layers::LayerClassifier;
use crate::error::Result;
use crate::tools::param;
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
        ToolSchema {
            name: "code_explain".to_string(),
            description: "Dense, bounded AST inspection of a specific code symbol: extracts signature, doc comments, bounded source snippet, callers, callees, architectural layer, and blast radius without reading full files.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "Specific target function, method, struct, enum, trait, or class name to inspect"
                    },
                    "file": {
                        "type": "string",
                        "description": "Optional file path or substring to disambiguate if multiple symbols share the same name"
                    },
                    "include_source": {
                        "type": "boolean",
                        "description": "Whether to include verbatim source code slice (default: true)"
                    }
                },
                "required": ["symbol"]
            }),
        },
        ToolSchema {
            name: "code_trace".to_string(),
            description: "Trace execution flow and call hierarchy from an entrypoint symbol across the codebase AST dependency graph, rendering an ASCII tree with architectural metrics.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entrypoint": {
                        "type": "string",
                        "description": "Target function, method, or symbol to trace execution flow from"
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Maximum depth of call hierarchy to explore (default: 3, max: 5)"
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["downstream", "upstream", "both"],
                        "description": "Direction of call graph traversal: 'downstream' (what this calls), 'upstream' (who calls this), or 'both' (default: 'downstream')"
                    }
                },
                "required": ["entrypoint"]
            }),
        },
        ToolSchema {
            name: "code_impact".to_string(),
            description: "Compute the architectural blast radius and risk assessment for modifying a symbol or file before touching code. Reveals direct and transitive dependents, test suites covering this target, cyclic dependencies, and risk level.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Symbol name (e.g. 'execute_turn') or relative file path (e.g. 'src/agent/loop.rs') to evaluate blast radius and architectural risk for"
                    }
                },
                "required": ["target"]
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
            let query = match param::require_query(args, "code_explore") {
                Ok(q) => q,
                Err(e) => return Some(Err(e.into())),
            };

            let symbol = param::get_str_with_aliases(args, &["symbol", "name", "target"]);
            let max_depth = param::opt_usize(args, "max_depth", 2);
            let include_source = param::opt_bool(args, "include_source", true);

            let graph = match CodeGraph::load_or_build(workspace_root) {
                Ok(g) => g,
                Err(e) => return Some(Err(e)),
            };

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
            let staged_only = param::opt_bool(args, "staged_only", false);
            let explicit_files = param::opt_string_array(args, "files")
                .or_else(|| param::opt_path(args).map(|p| vec![p.to_string()]));

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

            let graph = match CodeGraph::load_or_build(workspace_root) {
                Ok(g) => g,
                Err(e) => return Some(Err(e)),
            };

            let projection = crate::git::diff_projector::DiffProjector::project_workspace_diff(
                workspace_root,
                staged_only,
                Some(&graph),
            )
            .await
            .unwrap_or(crate::git::diff_projector::DiffProjectionReport {
                total_files: 0,
                total_symbols_modified: 0,
                changes: vec![],
                affected_caller_files: vec![],
                high_risk_symbols: vec![],
                breaking_changes: vec![],
            });

            let mut out = format!(
                "### 📊 Diff Impact & Blast Radius Report ({} modified files)\n\n",
                modified_files.len()
            );

            if !projection.is_empty() {
                out.push_str(&crate::git::diff_projector::DiffProjector::format_markdown(
                    &projection,
                ));
                out.push_str("\n---\n\n### 📁 File-Level Architectural Breakdown\n\n");
            }

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
        "code_explain" => {
            let symbol = match param::require_str(args, "symbol", "code_explain") {
                Ok(s) => s,
                Err(e) => return Some(Err(e.into())),
            };
            let file_filter = param::opt_path(args);
            let include_source = param::opt_bool(args, "include_source", true);

            let graph = match CodeGraph::load_or_build(workspace_root) {
                Ok(g) => g,
                Err(e) => return Some(Err(e)),
            };

            let res = match CodeExploreEngine::explore(
                workspace_root,
                &graph,
                symbol,
                Some(symbol),
                2,
                include_source,
            ) {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };

            let mut summary = if let Some(filter) = file_filter {
                let filtered_matches: Vec<_> = res
                    .matches
                    .into_iter()
                    .filter(|m| m.file_path.contains(filter))
                    .collect();
                if filtered_matches.is_empty() {
                    format!(
                        "### 🔍 Symbol: `{}`\n\nNo symbol matching `{}` found in file matching filter `{}`.",
                        symbol, symbol, filter
                    )
                } else {
                    let mut out = format!(
                        "### 🔍 Symbol Bounded Slice: `{}` (Filtered by `{}`)\n\n",
                        symbol, filter
                    );
                    for m in filtered_matches {
                        out.push_str(&format!(
                            "#### `{}` [{}] ({})\n",
                            m.symbol_name,
                            m.kind,
                            m.layer.badge()
                        ));
                        out.push_str(&format!(
                            "- **Location**: `{}:{}-{}`\n",
                            m.file_path, m.line_range.0, m.line_range.1
                        ));
                        out.push_str(&format!("- **Signature**: `{}`\n", m.signature));
                        if let Some(doc) = m.doc_comment {
                            out.push_str(&format!("- **Doc**: *{}*\n", doc.trim()));
                        }
                        if let Some(src) = m.source_code {
                            out.push_str("\n```\n");
                            out.push_str(&src);
                            out.push_str("\n```\n\n");
                        }
                        out.push_str(&format!(
                            "- **Callers ({})**: {}\n",
                            m.callers.len(),
                            if m.callers.is_empty() {
                                "None (Root)".to_string()
                            } else {
                                m.callers
                                    .iter()
                                    .map(|c| {
                                        format!("`{}` in `{}:{}`", c.name, c.file_path, c.line)
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            }
                        ));
                        out.push_str(&format!(
                            "- **Callees ({})**: {}\n",
                            m.callees.len(),
                            if m.callees.is_empty() {
                                "None (Leaf)".to_string()
                            } else {
                                m.callees
                                    .iter()
                                    .map(|c| {
                                        format!("`{}` in `{}:{}`", c.name, c.file_path, c.line)
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            }
                        ));
                    }
                    out
                }
            } else {
                res.summary
            };

            // Enrich with architectural blast radius if available
            if let Ok(blast) = graph.get_blast_radius(symbol, workspace_root) {
                summary.push_str("\n---\n**Impact Assessment**:\n");
                summary.push_str(&format!(
                    "- **Risk Level**: `{}` (Direct: {}, Transitive: {})\n",
                    blast.risk_level,
                    blast.direct_dependents.len(),
                    blast.transitive_dependents.len()
                ));
                summary.push_str(&format!(
                    "- **Test Coverage**: {}\n",
                    if blast.test_coverage.is_empty() {
                        "⚠️ Untested".to_string()
                    } else {
                        blast.test_coverage.join(", ")
                    }
                ));
            }

            Some(Ok(summary))
        }
        "code_trace" => {
            let entrypoint = match param::require_str(args, "entrypoint", "code_trace") {
                Ok(e) => e,
                Err(e) => return Some(Err(e.into())),
            };
            let max_depth = param::opt_usize(args, "max_depth", 3).clamp(1, 5);
            let direction_str = param::opt_str(args, "direction").unwrap_or("downstream");
            let mode = match direction_str.to_lowercase().as_str() {
                "upstream" | "callers" => crate::context::graph_visualizer::VisualizeMode::Upstream,
                "both" | "all" => crate::context::graph_visualizer::VisualizeMode::Both,
                _ => crate::context::graph_visualizer::VisualizeMode::Downstream,
            };

            let graph = match CodeGraph::load_or_build(workspace_root) {
                Ok(g) => g,
                Err(e) => return Some(Err(e)),
            };

            match crate::context::graph_visualizer::GraphVisualizer::render(
                workspace_root,
                &graph,
                entrypoint,
                mode,
                max_depth,
            ) {
                Ok(rendered) => Some(Ok(rendered)),
                Err(e) => Some(Err(e)),
            }
        }
        "code_impact" => {
            let target = match param::require_str(args, "target", "code_impact") {
                Ok(t) => t,
                Err(e) => return Some(Err(e.into())),
            };

            let graph = match CodeGraph::load_or_build(workspace_root) {
                Ok(g) => g,
                Err(e) => return Some(Err(e)),
            };

            match graph.get_blast_radius(target, workspace_root) {
                Ok(report) => {
                    let mut out = format!(
                        "### 🎯 Architectural Impact & Blast Radius: `{}`\n\n",
                        report.target
                    );
                    out.push_str(&format!(
                        "- **Target Type**: `{}` in `{}`\n",
                        report.target_type, report.file_path
                    ));
                    out.push_str(&format!(
                        "- **Risk Assessment**: `{}` (Score: {:.2})\n",
                        report.risk_level, report.composite_risk_score
                    ));
                    out.push_str(&format!("- **Summary**: {}\n\n", report.summary));

                    out.push_str("#### 🔗 Direct Dependents:\n");
                    if report.direct_dependents.is_empty() {
                        out.push_str("- *None (isolated target)*\n");
                    } else {
                        for dep in &report.direct_dependents {
                            out.push_str(&format!("- `{}`\n", dep));
                        }
                    }

                    out.push_str(&format!(
                        "\n- **Transitive Dependents Count**: `{}`\n",
                        report.transitive_dependents.len()
                    ));
                    if !report.transitive_dependents.is_empty()
                        && report.transitive_dependents.len() <= 10
                    {
                        for dep in &report.transitive_dependents {
                            out.push_str(&format!("  - `{}`\n", dep));
                        }
                    }

                    out.push_str("\n#### 🧪 Test Coverage:\n");
                    if report.test_coverage.is_empty() {
                        out.push_str("⚠️ **Untested**: No direct test files cover this target.\n");
                    } else {
                        for test in &report.test_coverage {
                            out.push_str(&format!("- ✓ `{}`\n", test));
                        }
                    }

                    if report.in_cyclic_dependency {
                        out.push_str(&format!(
                            "\n⚠️ **Cyclic Dependency Detected**: Cycles involve `{}`\n",
                            report.cycle_members.join("`, `")
                        ));
                    }

                    Some(Ok(out))
                }
                Err(e) => Some(Err(e)),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_explore_tools_schemas_count() {
        let schemas = get_schemas();
        assert_eq!(schemas.len(), 5);
        let names: Vec<_> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"code_explore"));
        assert!(names.contains(&"diff_impact"));
        assert!(names.contains(&"code_explain"));
        assert!(names.contains(&"code_trace"));
        assert!(names.contains(&"code_impact"));
    }

    #[tokio::test]
    async fn test_explore_tools_dispatch() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_explore_tools_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let src_file = temp_dir.join("service.rs");
        std::fs::write(
            &src_file,
            r#"
/// Calculates the total cost with taxes
pub fn calculate_total(subtotal: f64) -> f64 {
    subtotal * 1.1
}

pub fn checkout() {
    let _ = calculate_total(100.0);
}
"#,
        )
        .unwrap();

        // 1. code_explain
        let args = json!({ "symbol": "calculate_total" });
        let res = dispatch("code_explain", &args, &temp_dir).await;
        assert!(res.is_some());
        let output = res.unwrap().unwrap();
        assert!(output.contains("calculate_total"));

        // 2. code_trace
        let trace_args = json!({ "entrypoint": "checkout" });
        let trace_res = dispatch("code_trace", &trace_args, &temp_dir).await;
        assert!(trace_res.is_some());
        let trace_out = trace_res.unwrap().unwrap();
        assert!(trace_out.contains("checkout"));

        // 3. code_impact
        let impact_args = json!({ "target": "calculate_total" });
        let impact_res = dispatch("code_impact", &impact_args, &temp_dir).await;
        assert!(impact_res.is_some());
        let impact_out = impact_res.unwrap().unwrap();
        assert!(impact_out.contains("Architectural Impact"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
