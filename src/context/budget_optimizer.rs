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
