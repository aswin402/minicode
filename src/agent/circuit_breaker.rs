use crate::error::{ProviderError, Result};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// State of the Circuit Breaker protecting against upstream provider network failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CircuitState {
    Closed,   // Normal operation: requests pass through
    Open,     // Tripped: requests fail fast without calling provider
    HalfOpen, // Testing: single canary request allowed to probe recovery
}

/// Configuration for the Circuit Breaker.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub cooldown_duration: Duration,
    pub half_open_success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            cooldown_duration: Duration::from_secs(10),
            half_open_success_threshold: 2,
        }
    }
}

/// Thread-safe Network Circuit Breaker for LLM API streams and HTTP requests.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    consecutive_failures: Arc<AtomicU32>,
    consecutive_successes: Arc<AtomicU32>,
    last_failure_timestamp_ms: Arc<AtomicU64>,
}

#[allow(dead_code)]
impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            consecutive_successes: Arc::new(AtomicU32::new(0)),
            last_failure_timestamp_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns current state of the circuit breaker
    pub fn state(&self) -> CircuitState {
        let failures = self.consecutive_failures.load(Ordering::Relaxed);
        if failures < self.config.failure_threshold {
            return CircuitState::Closed;
        }

        let now_ms = Self::current_time_ms();
        let last_fail = self.last_failure_timestamp_ms.load(Ordering::Relaxed);
        let elapsed = Duration::from_millis(now_ms.saturating_sub(last_fail));

        if elapsed >= self.config.cooldown_duration {
            CircuitState::HalfOpen
        } else {
            CircuitState::Open
        }
    }

    /// Checks if a request is permitted to proceed
    pub fn can_execute(&self) -> Result<()> {
        match self.state() {
            CircuitState::Closed | CircuitState::HalfOpen => Ok(()),
            CircuitState::Open => Err(ProviderError::CircuitOpen(
                "Circuit breaker is OPEN. Upstream provider is temporarily unreachable; failing fast to conserve resources."
                    .to_string(),
            )
            .into()),
        }
    }

    /// Records a successful request, resetting failures or transitioning from HalfOpen to Closed
    pub fn record_success(&self) {
        let state = self.state();
        match state {
            CircuitState::HalfOpen => {
                let succ = self.consecutive_successes.fetch_add(1, Ordering::Relaxed) + 1;
                if succ >= self.config.half_open_success_threshold {
                    self.consecutive_failures.store(0, Ordering::Relaxed);
                    self.consecutive_successes.store(0, Ordering::Relaxed);
                }
            }
            CircuitState::Closed => {
                self.consecutive_failures.store(0, Ordering::Relaxed);
            }
            CircuitState::Open => {}
        }
    }

    /// Records a request failure, tripping the breaker if threshold reached
    pub fn record_failure(&self) {
        self.consecutive_successes.store(0, Ordering::Relaxed);
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        self.last_failure_timestamp_ms
            .store(Self::current_time_ms(), Ordering::Relaxed);
    }

    fn current_time_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Exponential backoff and retry policy for transient LLM errors.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RetryPolicy {
    pub max_retries: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(400),
            max_delay: Duration::from_secs(5),
            multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Determines whether an error is transient and safe to retry.
    pub fn is_retryable(error: &crate::error::MinicodeError) -> bool {
        match error {
            crate::error::MinicodeError::Provider(pe) => match pe {
                ProviderError::RateLimited { .. } => true,
                ProviderError::Api { status, .. } => {
                    *status == 429 || (*status >= 500 && *status <= 599)
                }
                ProviderError::Http(_) => true,
                ProviderError::Network(_) => true,
                ProviderError::StreamDecode(_) => true,
                _ => false,
            },
            _ => false,
        }
    }

    /// Calculates backoff delay for the given attempt index (0-indexed).
    pub fn delay_for_attempt(&self, attempt: usize) -> Duration {
        if attempt == 0 {
            return self.initial_delay;
        }

        let factor = self.multiplier.powi(attempt as i32);
        let millis = (self.initial_delay.as_millis() as f64 * factor) as u64;
        let dur = Duration::from_millis(millis);

        dur.min(self.max_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_transitions() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            cooldown_duration: Duration::from_millis(50),
            half_open_success_threshold: 1,
        };

        let cb = CircuitBreaker::new(config);
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.can_execute().is_ok());

        // First failure
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        // Second failure -> Trips to Open
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(cb.can_execute().is_err());

        // Wait for cooldown
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        assert!(cb.can_execute().is_ok());

        // Success in HalfOpen -> Recovers to Closed
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.can_execute().is_ok());
    }

    #[test]
    fn test_retry_policy_delays() {
        let policy = RetryPolicy::default();
        let d0 = policy.delay_for_attempt(0);
        let d1 = policy.delay_for_attempt(1);
        let d2 = policy.delay_for_attempt(2);

        assert_eq!(d0, Duration::from_millis(400));
        assert_eq!(d1, Duration::from_millis(800));
        assert_eq!(d2, Duration::from_millis(1600));
    }
}
