use crate::agent::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, RetryPolicy};
use crate::agent::provider::{ChunkStream, CompletionOptions, Provider, ToolSchema};
use crate::agent::types::Message;
use crate::error::{ProviderError, Result};
use async_trait::async_trait;

/// A resilient provider decorator that wraps any LLM Provider with
/// an automated circuit breaker and exponential backoff retry policy.
#[allow(dead_code)]
pub struct ResilientProvider<P: Provider> {
    inner: P,
    circuit_breaker: CircuitBreaker,
    retry_policy: RetryPolicy,
}

#[allow(dead_code)]
impl<P: Provider> ResilientProvider<P> {
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            circuit_breaker: CircuitBreaker::new(CircuitBreakerConfig::default()),
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn with_config(
        inner: P,
        cb_config: CircuitBreakerConfig,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            inner,
            circuit_breaker: CircuitBreaker::new(cb_config),
            retry_policy,
        }
    }

    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }
}

#[async_trait]
impl<P: Provider> Provider for ResilientProvider<P> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn default_model(&self) -> &str {
        self.inner.default_model()
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        self.circuit_breaker.can_execute()?;

        let mut last_error = None;
        for attempt in 0..=self.retry_policy.max_retries {
            if attempt > 0 {
                let delay = self.retry_policy.delay_for_attempt(attempt);
                tracing::warn!(
                    attempt = attempt,
                    delay_ms = delay.as_millis(),
                    "Retrying LLM stream completion with exponential backoff..."
                );
                tokio::time::sleep(delay).await;
            }

            match self.inner.stream_completion(messages, tools, options).await {
                Ok(stream) => {
                    self.circuit_breaker.record_success();
                    return Ok(stream);
                }
                Err(err) => {
                    self.circuit_breaker.record_failure();
                    if !RetryPolicy::is_retryable(&err) {
                        return Err(err);
                    }
                    last_error = Some(err);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ProviderError::Network("All stream completion retry attempts failed".to_string()).into()
        }))
    }
}
