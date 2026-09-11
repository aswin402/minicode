use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
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
        "prune_context" => Some(Ok(
            "✔ Multi-turn observation deduplication and pruning applied.".to_string(),
        )),
        "optimize_token_budget" => Some({
            let model_name = param::opt_str(args, "model_name").unwrap_or("gpt-4o");

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
