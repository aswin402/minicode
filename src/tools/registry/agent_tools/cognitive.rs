use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "record_episode".to_string(),
            description: "Record a completed task episode, bug fix, or architectural breakthrough into long-term vector memory for cross-session recall.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Concise summary title of the episode/solution"
                    },
                    "summary": {
                        "type": "string",
                        "description": "Detailed explanation of what was fixed, designed, or learned"
                    },
                    "tags": {
                        "type": "array",
                        "description": "Search tags and keywords (e.g. ['tree-sitter', 'segfault'])",
                        "items": { "type": "string" }
                    },
                    "code_references": {
                        "type": "array",
                        "description": "Relevant files or functions changed",
                        "items": { "type": "string" }
                    }
                },
                "required": ["title", "summary"]
            }),
        },
        ToolSchema {
            name: "recall_episodes".to_string(),
            description: "Perform hybrid semantic and keyword search across historical session episodes to recall past architectural decisions and bug fixes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query or problem description to recall"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of episodes to return (default: 3)"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSchema {
            name: "critic_review".to_string(),
            description: "Run an automated Actor-Critic evaluation pass over current workspace changes (compiler diagnostics, linter warnings, git status) to verify code quality.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "sequential_thinking".to_string(),
            description: "Execute a dynamic Graph of Thoughts (GoT) reasoning step to branch hypotheses, score confidence, revise earlier conclusions, and synthesize complex solutions.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "thought_number": {
                        "type": "integer",
                        "description": "Current thought number in the sequence (1-indexed)"
                    },
                    "total_thoughts": {
                        "type": "integer",
                        "description": "Estimated total thoughts required (adaptive)"
                    },
                    "thought": {
                        "type": "string",
                        "description": "The reasoning content, hypothesis analysis, or evaluation"
                    },
                    "is_revision": {
                        "type": "boolean",
                        "description": "Whether this thought revises a prior thought"
                    },
                    "revises_thought": {
                        "type": "integer",
                        "description": "The thought number being revised if is_revision is true"
                    },
                    "branch_from_thought": {
                        "type": "integer",
                        "description": "The thought number to branch off from if exploring an alternative hypothesis"
                    },
                    "branch_id": {
                        "type": "string",
                        "description": "Identifier name for this reasoning branch (e.g. 'hypothesis_a')"
                    },
                    "needs_more_thoughts": {
                        "type": "boolean",
                        "description": "Whether more thinking steps are needed before reaching a conclusion"
                    },
                    "score": {
                        "type": "number",
                        "description": "Optional confidence score between 0.0 and 1.0"
                    }
                },
                "required": ["thought_number", "total_thoughts", "thought", "needs_more_thoughts"]
            }),
        },
        ToolSchema {
            name: "score_task_complexity".to_string(),
            description: "Compute task complexity score (1-10), risk level, estimated token context, and topological subtask decomposition plan before executing complex coding changes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Natural language task or feature description to evaluate"
                    }
                },
                "required": ["task"]
            }),
        },
        ToolSchema {
            name: "explore_hypotheses".to_string(),
            description: "Spawn multiple speculative Git worktree branches to explore and compare alternative implementation hypotheses.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "hypotheses": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of alternative implementation descriptions to explore"
                    }
                },
                "required": ["hypotheses"]
            }),
        },
        ToolSchema {
            name: "evaluate_branch".to_string(),
            description: "Run automated compiler diagnostics and calculate fitness score for a speculative hypothesis branch.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "branch_id": {
                        "type": "string",
                        "description": "Identifier of the hypothesis branch (e.g. 'hyp_20260818_120000_b1')"
                    }
                },
                "required": ["branch_id"]
            }),
        },
        ToolSchema {
            name: "select_best_branch".to_string(),
            description: "Select the winning speculative branch with the highest fitness score and discard temporary alternative branches.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "evaluate_all_branches".to_string(),
            description: "Concurrently evaluate all active speculative hypothesis branches using compiler diagnostics.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "prune_branches".to_string(),
            description: "Automatically prune failed or low-fitness speculative hypothesis worktree branches.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "min_fitness": {
                        "type": "number",
                        "description": "Minimum fitness score threshold below which branches are pruned (default: 0.3)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "compare_branches".to_string(),
            description: "Output a structured comparison matrix table of all speculative branches in the active hypothesis session.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "route_model".to_string(),
            description: "Assess task complexity, check model tier recommendations (Fast, Standard, DeepReasoning), inspect provider health and fallback chains, or query/override active model routing.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_description": {
                        "type": "string",
                        "description": "Optional prompt or task description to assess complexity and optimal model tier"
                    },
                    "force_tier": {
                        "type": "string",
                        "description": "Optional tier override to enforce: 'fast', 'standard', 'deep', or 'auto' (clear override)",
                        "enum": ["fast", "standard", "deep", "auto"]
                    },
                    "query_type": {
                        "type": "string",
                        "description": "Query mode: 'assess' (default), 'status' (health & telemetry), or 'override'",
                        "enum": ["assess", "status", "override"]
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
        "record_episode" => Some((|| {
            let title = param::require_str(args, "title", "record_episode")?;
            let summary = param::require_str(args, "summary", "record_episode")?;
            let tags = param::opt_string_array(args, "tags").unwrap_or_default();
            let code_refs = param::opt_string_array(args, "code_references").unwrap_or_default();

            let mut mem = crate::context::episodic::EpisodicMemory::load(workspace_root)?;
            let ep_id = mem.record_episode(title, summary, tags, code_refs, "current_session");
            mem.save(workspace_root)?;

            Ok(format!("✔ Recorded episodic memory `{}`: **{}**", ep_id, title))
        })()),
        "recall_episodes" => Some((|| {
            let query = param::require_str(args, "query", "recall_episodes")?;
            let limit = param::opt_usize(args, "limit", 3);

            let mem = crate::context::episodic::EpisodicMemory::load(workspace_root)?;
            let results = mem.search(query, limit);

            if results.is_empty() {
                return Ok(format!("ℹ No historical episodes matched query `{}`.", query));
            }

            let mut out = format!("🧠 Recalled {} Relevant Historical Episode(s) for `{}`:\n\n", results.len(), query);
            for (idx, r) in results.iter().enumerate() {
                out.push_str(&format!(
                    "{}. **{}** (Score: {:.2})\n   _{}_\n   🏷 Tags: {}\n   📁 Files: {}\n\n",
                    idx + 1,
                    r.item.title,
                    r.score,
                    r.item.summary,
                    r.item.tags.join(", "),
                    r.item.code_references.join(", ")
                ));
            }
            Ok(out)
        })()),
        "critic_review" => Some(async {
            let report =
                crate::agent::critic::CriticValidator::review_workspace(workspace_root).await?;
            Ok(report.format_for_agent())
        }.await),
        "sequential_thinking" => Some((|| {
            let thought_node: crate::agent::sequential_thinking::ThoughtNode =
                serde_json::from_value(args.clone()).map_err(|e| {
                    ToolError::InvalidArguments {
                        name: "sequential_thinking".to_string(),
                        reason: format!("Invalid thought node parameters: {}", e),
                    }
                })?;
            let output = crate::agent::sequential_thinking::ThinkingSession::step(thought_node)?;
            Ok(output)
        })()),
        "score_task_complexity" => Some((|| {
            let task = param::require_str(args, "task", "score_task_complexity")?;
            let score =
                crate::agent::complexity::TaskComplexityScorer::score_task(workspace_root, task)?;
            Ok(score.format_markdown())
        })()),
        "explore_hypotheses" => Some(async {
            let hypotheses = param::opt_string_array(args, "hypotheses").ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "explore_hypotheses".to_string(),
                    reason: "Missing 'hypotheses' array".to_string(),
                }
            })?;

            let session = crate::agent::hypothesis::HypothesisEngine::create_branches(
                workspace_root,
                &hypotheses,
            )
            .await?;

            let mut out = format!(
                "🌱 Spawned {} speculative branches (Session `{}`):\n\n",
                session.branches.len(),
                session.id
            );
            for (i, b) in session.branches.iter().enumerate() {
                out.push_str(&format!(
                    "{}. **Branch `{}`**:\n   _{}_\n   📁 Worktree: `{}`\n\n",
                    i + 1,
                    b.id,
                    b.description,
                    b.worktree_path.display()
                ));
            }
            out.push_str("👉 Use 'evaluate_branch' to score each branch or 'select_best_branch' to merge the winner.");
            Ok(out)
        }.await),
        "evaluate_branch" => Some(async {
            let branch_id = param::require_str(args, "branch_id", "evaluate_branch")?;

            let branch = crate::agent::hypothesis::HypothesisEngine::evaluate_branch(
                workspace_root,
                branch_id,
            )
            .await?;

            let status_icon = if branch.compiler_clean { "✔" } else { "✗" };
            let report = format!(
                "{} Branch `{}` Evaluation:\n• Fitness Score: {:.2}/1.00\n• Status: {:?}\n• Compiler Clean: {}\n• Errors: {}\n• Warnings: {}\n• Notes: {}",
                status_icon,
                branch.id,
                branch.fitness_score,
                branch.status,
                branch.compiler_clean,
                branch.compiler_errors,
                branch.compiler_warnings,
                branch.notes
            );
            Ok(report)
        }.await),
        "select_best_branch" => Some(async {
            let winner =
                crate::agent::hypothesis::HypothesisEngine::select_best_branch(workspace_root)
                    .await?;

            let report = format!(
                "🏆 Selected Winning Branch `{}`!\n• Description: {}\n• Fitness Score: {:.2}\n• Worktree: `{}`\n\nAll alternative speculative branches have been cleanly discarded.",
                winner.id,
                winner.description,
                winner.fitness_score,
                winner.worktree_path.display()
            );
            Ok(report)
        }.await),
        "evaluate_all_branches" => Some(async {
            let evaluated = crate::agent::hypothesis::HypothesisEngine::evaluate_all_branches(workspace_root).await?;
            let mut out = format!("✔ Evaluated {} speculative branch(es):\n\n", evaluated.len());
            for b in &evaluated {
                out.push_str(&format!(
                    "• `{}`: Status: {:?}, Fitness: {:.2}, Clean: {}\n",
                    b.id, b.status, b.fitness_score, b.compiler_clean
                ));
            }
            Ok(out)
        }.await),
        "prune_branches" => Some(async {
            let min_fitness = param::opt_f64(args, "min_fitness", 0.3) as f32;
            let pruned = crate::agent::hypothesis::HypothesisEngine::prune_failed_branches(workspace_root, min_fitness).await?;
            if pruned.is_empty() {
                Ok("ℹ No branches were below the pruning threshold.".to_string())
            } else {
                Ok(format!("🗑 Pruned {} underperforming branch(es): {}", pruned.len(), pruned.join(", ")))
            }
        }.await),
        "compare_branches" => Some((|| {
            let session = crate::agent::hypothesis::HypothesisEngine::load_session(workspace_root)?;
            Ok(crate::agent::hypothesis::HypothesisEngine::format_comparison_matrix(&session))
        })()),
        "route_model" => Some(async {
            let task_description = param::opt_str(args, "task_description").unwrap_or("");
            let force_tier_str = param::opt_str(args, "force_tier");
            let query_type = param::opt_str(args, "query_type").unwrap_or("assess");

            let mut router = crate::agent::router::AdaptiveModelRouter::new();

            if let Some(tier_str) = force_tier_str {
                let tier = if tier_str.eq_ignore_ascii_case("auto") {
                    None
                } else {
                    crate::agent::router::ModelTier::parse_tier(tier_str)
                };
                router.set_forced_tier(tier);
            }

            match query_type {
                "status" => Ok(router.format_status_report()),
                "override" => {
                    let forced = router.get_forced_tier();
                    let msg = match forced {
                        Some(t) => format!("Router locked to {} tier ({})", t.badge(), t.description()),
                        None => "Router set to adaptive auto-routing mode".to_string(),
                    };
                    Ok(format!("{}\n\n{}", msg, router.format_status_report()))
                }
                _ => {
                    let decision = router.route_turn(task_description, 0, false, 0, 0);
                    let mut out = format!(
                        "# 🔀 Model Routing Assessment: {}\n\n",
                        decision.tier.badge()
                    );
                    out.push_str(&format!("📋 **Task:** {}\n", if task_description.is_empty() { "(empty/general)" } else { task_description }));
                    out.push_str(&format!("🎯 **Selected Tier:** `{}` ({})\n", decision.tier.badge(), decision.tier.description()));
                    out.push_str(&format!("💡 **Reasoning:** {}\n", decision.reason));
                    out.push_str(&format!("💰 **Relative Cost Factor:** {:.2}x\n\n", decision.estimated_cost_factor));
                    out.push_str(&format!(
                        "🚀 **Primary Endpoint:** `{}` / `{}` (Priority: {}, Context: {}k)\n\n",
                        decision.primary_endpoint.provider_name,
                        decision.primary_endpoint.model_name,
                        decision.primary_endpoint.priority,
                        decision.primary_endpoint.max_context / 1000,
                    ));
                    if !decision.fallback_chain.is_empty() {
                        out.push_str("🛡️ **Fallback Chain (Auto-Failover on 429/5xx):**\n");
                        for (idx, fb) in decision.fallback_chain.iter().enumerate() {
                            out.push_str(&format!(
                                "{}. `{}` / `{}` (Priority: {}, Context: {}k{})\n",
                                idx + 1,
                                fb.provider_name,
                                fb.model_name,
                                fb.priority,
                                fb.max_context / 1000,
                                if fb.is_local { ", Local" } else { "" }
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
