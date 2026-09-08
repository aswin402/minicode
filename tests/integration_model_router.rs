use minicode::agent::router::{AdaptiveModelRouter, ComplexityAnalyzer, ModelTier};
use serde_json::json;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_router_complexity_assessment_fast_tier() {
    let assessment =
        ComplexityAnalyzer::analyze("grep for SymbolIndex in src/context", 2, false, 0, 0);
    assert_eq!(assessment.recommended_tier, ModelTier::Fast);
    assert!(assessment.score < 0.35);
    assert_eq!(assessment.recommended_tier.relative_cost_factor(), 0.15);
    assert_eq!(assessment.recommended_tier.badge(), "⚡ Fast");
}

#[test]
fn test_router_complexity_assessment_deep_reasoning() {
    let assessment = ComplexityAnalyzer::analyze(
        "Architect and refactor concurrent transaction journal to eliminate deadlock risks",
        15,
        false,
        0,
        3,
    );
    assert_eq!(assessment.recommended_tier, ModelTier::DeepReasoning);
    assert!(assessment.score >= 0.70);
    assert_eq!(assessment.recommended_tier.relative_cost_factor(), 4.0);
    assert_eq!(assessment.recommended_tier.badge(), "🧠 Deep Reasoning");
}

#[test]
fn test_router_fallback_chain_construction() {
    let mut router = AdaptiveModelRouter::new();

    let decision = router.route_turn(
        "implement comprehensive unit test suite covering configuration parsing and environment overrides",
        5,
        false,
        0,
        1,
    );
    assert_eq!(decision.tier, ModelTier::Standard);
    assert_eq!(decision.primary_endpoint.provider_name, "anthropic");
    assert!(!decision.fallback_chain.is_empty());

    // Fallbacks should include secondary options
    let fallback_names: Vec<&str> = decision
        .fallback_chain
        .iter()
        .map(|e| e.provider_name.as_str())
        .collect();
    assert!(fallback_names.contains(&"gemini"));
    assert!(fallback_names.contains(&"openai"));
}

#[test]
fn test_router_health_circuit_breaker() {
    let mut router = AdaptiveModelRouter::new();
    router.set_cooldown_duration(Duration::from_secs(45));

    // Initially Anthropic is primary and healthy
    assert!(router.is_provider_available("anthropic"));
    let d1 = router.route_turn("write helper function", 2, false, 0, 0);
    assert_eq!(d1.primary_endpoint.provider_name, "anthropic");

    // Trigger HTTP 429 rate limit error
    router.record_provider_error("anthropic", true);
    assert!(!router.is_provider_available("anthropic"));

    // Next turn should failover away from Anthropic to next best provider
    let d2 = router.route_turn("write helper function", 2, false, 0, 0);
    assert_ne!(d2.primary_endpoint.provider_name, "anthropic");
    assert_eq!(d2.primary_endpoint.provider_name, "gemini");

    // Fallback telemetry recorded
    assert_eq!(router.telemetry().fallback_events, 1);
}

#[test]
fn test_router_manual_tier_override_and_status() {
    let mut router = AdaptiveModelRouter::new();

    // Lock to Fast tier
    router.set_forced_tier(Some(ModelTier::Fast));
    assert_eq!(router.get_forced_tier(), Some(ModelTier::Fast));

    let decision = router.route_turn(
        "Architect a multi-file refactor resolving race conditions",
        20,
        true,
        2,
        5,
    );
    assert_eq!(decision.tier, ModelTier::Fast);

    let status = router.format_status_report();
    assert!(status.contains("Manual Lock"));
    assert!(status.contains("⚡ Fast"));
    assert!(status.contains("Provider Health Status"));

    // Clear override
    router.set_forced_tier(None);
    assert_eq!(router.get_forced_tier(), None);
}

#[tokio::test]
async fn test_tool_registry_route_model_dispatch() {
    let dir = tempdir().expect("tempdir");

    // 1. Assess complexity
    let assess_args = json!({
        "task_description": "grep for CodeGraph in src/context",
        "query_type": "assess"
    });
    let result =
        minicode::tools::registry::agent_tools::dispatch("route_model", &assess_args, dir.path())
            .await
            .expect("route_model registered")
            .expect("dispatch success");

    assert!(result.contains("Model Routing Assessment"));
    assert!(result.contains("⚡ Fast"));
    assert!(result.contains("Primary Endpoint"));

    // 2. Status report
    let status_args = json!({
        "query_type": "status"
    });
    let status_res =
        minicode::tools::registry::agent_tools::dispatch("route_model", &status_args, dir.path())
            .await
            .expect("route_model registered")
            .expect("dispatch success");

    assert!(status_res.contains("Adaptive Model Router & Fallback Status"));
    assert!(status_res.contains("Routing Telemetry"));

    // 3. Override tier
    let override_args = json!({
        "force_tier": "deep",
        "query_type": "override"
    });
    let override_res =
        minicode::tools::registry::agent_tools::dispatch("route_model", &override_args, dir.path())
            .await
            .expect("route_model registered")
            .expect("dispatch success");

    assert!(override_res.contains("Router locked to 🧠 Deep Reasoning tier"));
}
