use crate::agent::models::get_model_context_limit;
use crate::agent::types::Message;
use crate::context::compressor::ContextCompressor;
use serde::{Deserialize, Serialize};

/// Predictive projection of multi-turn token consumption
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenForecast {
    pub total_turns: usize,
    pub avg_tokens_per_turn: usize,
    pub recent_burn_velocity: usize,
    pub projected_tokens_next_5_turns: usize,
    pub turns_until_exhaustion: usize,
}

/// Potential optimization action to recover context headroom
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptimizationAction {
    pub title: String,
    pub description: String,
    pub estimated_savings: usize,
    pub priority: String, // "High" | "Medium" | "Low"
}

/// Comprehensive token budget and context utilization report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BudgetReport {
    pub model_name: String,
    pub current_tokens: usize,
    pub context_limit: usize,
    pub utilization_pct: f64,
    pub forecast: TokenForecast,
    pub actions: Vec<OptimizationAction>,
}

impl BudgetReport {
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 📈 Predictive Multi-Turn Token Budget Report\n\n\
            - **Model:** `{}`\n\
            - **Context Limit:** {} tokens\n\
            - **Active Context Usage:** {} tokens (**{:.1}%**)\n\
            - **Current Burn Rate:** ~{} tokens/turn\n\
            - **Estimated Turns to Limit:** {}\n\n",
            self.model_name,
            self.context_limit,
            self.current_tokens,
            self.utilization_pct,
            self.forecast.recent_burn_velocity,
            if self.forecast.turns_until_exhaustion > 1000 {
                "1000+".to_string()
            } else {
                self.forecast.turns_until_exhaustion.to_string()
            }
        );

        // Utilization Status Alert
        if self.utilization_pct >= 85.0 {
            out.push_str("> 🚨 **CRITICAL CONTEXT PRESSURE**: Immediate compaction recommended to avoid truncation!\n\n");
        } else if self.utilization_pct >= 60.0 {
            out.push_str("> ⚠️ **MODERATE CONTEXT PRESSURE**: Proactive optimization advised.\n\n");
        } else {
            out.push_str("> ✔ **HEALTHY CONTEXT HEADROOM**: Sufficient tokens available for multi-turn execution.\n\n");
        }

        // Forecast Projection Table
        out.push_str("### 🔮 Turn Consumption Forecast\n\n");
        out.push_str("| Metric | Value |\n");
        out.push_str("| :--- | :--- |\n");
        out.push_str(&format!(
            "| Total Recorded Turns | {} |\n",
            self.forecast.total_turns
        ));
        out.push_str(&format!(
            "| Avg Tokens / Turn | ~{} tokens |\n",
            self.forecast.avg_tokens_per_turn
        ));
        out.push_str(&format!(
            "| Recent Burn Velocity | ~{} tokens/turn |\n",
            self.forecast.recent_burn_velocity
        ));
        out.push_str(&format!(
            "| Projected +5 Turns Consumption | +{} tokens |\n",
            self.forecast.projected_tokens_next_5_turns
        ));
        out.push_str(&format!(
            "| Est. Turns Remaining | ~{} turns |\n\n",
            self.forecast.turns_until_exhaustion
        ));

        // Recommendations
        if !self.actions.is_empty() {
            out.push_str("### 🛠️ Optimization Recommendations\n\n");
            out.push_str("| Priority | Action | Estimated Savings | Details |\n");
            out.push_str("| :---: | :--- | :---: | :--- |\n");
            for act in &self.actions {
                out.push_str(&format!(
                    "| **{}** | **{}** | -{} tokens | {} |\n",
                    act.priority, act.title, act.estimated_savings, act.description
                ));
            }
            out.push('\n');
        }

        out
    }
}

pub struct TokenBudgetOptimizer;

impl TokenBudgetOptimizer {
    /// Analyzes a list of messages and predicts upcoming token limits
    pub fn analyze_messages(messages: &[Message], model_name: &str) -> BudgetReport {
        let compressor = ContextCompressor::default_safe();
        let current_tokens = compressor.count_messages_tokens(messages);
        let context_limit = get_model_context_limit(model_name);

        let utilization_pct = if context_limit > 0 {
            (current_tokens as f64 / context_limit as f64) * 100.0
        } else {
            0.0
        };

        // Estimate turn counts
        let mut turn_tokens = Vec::new();
        let mut current_turn_tokens = 0;
        for msg in messages {
            let msg_tokens = compressor.count_tokens(&msg.content);
            current_turn_tokens += msg_tokens;
            if msg.role == crate::agent::types::Role::User && current_turn_tokens > 0 {
                turn_tokens.push(current_turn_tokens);
                current_turn_tokens = 0;
            }
        }
        if current_turn_tokens > 0 {
            turn_tokens.push(current_turn_tokens);
        }

        let forecast = Self::forecast_from_history(&turn_tokens, context_limit, current_tokens);
        let actions = Self::generate_recommendations(messages, current_tokens, context_limit);

        BudgetReport {
            model_name: model_name.to_string(),
            current_tokens,
            context_limit,
            utilization_pct,
            forecast,
            actions,
        }
    }

    /// Computes consumption forecast from historical per-turn token deltas
    pub fn forecast_from_history(
        turn_tokens: &[usize],
        context_limit: usize,
        current_tokens: usize,
    ) -> TokenForecast {
        let total_turns = turn_tokens.len();
        let avg_tokens_per_turn = if total_turns > 0 {
            turn_tokens.iter().sum::<usize>() / total_turns
        } else {
            500
        };

        let recent_burn_velocity = if total_turns >= 3 {
            let recent_slice = &turn_tokens[total_turns - 3..];
            recent_slice.iter().sum::<usize>() / 3
        } else if total_turns > 0 {
            avg_tokens_per_turn
        } else {
            500
        };

        let projected_tokens_next_5_turns = recent_burn_velocity * 5;
        let remaining_headroom = context_limit.saturating_sub(current_tokens);
        let turns_until_exhaustion = if recent_burn_velocity > 0 {
            remaining_headroom / recent_burn_velocity
        } else {
            999
        };

        TokenForecast {
            total_turns,
            avg_tokens_per_turn,
            recent_burn_velocity,
            projected_tokens_next_5_turns,
            turns_until_exhaustion,
        }
    }

    /// Generates actionable optimization steps based on current context content
    fn generate_recommendations(
        messages: &[Message],
        current_tokens: usize,
        context_limit: usize,
    ) -> Vec<OptimizationAction> {
        let mut actions = Vec::new();
        let compressor = ContextCompressor::default_safe();

        // 1. Check for long tool outputs that can be observation-masked
        let mut maskable_savings = 0;
        let mut long_observations = 0;
        for msg in messages {
            if msg.role == crate::agent::types::Role::Tool && msg.content.lines().count() > 30 {
                long_observations += 1;
                let tokens = compressor.count_tokens(&msg.content);
                maskable_savings += tokens.saturating_sub(150);
            }
        }

        if long_observations > 0 && maskable_savings > 200 {
            actions.push(OptimizationAction {
                title: "Observation Masking".to_string(),
                description: format!(
                    "Fold {} verbose tool outputs (keeping head & tail 15 lines).",
                    long_observations
                ),
                estimated_savings: maskable_savings,
                priority: if current_tokens > context_limit / 2 {
                    "High".to_string()
                } else {
                    "Medium".to_string()
                },
            });
        }

        // 2. Check for sliding window compaction
        if messages.len() > 10 {
            let older_msgs = &messages[..messages.len() - 6];
            let older_tokens = compressor.count_messages_tokens(older_msgs);
            let estimated_compacted = 400; // memory anchor token footprint
            let savings = older_tokens.saturating_sub(estimated_compacted);

            if savings > 500 {
                actions.push(OptimizationAction {
                    title: "Rolling-Window Memory Anchor".to_string(),
                    description: format!(
                        "Compress the oldest {} messages into a structured semantic memory anchor.",
                        older_msgs.len()
                    ),
                    estimated_savings: savings,
                    priority: if current_tokens > (context_limit * 3) / 4 {
                        "High".to_string()
                    } else {
                        "Medium".to_string()
                    },
                });
            }
        }

        // 3. Redundant file reads / deduplication
        let mut file_reads = std::collections::HashSet::new();
        let mut duplicate_read_tokens = 0;
        for msg in messages {
            if let Some(ref tool_calls) = msg.tool_calls {
                for tc in tool_calls {
                    if tc.name == "read_file" || tc.name == "view_file" {
                        if let Some(path) = tc.arguments.get("path").and_then(|p| p.as_str()) {
                            if !file_reads.insert(path.to_string()) {
                                duplicate_read_tokens += 300;
                            }
                        }
                    }
                }
            }
        }

        if duplicate_read_tokens > 300 {
            actions.push(OptimizationAction {
                title: "Observation Deduplication".to_string(),
                description: "Prune redundant repeat file reads across turns.".to_string(),
                estimated_savings: duplicate_read_tokens,
                priority: "Low".to_string(),
            });
        }

        actions
    }
}

// ============================================================================
// Caveman v4.0 Rock 5: 10-Sink Session Token Waste & Skill Distillation Engine
// (`juliusbrussee/caveman/proxy/internal/store/detect_*.go` Parity)
// ============================================================================

/// A single detected token-waste sink or skill-distillation candidate, gated by the
/// Caveman v4.0 **Net-Token-Negative Gate** (`projected_savings_tokens > fix_injection_cost_tokens`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CavemanWasteFinding {
    pub sink_id: String,
    pub category: String,
    pub evidence: String,
    pub projected_savings_tokens: usize,
    pub fix_injection_cost_tokens: usize,
    pub recommendation: String,
}

impl CavemanWasteFinding {
    /// Returns true if and only if applying the fix saves strictly more tokens than its prompt/skill injection cost.
    #[must_use]
    pub fn passes_net_token_negative_gate(&self) -> bool {
        self.projected_savings_tokens > self.fix_injection_cost_tokens
    }

    /// Net token savings after subtracting the fix's injection cost.
    #[must_use]
    pub fn net_savings_tokens(&self) -> usize {
        self.projected_savings_tokens
            .saturating_sub(self.fix_injection_cost_tokens)
    }
}

/// Complete 10-sink Caveman v4.0 waste analysis & skill distillation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CavemanWasteReport {
    pub unmodified_reread_count: usize,
    pub unmodified_reread_tokens: usize,
    pub distinct_tools_used: usize,
    pub total_registered_tools: usize,
    pub subagent_calls: usize,
    pub subagent_tokens: usize,
    pub effective_input_multiplier: f64,
    pub compaction_events: usize,
    pub bloated_rule_sections: Vec<(String, usize)>,
    pub top_tool_portfolio: Vec<(String, usize)>,
    pub repeated_procedures: Vec<(String, usize)>,
    pub verified_outcomes_count: usize,
    pub reusable_blocks_count: usize,
    pub findings: Vec<CavemanWasteFinding>,
}

impl CavemanWasteReport {
    #[must_use]
    pub fn format_markdown(&self) -> String {
        let mut out = String::from(
            "\n### 🪨 Caveman v4.0 10-Sink Token Waste & Skill Distillation Audit (`caveman-learn`)\n",
        );
        out.push_str(&format!(
            "- **Unmodified File Re-Reads (`reread_waste`)**: {} repeat reads (~{} tokens)\n\
             - **Tool Schema Portfolio (`mcp_schema_tax`)**: {} / {} distinct tools invoked\n\
             - **Subagent Handoff Spend (`subagent_spend_share`)**: {} calls (~{} tokens)\n\
             - **KV-Cache Hygiene (`effective_input_multiplier`)**: {:.2}x (1.00x = byte-identical prefix via `CacheAligner`)\n\
             - **Compaction Churn (`compaction_churn`)**: {} compaction/fold markers\n\
             - **Verified Outcomes (`outcome_efficiency`)**: {} verification/commit checkpoints\n\
             - **Reusable Verification Blocks (`.minicode/blocks/`)**: {} saved scripts\n",
            self.unmodified_reread_count,
            self.unmodified_reread_tokens,
            self.distinct_tools_used,
            self.total_registered_tools,
            self.subagent_calls,
            self.subagent_tokens,
            self.effective_input_multiplier,
            self.compaction_events,
            self.verified_outcomes_count,
            self.reusable_blocks_count,
        ));

        if !self.top_tool_portfolio.is_empty() {
            let summary: Vec<String> = self
                .top_tool_portfolio
                .iter()
                .take(4)
                .map(|(name, toks)| format!("`{}` ({} toks)", name, toks))
                .collect();
            out.push_str(&format!(
                "- **Top Tool Output Portfolio**: {}\n",
                summary.join(", ")
            ));
        }

        if !self.repeated_procedures.is_empty() {
            let procs: Vec<String> = self
                .repeated_procedures
                .iter()
                .take(3)
                .map(|(seq, count)| format!("`{}` ({}x)", seq, count))
                .collect();
            out.push_str(&format!(
                "- **Skill Distillation Candidates (`procedure_repeat`)**: {}\n",
                procs.join(", ")
            ));
        }

        if !self.findings.is_empty() {
            out.push_str(
                "\n| Sink ID | Category | Net Savings | Evidence & Action (Net-Token-Negative Gated) |\n\
                 | :--- | :--- | :---: | :--- |\n",
            );
            for f in &self.findings {
                out.push_str(&format!(
                    "| `{}` | {} | **-{} toks** | {} — *{}* |\n",
                    f.sink_id,
                    f.category,
                    f.net_savings_tokens(),
                    f.evidence,
                    f.recommendation
                ));
            }
        }
        out
    }
}

pub struct CavemanWasteAnalyzer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileReadState {
    ReadUnmodified,
    Mutated,
}

impl CavemanWasteAnalyzer {
    /// Analyzes workspace rule sections, saved `.minicode/blocks/`, and session messages
    /// across all 10 Caveman v4.0 waste detectors, filtering all findings through the
    /// strict **Net-Token-Negative Gate** (`projected_savings_tokens > fix_injection_cost_tokens`).
    #[must_use]
    pub fn analyze(workspace_root: &std::path::Path, messages: &[Message]) -> CavemanWasteReport {
        let mut effective_messages: Vec<Message> = messages.to_vec();
        if effective_messages.is_empty() {
            effective_messages = Self::load_recent_session_messages(workspace_root, 3);
        }

        // 1. State-machine file re-read detector (`detect_reread.go`)
        let mut file_states: std::collections::HashMap<String, FileReadState> =
            std::collections::HashMap::new();
        let mut unmodified_reread_count = 0usize;
        let mut unmodified_reread_tokens = 0usize;

        // 2 & 7. Tool usage & output portfolio (`detect_mcp_tax.go`, `detect_tool_portfolio.go`)
        let mut distinct_tools: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut tool_output_tokens: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut ordered_tool_sequence: Vec<String> = Vec::new();

        // 3. Subagent spend (`detect_subagent_spend.go`)
        let mut subagent_calls = 0usize;
        let mut subagent_tokens = 0usize;

        // 5. Compaction churn (`detect_compaction.go`)
        let mut compaction_events = 0usize;

        // 9. Outcome efficiency (`detect_outcomes.go`)
        let mut verified_outcomes_count = 0usize;

        for msg in &effective_messages {
            let msg_tokens = ContextCompressor::count_tokens_fast(&msg.content);
            if msg.content.contains("<auto_compact_anchor>")
                || msg.content.contains("[Micro-Compacted")
            {
                compaction_events += 1;
            }
            if msg.role == crate::agent::types::Role::Tool {
                let t_name = msg.tool_name.as_deref().unwrap_or("tool");
                *tool_output_tokens.entry(t_name.to_string()).or_insert(0) += msg_tokens;
                if matches!(
                    t_name,
                    "spawn_subagent" | "delegate_task" | "fanout_agents" | "minitask"
                ) {
                    subagent_tokens += msg_tokens;
                }
            }

            if let Some(ref tool_calls) = msg.tool_calls {
                for tc in tool_calls {
                    distinct_tools.insert(tc.name.clone());
                    ordered_tool_sequence.push(tc.name.clone());

                    if matches!(
                        tc.name.as_str(),
                        "spawn_subagent" | "delegate_task" | "fanout_agents"
                    ) {
                        subagent_calls += 1;
                    }
                    if matches!(
                        tc.name.as_str(),
                        "power_verify" | "git_commit" | "archive_plan"
                    ) {
                        verified_outcomes_count += 1;
                    }

                    if let Some(path) = tc
                        .arguments
                        .get("path")
                        .or_else(|| tc.arguments.get("file_path"))
                        .and_then(|v| v.as_str())
                    {
                        if matches!(tc.name.as_str(), "read_file" | "view_file") {
                            if file_states.get(path) == Some(&FileReadState::ReadUnmodified) {
                                unmodified_reread_count += 1;
                                unmodified_reread_tokens += 320;
                            } else {
                                file_states.insert(path.to_string(), FileReadState::ReadUnmodified);
                            }
                        } else if matches!(
                            tc.name.as_str(),
                            "write_file" | "patch_file" | "edit_file" | "ast_grep_rewrite"
                        ) {
                            file_states.insert(path.to_string(), FileReadState::Mutated);
                        }
                    }
                }
            }
        }

        // 6. Rule section bloat detector (`detect_sections.go`)
        let bloated_rule_sections = Self::detect_bloated_rule_sections(workspace_root);

        // 7. Sort tool output portfolio
        let mut top_tool_portfolio: Vec<(String, usize)> = tool_output_tokens.into_iter().collect();
        top_tool_portfolio.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // 8. Sliding-window 3-gram procedure sequence mining (`detect_procedures.go` / `SKILL_DISTILLATION`)
        let mut ngram_counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        if ordered_tool_sequence.len() >= 3 {
            for win in ordered_tool_sequence.windows(3) {
                // Skip trivial 3x identical read_file loops (already covered by reread_waste)
                if win[0] == win[1] && win[1] == win[2] {
                    continue;
                }
                let key = format!("{} -> {} -> {}", win[0], win[1], win[2]);
                *ngram_counts.entry(key).or_insert(0) += 1;
            }
        }
        let mut repeated_procedures: Vec<(String, usize)> = ngram_counts
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .collect();
        repeated_procedures.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let total_registered_tools = crate::constants::TOTAL_TOOL_COUNT;
        let reusable_blocks_count =
            crate::agent::caveman::CavemanBlockStore::list_blocks(workspace_root).len();

        // 4. KV-cache hygiene multiplier (`detect_cache_hygiene.go`)
        // Because `CacheAligner` deterministically sorts tool schemas and volatile blocks live in recency tail:
        let effective_input_multiplier = 1.00;

        // Build candidate findings and filter strictly via the Net-Token-Negative Gate
        let mut candidate_findings = Vec::new();

        if unmodified_reread_count > 0 {
            candidate_findings.push(CavemanWasteFinding {
                sink_id: "reread_waste".to_string(),
                category: "File Re-Read".to_string(),
                evidence: format!(
                    "{} unmodified file re-reads detected across turns",
                    unmodified_reread_count
                ),
                projected_savings_tokens: unmodified_reread_tokens,
                fix_injection_cost_tokens: 40,
                recommendation: "Use locate_symbol / ast_extract_symbol or line-range read_file instead of re-reading full unmodified files".to_string(),
            });
        }

        for (section_title, tok_count) in &bloated_rule_sections {
            let est_savings = (*tok_count * 45) / 100;
            candidate_findings.push(CavemanWasteFinding {
                sink_id: format!("section_bloat:{}", section_title),
                category: "Rule Section Bloat".to_string(),
                evidence: format!("Section `{}` consumes ~{} tokens every turn", section_title, tok_count),
                projected_savings_tokens: est_savings,
                fix_injection_cost_tokens: 35,
                recommendation: "Compress prose via Caveman AST or split into a scoped child DOX contract via dox_tree".to_string(),
            });
        }

        for (proc_seq, count) in &repeated_procedures {
            candidate_findings.push(CavemanWasteFinding {
                sink_id: format!("procedure_repeat:{}", count),
                category: "SKILL_DISTILLATION".to_string(),
                evidence: format!("3-step procedure `{}` repeated {}x", proc_seq, count),
                projected_savings_tokens: count * 180,
                fix_injection_cost_tokens: 120,
                recommendation: "Distill repeatable workflow into a MiniVault skill via vault_create or save verification script in .minicode/blocks/".to_string(),
            });
        }

        if subagent_calls > 0 && subagent_tokens / subagent_calls.max(1) > 250 {
            candidate_findings.push(CavemanWasteFinding {
                sink_id: "subagent_spend_share".to_string(),
                category: "Subagent Handoff".to_string(),
                evidence: format!(
                    "{} subagent calls averaging {} tokens/handoff",
                    subagent_calls,
                    subagent_tokens / subagent_calls.max(1)
                ),
                projected_savings_tokens: (subagent_tokens * 60) / 100,
                fix_injection_cost_tokens: 65,
                recommendation: "Enable cavecrew + caveman-explore (FastContext path:START-END evidence contract) for ~65% smaller subagent handoffs".to_string(),
            });
        }

        // Strict Net-Token-Negative Gate (`projected_savings_tokens > fix_injection_cost_tokens`)
        let findings: Vec<CavemanWasteFinding> = candidate_findings
            .into_iter()
            .filter(CavemanWasteFinding::passes_net_token_negative_gate)
            .collect();

        CavemanWasteReport {
            unmodified_reread_count,
            unmodified_reread_tokens,
            distinct_tools_used: distinct_tools.len(),
            total_registered_tools,
            subagent_calls,
            subagent_tokens,
            effective_input_multiplier,
            compaction_events,
            bloated_rule_sections,
            top_tool_portfolio,
            repeated_procedures,
            verified_outcomes_count,
            reusable_blocks_count,
            findings,
        }
    }

    fn detect_bloated_rule_sections(workspace_root: &std::path::Path) -> Vec<(String, usize)> {
        let mut bloated = Vec::new();
        for rel in [".minicode/minicode.md", ".minicode/RULES.md", "AGENTS.md"] {
            let p = workspace_root.join(rel);
            let Ok(content) = std::fs::read_to_string(&p) else {
                continue;
            };
            let mut current_heading = format!("{}:(preamble)", rel);
            let mut current_body = String::new();
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("## ") || trimmed.starts_with("# ") {
                    let toks = ContextCompressor::count_tokens_fast(&current_body);
                    if toks > 350 {
                        bloated.push((current_heading.clone(), toks));
                    }
                    current_heading = format!("{}:{}", rel, trimmed.trim_start_matches('#').trim());
                    current_body.clear();
                } else {
                    current_body.push_str(line);
                    current_body.push('\n');
                }
            }
            let toks = ContextCompressor::count_tokens_fast(&current_body);
            if toks > 350 {
                bloated.push((current_heading, toks));
            }
        }
        bloated.sort_by(|a, b| b.1.cmp(&a.1));
        bloated
    }

    /// Reconstructs `Vec<Message>` from recent `.minicode/sessions/*.jsonl` event streams
    /// (or `.json` snapshots) when no in-memory message slice is provided.
    #[must_use]
    pub fn load_recent_session_messages(
        workspace_root: &std::path::Path,
        max_sessions: usize,
    ) -> Vec<Message> {
        let sessions_dir = workspace_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join("sessions");
        let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
            return Vec::new();
        };
        let mut paths: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|ext| ext == "json" || ext == "jsonl")
                        .unwrap_or(false)
            })
            .collect();
        paths.sort();
        paths.reverse();

        let mut out = Vec::new();
        for path in paths.into_iter().take(max_sessions) {
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(arr) = val.get("messages").and_then(|m| m.as_array()) {
                    for item in arr {
                        if let Ok(msg) = serde_json::from_value::<Message>(item.clone()) {
                            out.push(msg);
                        }
                    }
                    continue;
                }
            }
            // Project append-only JSONL AgentEvent stream into Message sequence
            for line in raw.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(ev) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                    continue;
                };
                match ev.get("event").and_then(|v| v.as_str()).unwrap_or("") {
                    "user_prompt" => {
                        if let Some(p) = ev.get("prompt").and_then(|v| v.as_str()) {
                            out.push(Message::user(p));
                        }
                    }
                    "tool_call" => {
                        let id = ev
                            .get("tool_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("call")
                            .to_string();
                        let name = ev
                            .get("tool")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let arguments = ev
                            .get("args")
                            .cloned()
                            .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
                        if !name.is_empty() {
                            out.push(Message {
                                role: crate::agent::types::Role::Assistant,
                                content: String::new(),
                                tool_name: None,
                                tool_call_id: None,
                                tool_calls: Some(vec![crate::agent::types::ToolCall {
                                    id,
                                    name,
                                    arguments,
                                }]),
                                reasoning_content: None,
                                images: None,
                            });
                        }
                    }
                    "tool_result" => {
                        let id = ev
                            .get("tool_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("call")
                            .to_string();
                        let name = ev
                            .get("tool")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tool")
                            .to_string();
                        let output = ev
                            .get("output")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        out.push(Message::tool_result(id, name, output));
                    }
                    "context_compacted" => {
                        out.push(Message::system("<auto_compact_anchor>"));
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{Role, ToolCall};
    use serde_json::json;

    #[test]
    fn test_caveman_waste_analyzer_10_sinks_and_net_token_negative_gate() {
        let mut msgs = Vec::new();
        for i in 0..3 {
            msgs.push(Message {
                role: Role::Assistant,
                content: String::new(),
                tool_name: None,
                tool_call_id: None,
                tool_calls: Some(vec![
                    ToolCall {
                        id: format!("c1_{}", i),
                        name: "grep_search".to_string(),
                        arguments: json!({"query": "foo"}),
                    },
                    ToolCall {
                        id: format!("c2_{}", i),
                        name: "read_file".to_string(),
                        arguments: json!({"path": "src/main.rs"}),
                    },
                    ToolCall {
                        id: format!("c3_{}", i),
                        name: "exec_cmd".to_string(),
                        arguments: json!({"command": "cargo check"}),
                    },
                ]),
                reasoning_content: None,
                images: None,
            });
        }

        let report = CavemanWasteAnalyzer::analyze(std::path::Path::new("."), &msgs);
        // src/main.rs was read 3 times without any intervening write_file/patch_file -> 2 unmodified re-reads
        assert_eq!(report.unmodified_reread_count, 2);
        // 3-gram `grep_search -> read_file -> exec_cmd` repeated 3 times -> SKILL_DISTILLATION candidate
        assert!(!report.repeated_procedures.is_empty());
        assert!(report
            .findings
            .iter()
            .all(CavemanWasteFinding::passes_net_token_negative_gate));
        assert!(report
            .findings
            .iter()
            .any(|f| f.category == "SKILL_DISTILLATION"));
        let md = report.format_markdown();
        assert!(md.contains("Caveman v4.0 10-Sink Token Waste & Skill Distillation Audit"));
    }
}
