use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
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
    ]
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "audit_architecture" | "check_architecture" => Some((|| {
            let mode = param::opt_str(args, "mode").unwrap_or("check");
            let enforce = param::opt_bool(args, "enforce", false);
            let format = param::opt_str(args, "format").unwrap_or("markdown");

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
                    out.push_str("| :--- | :---: | :---: | :---: | :---: | :---: | :--- |\n");
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
            let target_file = param::opt_str(args, "target_file");
            let untested_only = param::opt_bool(args, "untested_only", false);
            let min_risk = param::get_f64(args, "min_risk");

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
            let target_file = param::opt_str(args, "target_file");

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
        "architecture_invariants" => Some((|| {
            let target_file = param::opt_str(args, "target_file");

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
            let target_file = param::opt_str(args, "target_file");
            let min_confidence = param::opt_str(args, "min_confidence");

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
        _ => None,
    }
}
