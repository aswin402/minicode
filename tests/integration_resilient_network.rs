/// Integration tests for Phase 40: Milestone v0.0.50 — Resilient Stream Re-Connection & Network Circuit Breaker
///
/// Tests circuit breaker state transitions, exponential backoff delays,
/// retryable error classification, and ResilientProvider integration.
use minicode::agent::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitState, RetryPolicy,
};
use minicode::agent::mock_provider::MockProvider;
use minicode::agent::provider::{CompletionOptions, Provider, ResilientProvider};
use minicode::agent::types::Message;
use minicode::error::{MinicodeError, ProviderError};
use std::time::Duration;

#[test]
fn test_circuit_breaker_trip_and_cooldown() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        cooldown_duration: Duration::from_millis(50),
        half_open_success_threshold: 2,
    };

    let cb = CircuitBreaker::new(config);
    assert_eq!(cb.state(), CircuitState::Closed);
    assert!(cb.can_execute().is_ok());

    // Fail twice: still closed
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Closed);

    // Fail 3rd time: trips to Open
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);
    assert!(cb.can_execute().is_err());

    // Cooldown elapses: transitions to HalfOpen
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(cb.state(), CircuitState::HalfOpen);
    assert!(cb.can_execute().is_ok());

    // 1st success in HalfOpen
    cb.record_success();
    assert_eq!(cb.state(), CircuitState::HalfOpen);

    // 2nd success in HalfOpen -> transitions to Closed
    cb.record_success();
    assert_eq!(cb.state(), CircuitState::Closed);
    assert!(cb.can_execute().is_ok());
}

#[test]
fn test_retry_policy_retryable_classification() {
    let rate_limit_err = MinicodeError::Provider(ProviderError::RateLimited {
        retry_after_secs: Some(5),
    });
    assert!(RetryPolicy::is_retryable(&rate_limit_err));

    let server_err = MinicodeError::Provider(ProviderError::Api {
        status: 503,
        message: "Service Unavailable".to_string(),
    });
    assert!(RetryPolicy::is_retryable(&server_err));

    let model_err = MinicodeError::Provider(ProviderError::UnsupportedModel {
        model: "invalid".to_string(),
        provider: "gemini".to_string(),
    });
    assert!(!RetryPolicy::is_retryable(&model_err));
}

#[tokio::test]
async fn test_resilient_provider_success_flow() {
    let mock = MockProvider::new("mock", "mock-model");
    mock.push_response(&["Hello from resilient stream!"], vec![]);
    let resilient = ResilientProvider::new(mock);

    let messages = vec![Message::user("Hi")];
    let options = CompletionOptions {
        model: "mock-model".to_string(),
        temperature: 0.7,
        max_tokens: 1000,
        system_instruction: None,
    };

    let result = resilient.stream_completion(&messages, &[], &options).await;
    assert!(result.is_ok());
    assert_eq!(resilient.circuit_breaker().state(), CircuitState::Closed);
}
