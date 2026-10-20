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

        let mut current_options = options.clone();
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

            match self
                .inner
                .stream_completion(messages, tools, &current_options)
                .await
            {
                Ok(stream) => {
                    self.circuit_breaker.record_success();
                    return Ok(stream);
                }
                Err(err) => {
                    self.circuit_breaker.record_failure();

                    // Check if error is due to Model Not Found / 404 / Decommissioned
                    let err_str = err.to_string().to_lowercase();
                    if err_str.contains("404")
                        || err_str.contains("not found")
                        || err_str.contains("model not found")
                        || err_str.contains("does not exist")
                        || err_str.contains("unsupported model")
                    {
                        if let Some(next_model) =
                            crate::agent::models::DynamicModelResolver::resolve_next_fallback(
                                self.inner.name(),
                                &current_options.model,
                                crate::agent::models::SemanticTier::Auto,
                            )
                        {
                            tracing::warn!(
                                provider = %self.inner.name(),
                                failed_model = %current_options.model,
                                fallback_model = %next_model,
                                "LLM model returned 404/Not Found; dynamically failing over to next available model"
                            );
                            current_options.model = next_model;
                            continue;
                        }
                    }

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

/// One slot in the 4-slot failover chain (`Primary -> 1st Fallback -> 2nd Fallback -> Global Last Fallback ★`).
pub struct FallbackTarget {
    pub slot_label: String,
    pub provider_name: String,
    pub model: String,
    pub provider: Box<dyn Provider>,
}

/// Multi-provider, multi-model failover chain decorator implementing `Provider`.
/// Automatically routes requests across up to 4 slots:
/// `[1] Primary -> [2] 1st Fallback -> [3] 2nd Fallback -> [4] Global Last Fallback ★`.
/// When a slot fails to connect or yields an initial HTTP/API/RateLimit/Timeout error before emitting tokens,
/// `FallbackChainProvider` transparently switches to the next configured slot (even across different providers).
pub struct FallbackChainProvider {
    targets: Vec<FallbackTarget>,
    active_idx: std::sync::atomic::AtomicUsize,
}

impl FallbackChainProvider {
    #[must_use]
    pub fn new(targets: Vec<FallbackTarget>) -> Self {
        Self {
            targets,
            active_idx: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    #[must_use]
    pub fn targets(&self) -> &[FallbackTarget] {
        &self.targets
    }

    #[must_use]
    pub fn active_target(&self) -> Option<&FallbackTarget> {
        let idx = self
            .active_idx
            .load(std::sync::atomic::Ordering::Relaxed)
            .min(self.targets.len().saturating_sub(1));
        self.targets.get(idx)
    }
}

#[async_trait]
impl Provider for FallbackChainProvider {
    fn name(&self) -> &str {
        self.active_target()
            .map(|t| t.provider.name())
            .unwrap_or("unconfigured")
    }

    fn default_model(&self) -> &str {
        self.active_target()
            .map(|t| {
                if !t.model.is_empty() {
                    t.model.as_str()
                } else {
                    t.provider.default_model()
                }
            })
            .unwrap_or("default-model")
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        use futures::StreamExt;

        if self.targets.is_empty() {
            return Err(
                ProviderError::Network("No LLM provider slots configured".to_string()).into(),
            );
        }

        // Single-slot fast path (no fallback slots configured)
        if self.targets.len() == 1 {
            let target = &self.targets[0];
            let mut slot_options = options.clone();
            if slot_options.model.trim().is_empty() && !target.model.is_empty() {
                slot_options.model = target.model.clone();
            }
            return target
                .provider
                .stream_completion(messages, tools, &slot_options)
                .await;
        }

        // Multi-slot failover path: start from the currently active slot (default 0 = Primary),
        // then walk remaining slots in deterministic order.
        let start_idx = self
            .active_idx
            .load(std::sync::atomic::Ordering::Relaxed)
            .min(self.targets.len().saturating_sub(1));
        let mut order = Vec::with_capacity(self.targets.len());
        order.push(start_idx);
        for idx in 0..self.targets.len() {
            if idx != start_idx {
                order.push(idx);
            }
        }

        let mut last_err: Option<crate::error::MinicodeError> = None;

        for (attempt_pos, &target_idx) in order.iter().enumerate() {
            let target = &self.targets[target_idx];
            let mut slot_options = options.clone();
            if target_idx == 0 && !options.model.trim().is_empty() {
                slot_options.model = options.model.clone();
            } else if !target.model.trim().is_empty() {
                slot_options.model = target.model.clone();
            }

            match target
                .provider
                .stream_completion(messages, tools, &slot_options)
                .await
            {
                Ok(mut stream) => {
                    // Peek the first chunk to verify the stream didn't immediately fail with 429/5xx/401/404/timeout
                    match stream.next().await {
                        Some(Ok(first_chunk)) => {
                            if target_idx != start_idx || attempt_pos > 0 {
                                tracing::info!(
                                    slot = %target.slot_label,
                                    provider = %target.provider_name,
                                    model = %slot_options.model,
                                    "Successfully failed over to fallback model slot"
                                );
                            }
                            self.active_idx
                                .store(target_idx, std::sync::atomic::Ordering::Relaxed);
                            let chained =
                                futures::stream::once(async move { Ok(first_chunk) }).chain(stream);
                            return Ok(Box::pin(chained));
                        }
                        None => {
                            self.active_idx
                                .store(target_idx, std::sync::atomic::Ordering::Relaxed);
                            return Ok(Box::pin(futures::stream::empty()));
                        }
                        Some(Err(first_err)) => {
                            tracing::warn!(
                                failed_slot = %target.slot_label,
                                failed_provider = %target.provider_name,
                                failed_model = %slot_options.model,
                                error = %first_err,
                                remaining_fallbacks = self.targets.len().saturating_sub(attempt_pos + 1),
                                "Model slot failed on initial stream chunk; switching to next fallback slot"
                            );
                            last_err = Some(first_err);
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        failed_slot = %target.slot_label,
                        failed_provider = %target.provider_name,
                        failed_model = %slot_options.model,
                        error = %err,
                        remaining_fallbacks = self.targets.len().saturating_sub(attempt_pos + 1),
                        "Model slot failed to initialize stream; switching to next fallback slot"
                    );
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            ProviderError::Network("All 4 model/fallback slots failed".to_string()).into()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::provider::StreamChunk;
    use futures::StreamExt;

    struct MockSlotProvider {
        name: &'static str,
        model: &'static str,
        fail_first_chunk: bool,
        response_text: &'static str,
    }

    #[async_trait]
    impl Provider for MockSlotProvider {
        fn name(&self) -> &str {
            self.name
        }

        fn default_model(&self) -> &str {
            self.model
        }

        async fn stream_completion(
            &self,
            _messages: &[Message],
            _tools: &[ToolSchema],
            options: &CompletionOptions,
        ) -> Result<ChunkStream> {
            if self.fail_first_chunk {
                let s = futures::stream::once(async move {
                    Err(ProviderError::Network("HTTP 429 Too Many Requests".to_string()).into())
                });
                return Ok(Box::pin(s));
            }
            let text = format!("{}:{}", options.model, self.response_text);
            let s = futures::stream::once(async move { Ok(StreamChunk::Delta(text)) });
            Ok(Box::pin(s))
        }
    }

    #[tokio::test]
    async fn test_fallback_chain_provider_fails_over_across_4_slots() {
        let targets = vec![
            FallbackTarget {
                slot_label: "Primary".to_string(),
                provider_name: "anthropic".to_string(),
                model: "claude-3-7-sonnet".to_string(),
                provider: Box::new(MockSlotProvider {
                    name: "anthropic",
                    model: "claude-3-7-sonnet",
                    fail_first_chunk: true,
                    response_text: "from-primary",
                }),
            },
            FallbackTarget {
                slot_label: "1st Fallback".to_string(),
                provider_name: "openai".to_string(),
                model: "gpt-4o".to_string(),
                provider: Box::new(MockSlotProvider {
                    name: "openai",
                    model: "gpt-4o",
                    fail_first_chunk: true,
                    response_text: "from-fb1",
                }),
            },
            FallbackTarget {
                slot_label: "2nd Fallback".to_string(),
                provider_name: "gemini".to_string(),
                model: "gemini-2.5-pro".to_string(),
                provider: Box::new(MockSlotProvider {
                    name: "gemini",
                    model: "gemini-2.5-pro",
                    fail_first_chunk: false,
                    response_text: "from-fb2",
                }),
            },
            FallbackTarget {
                slot_label: "Global Last Fallback ★".to_string(),
                provider_name: "ollama".to_string(),
                model: "qwen2.5-coder:7b".to_string(),
                provider: Box::new(MockSlotProvider {
                    name: "ollama",
                    model: "qwen2.5-coder:7b",
                    fail_first_chunk: false,
                    response_text: "from-global-last",
                }),
            },
        ];

        let chain = FallbackChainProvider::new(targets);
        let opts = CompletionOptions::new("claude-3-7-sonnet");
        let mut stream = chain.stream_completion(&[], &[], &opts).await.unwrap();
        let first = stream.next().await.unwrap().unwrap();
        match first {
            StreamChunk::Delta(t) => assert_eq!(t, "gemini-2.5-pro:from-fb2"),
            other => panic!("Unexpected chunk: {:?}", other),
        }
        assert_eq!(chain.name(), "gemini");
        assert_eq!(chain.default_model(), "gemini-2.5-pro");
    }
}
