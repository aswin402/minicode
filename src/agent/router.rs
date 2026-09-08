use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Model capabilities and cost tier classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    /// Low-latency, economical models for simple queries, grep triage, short edits
    Fast,
    /// Balanced standard models for general coding and tool iteration
    Standard,
    /// High-compute models with extended thinking/reasoning for complex plans & refactors
    DeepReasoning,
}

impl ModelTier {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Fast => "⚡ Fast",
            Self::Standard => "⚖️ Standard",
            Self::DeepReasoning => "🧠 Deep Reasoning",
        }
    }

    #[allow(dead_code)]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Fast => "High speed, economical tokens (e.g. Haiku / 4o-mini / Flash)",
            Self::Standard => "Balanced generalist coding and tool dispatch (e.g. Sonnet / GPT-4o)",
            Self::DeepReasoning => {
                "Frontier reasoning with extended thinking (e.g. Claude 3.7 / O3 / R1)"
            }
        }
    }

    pub fn relative_cost_factor(&self) -> f32 {
        match self {
            Self::Fast => 0.15,
            Self::Standard => 1.0,
            Self::DeepReasoning => 4.0,
        }
    }

    pub fn parse_tier(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "fast" | "quick" | "haiku" | "mini" => Some(Self::Fast),
            "standard" | "default" | "normal" | "sonnet" => Some(Self::Standard),
            "deep" | "reasoning" | "thinking" | "deep_reasoning" | "o3" | "r1" => {
                Some(Self::DeepReasoning)
            }
            _ => None,
        }
    }
}

impl std::str::FromStr for ModelTier {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::parse_tier(s).ok_or(())
    }
}

/// Dynamic complexity assessment score and factors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComplexityAssessment {
    pub score: f32,
    pub recommended_tier: ModelTier,
    pub factors: Vec<String>,
    pub estimated_tokens: usize,
}

/// Provider health tracking for circuit-breaking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProviderHealthStatus {
    Healthy,
    Degraded { consecutive_errors: usize },
    CoolingOff { retry_after_secs: u64 },
}

/// A specific model provider endpoint in a tier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderEndpoint {
    pub provider_name: String,
    pub model_name: String,
    pub tier: ModelTier,
    pub priority: usize,
    pub max_context: usize,
    pub is_local: bool,
}

/// Dynamic routing decision for a turn
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteDecision {
    pub tier: ModelTier,
    pub primary_endpoint: ProviderEndpoint,
    pub fallback_chain: Vec<ProviderEndpoint>,
    pub reason: String,
    pub estimated_cost_factor: f32,
}

/// Cumulative routing telemetry and cost savings
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RouterTelemetry {
    pub total_turns_routed: usize,
    pub fast_tier_turns: usize,
    pub standard_tier_turns: usize,
    pub deep_reasoning_turns: usize,
    pub fallback_events: usize,
    pub estimated_tokens_diverted_to_fast: usize,
}

/// Turn complexity analyzer evaluating heuristic factors
pub struct ComplexityAnalyzer;

impl ComplexityAnalyzer {
    /// Analyzes turn inputs to produce an objective complexity score (0.0 to 1.0)
    pub fn analyze(
        prompt: &str,
        history_len: usize,
        error_recovery_active: bool,
        recent_failures: usize,
        files_modified_in_session: usize,
    ) -> ComplexityAssessment {
        let mut factors = Vec::new();
        let prompt_lower = prompt.to_lowercase();
        let word_count = prompt.split_whitespace().count();
        let estimated_tokens = (word_count as f32 * 1.35) as usize + (history_len * 450);

        let mut score: f32 = 0.40; // Base default (standard)

        // 1. High-complexity architectural keywords
        let complex_terms = [
            "architect",
            "refactor",
            "redesign",
            "deadlock",
            "concurrency",
            "memory leak",
            "race condition",
            "migration",
            "transaction",
            "rollback",
            "dag",
            "security audit",
            "threat",
            "formal verification",
        ];

        let mut matched_complex = Vec::new();
        for term in &complex_terms {
            if prompt_lower.contains(term) {
                matched_complex.push(*term);
            }
        }

        if !matched_complex.is_empty() {
            let boost = if matched_complex.len() >= 2 {
                0.35
            } else {
                0.30
            };
            score += boost;
            factors.push(format!(
                "High-complexity terms ({}) (+{:.2})",
                matched_complex.join(", "),
                boost
            ));
        } else {
            // Brevity penalties only apply when no complex terms are detected
            let is_read_only = prompt_lower.starts_with("grep")
                || prompt_lower.starts_with("find")
                || prompt_lower.starts_with("where is")
                || prompt_lower.starts_with("check")
                || prompt_lower.starts_with("status")
                || prompt_lower.starts_with("list");

            if word_count < 20 && is_read_only {
                score -= 0.25;
                factors.push("Brevity and simple read-only intent (-0.25)".to_string());
            } else if word_count < 10 {
                score -= 0.15;
                factors.push("Very short command (-0.15)".to_string());
            }
        }

        // 3. Error recovery or self-healing state
        if error_recovery_active || recent_failures > 0 {
            score += 0.30;
            factors.push(format!(
                "Active error recovery / recent failures ({}) (+0.30)",
                recent_failures
            ));
        }

        // 4. Session footprint / modified files
        if files_modified_in_session >= 4 {
            score += 0.20;
            factors.push(format!(
                "Large multi-file footprint ({} modified) (+0.20)",
                files_modified_in_session
            ));
        } else if files_modified_in_session >= 2 {
            score += 0.10;
            factors.push(format!(
                "Multi-file footprint ({} modified) (+0.10)",
                files_modified_in_session
            ));
        }

        // 5. Extended conversation history
        if history_len > 30 {
            score += 0.15;
            factors.push("Deep conversation history (>30 turns) (+0.15)".to_string());
        }

        // Clamp to [0.0, 1.0]
        score = score.clamp(0.0, 1.0);

        let recommended_tier = if score < 0.35 {
            ModelTier::Fast
        } else if score < 0.70 {
            ModelTier::Standard
        } else {
            ModelTier::DeepReasoning
        };

        ComplexityAssessment {
            score,
            recommended_tier,
            factors,
            estimated_tokens,
        }
    }
}

/// Adaptive model context router and multi-provider fallback cascade engine
pub struct AdaptiveModelRouter {
    endpoints: HashMap<ModelTier, Vec<ProviderEndpoint>>,
    forced_tier: Option<ModelTier>,
    health_map: HashMap<String, (ProviderHealthStatus, Option<Instant>)>,
    telemetry: RouterTelemetry,
    #[allow(dead_code)]
    cooldown_duration: Duration,
}

impl Default for AdaptiveModelRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl AdaptiveModelRouter {
    /// Creates a new router with standard default provider endpoints
    pub fn new() -> Self {
        let mut router = Self {
            endpoints: HashMap::new(),
            forced_tier: None,
            health_map: HashMap::new(),
            telemetry: RouterTelemetry::default(),
            cooldown_duration: Duration::from_secs(60),
        };

        router.initialize_default_endpoints();
        router
    }

    /// Initializes default tiered endpoints across common providers
    fn initialize_default_endpoints(&mut self) {
        // --- Fast Tier ---
        self.register_endpoint(ProviderEndpoint {
            provider_name: "anthropic".to_string(),
            model_name: "claude-3-5-haiku-latest".to_string(),
            tier: ModelTier::Fast,
            priority: 10,
            max_context: 200_000,
            is_local: false,
        });
        self.register_endpoint(ProviderEndpoint {
            provider_name: "gemini".to_string(),
            model_name: "gemini-1.5-flash".to_string(),
            tier: ModelTier::Fast,
            priority: 20,
            max_context: 1_000_000,
            is_local: false,
        });
        self.register_endpoint(ProviderEndpoint {
            provider_name: "openai".to_string(),
            model_name: "gpt-4o-mini".to_string(),
            tier: ModelTier::Fast,
            priority: 30,
            max_context: 128_000,
            is_local: false,
        });
        self.register_endpoint(ProviderEndpoint {
            provider_name: "deepseek".to_string(),
            model_name: "deepseek-chat".to_string(),
            tier: ModelTier::Fast,
            priority: 40,
            max_context: 65_536,
            is_local: false,
        });
        self.register_endpoint(ProviderEndpoint {
            provider_name: "ollama".to_string(),
            model_name: "qwen2.5-coder:7b".to_string(),
            tier: ModelTier::Fast,
            priority: 50,
            max_context: 32_768,
            is_local: true,
        });

        // --- Standard Tier ---
        self.register_endpoint(ProviderEndpoint {
            provider_name: "anthropic".to_string(),
            model_name: "claude-3-5-sonnet-latest".to_string(),
            tier: ModelTier::Standard,
            priority: 10,
            max_context: 200_000,
            is_local: false,
        });
        self.register_endpoint(ProviderEndpoint {
            provider_name: "gemini".to_string(),
            model_name: "gemini-2.0-flash".to_string(),
            tier: ModelTier::Standard,
            priority: 20,
            max_context: 1_000_000,
            is_local: false,
        });
        self.register_endpoint(ProviderEndpoint {
            provider_name: "openai".to_string(),
            model_name: "gpt-4o".to_string(),
            tier: ModelTier::Standard,
            priority: 30,
            max_context: 128_000,
            is_local: false,
        });
        self.register_endpoint(ProviderEndpoint {
            provider_name: "openrouter".to_string(),
            model_name: "anthropic/claude-3.5-sonnet".to_string(),
            tier: ModelTier::Standard,
            priority: 40,
            max_context: 200_000,
            is_local: false,
        });

        // --- Deep Reasoning Tier ---
        self.register_endpoint(ProviderEndpoint {
            provider_name: "anthropic".to_string(),
            model_name: "claude-3-7-sonnet-latest".to_string(),
            tier: ModelTier::DeepReasoning,
            priority: 10,
            max_context: 200_000,
            is_local: false,
        });
        self.register_endpoint(ProviderEndpoint {
            provider_name: "openai".to_string(),
            model_name: "o3-mini".to_string(),
            tier: ModelTier::DeepReasoning,
            priority: 20,
            max_context: 128_000,
            is_local: false,
        });
        self.register_endpoint(ProviderEndpoint {
            provider_name: "deepseek".to_string(),
            model_name: "deepseek-reasoner".to_string(),
            tier: ModelTier::DeepReasoning,
            priority: 30,
            max_context: 65_536,
            is_local: false,
        });
    }

    /// Registers a custom provider endpoint
    pub fn register_endpoint(&mut self, endpoint: ProviderEndpoint) {
        let list = self.endpoints.entry(endpoint.tier).or_default();
        // Replace existing endpoint with same provider and tier if present
        if let Some(pos) = list
            .iter()
            .position(|e| e.provider_name == endpoint.provider_name)
        {
            list[pos] = endpoint;
        } else {
            list.push(endpoint);
        }
        list.sort_by_key(|e| e.priority);
    }

    /// Sets or clears manual tier override
    pub fn set_forced_tier(&mut self, tier: Option<ModelTier>) {
        self.forced_tier = tier;
    }

    /// Gets current manual tier override, if any
    #[allow(dead_code)]
    pub fn get_forced_tier(&self) -> Option<ModelTier> {
        self.forced_tier
    }

    /// Sets custom circuit breaker cooldown duration
    #[allow(dead_code)]
    pub fn set_cooldown_duration(&mut self, duration: Duration) {
        self.cooldown_duration = duration;
    }

    /// Records successful completion from a provider, resetting error count
    #[allow(dead_code)]
    pub fn record_provider_success(&mut self, provider_name: &str) {
        self.health_map.insert(
            provider_name.to_string(),
            (ProviderHealthStatus::Healthy, None),
        );
    }

    /// Records an error from a provider, updating health state and cooldown
    #[allow(dead_code)]
    pub fn record_provider_error(&mut self, provider_name: &str, is_rate_limit_or_overload: bool) {
        let (current_status, _) = self
            .health_map
            .get(provider_name)
            .cloned()
            .unwrap_or((ProviderHealthStatus::Healthy, None));

        let new_status = if is_rate_limit_or_overload {
            self.telemetry.fallback_events += 1;
            ProviderHealthStatus::CoolingOff {
                retry_after_secs: self.cooldown_duration.as_secs(),
            }
        } else {
            match current_status {
                ProviderHealthStatus::Healthy => ProviderHealthStatus::Degraded {
                    consecutive_errors: 1,
                },
                ProviderHealthStatus::Degraded { consecutive_errors } => {
                    if consecutive_errors >= 2 {
                        self.telemetry.fallback_events += 1;
                        ProviderHealthStatus::CoolingOff {
                            retry_after_secs: self.cooldown_duration.as_secs(),
                        }
                    } else {
                        ProviderHealthStatus::Degraded {
                            consecutive_errors: consecutive_errors + 1,
                        }
                    }
                }
                ProviderHealthStatus::CoolingOff { retry_after_secs } => {
                    ProviderHealthStatus::CoolingOff { retry_after_secs }
                }
            }
        };

        let now = Instant::now();
        self.health_map
            .insert(provider_name.to_string(), (new_status, Some(now)));
    }

    /// Checks if a provider is currently available (not cooling off)
    pub fn is_provider_available(&self, provider_name: &str) -> bool {
        if let Some((status, Some(last_time))) = self.health_map.get(provider_name) {
            match status {
                ProviderHealthStatus::CoolingOff { retry_after_secs } => {
                    if last_time.elapsed() < Duration::from_secs(*retry_after_secs) {
                        return false;
                    }
                }
                _ => return true,
            }
        }
        true
    }

    /// Computes the optimal routing decision and fallback chain for a turn
    pub fn route_turn(
        &mut self,
        prompt: &str,
        history_len: usize,
        error_recovery_active: bool,
        recent_failures: usize,
        files_modified_in_session: usize,
    ) -> RouteDecision {
        let assessment = ComplexityAnalyzer::analyze(
            prompt,
            history_len,
            error_recovery_active,
            recent_failures,
            files_modified_in_session,
        );

        let selected_tier = self.forced_tier.unwrap_or(assessment.recommended_tier);

        let endpoints = self
            .endpoints
            .get(&selected_tier)
            .cloned()
            .unwrap_or_default();

        // Separate endpoints into available vs cooling-off
        let mut available: Vec<ProviderEndpoint> = Vec::new();
        let mut cooling: Vec<ProviderEndpoint> = Vec::new();

        for ep in endpoints {
            if self.is_provider_available(&ep.provider_name) {
                available.push(ep);
            } else {
                cooling.push(ep);
            }
        }

        // If no endpoints in current tier are available, pull from other tiers
        let candidate_pool = if !available.is_empty() {
            available
        } else if !cooling.is_empty() {
            // As a last resort, use cooling endpoints
            cooling
        } else {
            // Fallback to Standard or any available tier
            self.endpoints
                .get(&ModelTier::Standard)
                .cloned()
                .unwrap_or_else(|| {
                    vec![ProviderEndpoint {
                        provider_name: "default".to_string(),
                        model_name: "default".to_string(),
                        tier: selected_tier,
                        priority: 999,
                        max_context: 128_000,
                        is_local: false,
                    }]
                })
        };

        let primary_endpoint =
            candidate_pool
                .first()
                .cloned()
                .unwrap_or_else(|| ProviderEndpoint {
                    provider_name: "default".to_string(),
                    model_name: "default".to_string(),
                    tier: selected_tier,
                    priority: 999,
                    max_context: 128_000,
                    is_local: false,
                });

        let fallback_chain: Vec<ProviderEndpoint> =
            candidate_pool.iter().skip(1).cloned().collect();

        let reason = if self.forced_tier.is_some() {
            format!("Manual tier override: {}", selected_tier.badge())
        } else {
            format!(
                "Complexity score {:.2} ({} factors: {})",
                assessment.score,
                assessment.factors.len(),
                assessment.factors.join("; ")
            )
        };

        // Update telemetry
        self.telemetry.total_turns_routed += 1;
        match selected_tier {
            ModelTier::Fast => {
                self.telemetry.fast_tier_turns += 1;
                self.telemetry.estimated_tokens_diverted_to_fast += assessment.estimated_tokens;
            }
            ModelTier::Standard => self.telemetry.standard_tier_turns += 1,
            ModelTier::DeepReasoning => self.telemetry.deep_reasoning_turns += 1,
        }

        RouteDecision {
            tier: selected_tier,
            primary_endpoint,
            fallback_chain,
            reason,
            estimated_cost_factor: selected_tier.relative_cost_factor(),
        }
    }

    /// Gets current telemetry
    #[allow(dead_code)]
    pub fn telemetry(&self) -> &RouterTelemetry {
        &self.telemetry
    }

    /// Formats an informative markdown status report for TUI / LLM
    #[allow(dead_code)]
    pub fn format_status_report(&self) -> String {
        let mut out = String::new();
        out.push_str("# 🔀 Adaptive Model Router & Fallback Status\n\n");

        if let Some(forced) = self.forced_tier {
            out.push_str(&format!(
                "🔒 **Routing Mode:** Manual Lock ➔ `{}` ({})\n\n",
                forced.badge(),
                forced.description()
            ));
        } else {
            out.push_str("⚡ **Routing Mode:** Adaptive Auto-Routing (Complexity-Driven)\n\n");
        }

        out.push_str("### 📊 Routing Telemetry\n");
        out.push_str(&format!(
            "- **Total Routed Turns:** {}\n",
            self.telemetry.total_turns_routed
        ));
        out.push_str(&format!(
            "- **Fast Tier Turns:** {} (Tokens Diverted: ~{})\n",
            self.telemetry.fast_tier_turns, self.telemetry.estimated_tokens_diverted_to_fast
        ));
        out.push_str(&format!(
            "- **Standard Tier Turns:** {}\n",
            self.telemetry.standard_tier_turns
        ));
        out.push_str(&format!(
            "- **Deep Reasoning Turns:** {}\n",
            self.telemetry.deep_reasoning_turns
        ));
        out.push_str(&format!(
            "- **Fallback Failover Events:** {}\n\n",
            self.telemetry.fallback_events
        ));

        out.push_str("### 🏥 Provider Health Status\n");
        if self.health_map.is_empty() {
            out.push_str("All configured provider endpoints are healthy.\n\n");
        } else {
            for (p_name, (status, last_opt)) in &self.health_map {
                let badge = match status {
                    ProviderHealthStatus::Healthy => "🟢 Healthy",
                    ProviderHealthStatus::Degraded { consecutive_errors } => {
                        &format!("🟡 Degraded ({} errors)", consecutive_errors)
                    }
                    ProviderHealthStatus::CoolingOff { retry_after_secs } => {
                        let remaining = if let Some(last) = last_opt {
                            let el = last.elapsed().as_secs();
                            retry_after_secs.saturating_sub(el)
                        } else {
                            *retry_after_secs
                        };
                        &format!("🔴 Cooling Off ({}s remaining)", remaining)
                    }
                };
                out.push_str(&format!("- `{}`: {}\n", p_name, badge));
            }
            out.push('\n');
        }

        out.push_str("### 🎯 Configured Endpoints by Tier\n");
        for tier in [
            ModelTier::Fast,
            ModelTier::Standard,
            ModelTier::DeepReasoning,
        ] {
            out.push_str(&format!("#### {}\n", tier.badge()));
            if let Some(list) = self.endpoints.get(&tier) {
                for (idx, ep) in list.iter().enumerate() {
                    let avail = if self.is_provider_available(&ep.provider_name) {
                        "🟢"
                    } else {
                        "🔴"
                    };
                    out.push_str(&format!(
                        "{}. {} `{}` / `{}` (Priority: {}, Max Context: {}k{})\n",
                        idx + 1,
                        avail,
                        ep.provider_name,
                        ep.model_name,
                        ep.priority,
                        ep.max_context / 1000,
                        if ep.is_local { ", Local" } else { "" }
                    ));
                }
            } else {
                out.push_str("*(None configured)*\n");
            }
            out.push('\n');
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complexity_analyzer_read_only_fast_tier() {
        let assessment =
            ComplexityAnalyzer::analyze("grep for CodeGraph in src/context", 2, false, 0, 0);
        assert_eq!(assessment.recommended_tier, ModelTier::Fast);
        assert!(assessment.score < 0.35);
    }

    #[test]
    fn test_complexity_analyzer_architectural_deep_reasoning() {
        let assessment = ComplexityAnalyzer::analyze(
            "Architect a multi-file refactor resolving race conditions and deadlock",
            10,
            false,
            0,
            3,
        );
        assert_eq!(assessment.recommended_tier, ModelTier::DeepReasoning);
        assert!(assessment.score >= 0.70);
    }

    #[test]
    fn test_complexity_analyzer_error_recovery_escalation() {
        let assessment = ComplexityAnalyzer::analyze("verify the fix", 15, true, 2, 4);
        assert_eq!(assessment.recommended_tier, ModelTier::DeepReasoning);
    }

    #[test]
    fn test_router_forced_tier_override() {
        let mut router = AdaptiveModelRouter::new();
        router.set_forced_tier(Some(ModelTier::Fast));

        let decision = router.route_turn(
            "Architect a multi-file refactor resolving race conditions",
            10,
            true,
            2,
            5,
        );
        assert_eq!(decision.tier, ModelTier::Fast);
        assert!(decision.reason.contains("Manual tier override"));
    }

    #[test]
    fn test_router_circuit_breaker_and_fallback() {
        let mut router = AdaptiveModelRouter::new();
        router.set_cooldown_duration(Duration::from_secs(30));

        // Primary for Standard is Anthropic
        let d1 = router.route_turn("implement standard unit test", 5, false, 0, 1);
        assert_eq!(d1.primary_endpoint.provider_name, "anthropic");

        // Record rate limit error on Anthropic
        router.record_provider_error("anthropic", true);
        assert!(!router.is_provider_available("anthropic"));

        // Next route should failover to secondary provider (Gemini or OpenAI)
        let d2 = router.route_turn("implement standard unit test", 5, false, 0, 1);
        assert_ne!(d2.primary_endpoint.provider_name, "anthropic");
        assert_eq!(d2.primary_endpoint.provider_name, "gemini");

        // Telemetry should reflect 1 fallback event
        assert_eq!(router.telemetry().fallback_events, 1);
    }
}
