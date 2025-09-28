use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::components::registry::Registry;
use crate::types::{HostRateLimitState, RateLimitConfig, RateLimitError};

/// Commands for the rate limiter service.
pub enum RateLimitCommand {
    AcquirePermit {
        host: String,
        response_tx: oneshot::Sender<Duration>,
    },
    RecordResponse {
        host: String,
        status: u16,
        retry_after: Option<Duration>,
    },
    Shutdown,
}

/// Rate limiting service that coordinates across multiple agents.
pub struct RateLimiterService<R> {
    command_tx: mpsc::UnboundedSender<RateLimitCommand>,
    handle: Option<JoinHandle<()>>,
    _phantom: PhantomData<R>,
}

impl<R: Registry + 'static> RateLimiterService<R> {
    pub fn new(registry: Arc<R>, config: RateLimitConfig) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let worker = RateLimiterWorker {
            registry,
            config,
            command_rx,
        };

        let handle = tokio::spawn(worker.run());

        Self {
            command_tx,
            handle: Some(handle),
            _phantom: PhantomData,
        }
    }

    pub async fn acquire_permit(&self, host: String) -> Result<Duration, RateLimitError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(RateLimitCommand::AcquirePermit { host, response_tx })
            .map_err(|_| RateLimitError::Communication)?;

        response_rx.await.map_err(|_| RateLimitError::Communication)
    }

    pub async fn record_response(
        &self,
        host: String,
        status: u16,
        retry_after: Option<Duration>,
    ) -> Result<(), RateLimitError> {
        self.command_tx
            .send(RateLimitCommand::RecordResponse {
                host,
                status,
                retry_after,
            })
            .map_err(|_| RateLimitError::Communication)?;

        Ok(())
    }
}

impl<R> Drop for RateLimiterService<R> {
    fn drop(&mut self) {
        let _ = self.command_tx.send(RateLimitCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

/// Internal worker that processes rate limiting commands.
struct RateLimiterWorker<R> {
    registry: Arc<R>,
    config: RateLimitConfig,
    command_rx: mpsc::UnboundedReceiver<RateLimitCommand>,
}

impl<R: Registry> RateLimiterWorker<R> {
    async fn run(mut self) {
        while let Some(command) = self.command_rx.recv().await {
            match command {
                RateLimitCommand::AcquirePermit { host, response_tx } => {
                    let delay = self.acquire_permit(&host).await;
                    let _ = response_tx.send(delay);
                }
                RateLimitCommand::RecordResponse {
                    host,
                    status,
                    retry_after,
                } => {
                    self.record_response(&host, status, retry_after).await;
                }
                RateLimitCommand::Shutdown => break,
            }
        }
    }

    async fn acquire_permit(&self, host: &str) -> Duration {
        use std::time::SystemTime;

        let key = format!("rate_limit:{}", host);

        // Get or create host state
        let mut state = self.registry.get::<HostRateLimitState>(&key)
            .unwrap_or(None)
            .unwrap_or_else(|| HostRateLimitState::new(
                self.config.requests_per_second_per_host,
                self.config.burst_size
            ));

        let now = SystemTime::now();
        let delay = state.calculate_delay(now);

        // Save updated state
        let _ = self.registry.set_with_ttl(&key, &state, self.config.inactive_host_ttl);

        delay
    }

    async fn record_response(&self, host: &str, status: u16, retry_after: Option<Duration>) {
        use std::time::SystemTime;

        // Only handle 429 responses with Retry-After when enabled
        if status == 429 && retry_after.is_some() && self.config.enable_retry_after {
            let key = format!("rate_limit:{}", host);

            // Get or create host state
            let mut state = self.registry.get::<HostRateLimitState>(&key)
                .unwrap_or(None)
                .unwrap_or_else(|| HostRateLimitState::new(
                    self.config.requests_per_second_per_host,
                    self.config.burst_size
                ));

            // Set retry-after deadline
            let retry_duration = retry_after.unwrap().min(self.config.max_backoff_delay);
            state.retry_after_until = Some(SystemTime::now() + retry_duration);

            // Save updated state
            let _ = self.registry.set_with_ttl(&key, &state, self.config.inactive_host_ttl);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::registry::InMemoryRegistry;

    fn create_test_service() -> RateLimiterService<InMemoryRegistry> {
        let registry = Arc::new(InMemoryRegistry::new());
        let config = RateLimitConfig::default();
        RateLimiterService::new(registry, config)
    }

    // TDD: First failing test
    #[tokio::test]
    async fn should_allow_request_when_no_previous_requests() {
        // Arrange
        let service = create_test_service();

        // Act
        let delay = service.acquire_permit("example.com".to_string()).await.unwrap();

        // Assert - first request should have no delay
        assert_eq!(delay, Duration::ZERO);
    }

    // TDD: Second test to drive out token bucket logic
    #[tokio::test]
    async fn should_delay_request_when_rate_limit_exceeded() {
        // Arrange
        let service = create_test_service();

        // Act - exhaust the burst capacity
        for _ in 0..5 {
            let delay = service.acquire_permit("example.com".to_string()).await.unwrap();
            assert_eq!(delay, Duration::ZERO); // Burst should allow immediate requests
        }

        // Act - next request should be delayed
        let delay = service.acquire_permit("example.com".to_string()).await.unwrap();

        // Assert - should be delayed (approximately 1 second for 1 RPS)
        assert!(delay > Duration::from_millis(900));
        assert!(delay < Duration::from_millis(1100));
    }

    // TDD: Third test for host isolation
    #[tokio::test]
    async fn should_isolate_rate_limits_per_host() {
        // Arrange
        let service = create_test_service();

        // Act - exhaust rate limit for first host
        for _ in 0..6 {
            let _ = service.acquire_permit("example.com".to_string()).await.unwrap();
        }

        // Act - request for different host should not be delayed
        let delay = service.acquire_permit("other.com".to_string()).await.unwrap();

        // Assert - different host should have no delay
        assert_eq!(delay, Duration::ZERO);
    }

    // TDD: Fourth test for Retry-After support
    #[tokio::test]
    async fn should_handle_retry_after_delays() {
        // Arrange
        let service = create_test_service();
        let host = "example.com".to_string();

        // Act - simulate 429 response with Retry-After
        service.record_response(
            host.clone(),
            429,
            Some(Duration::from_millis(500))
        ).await.unwrap();

        // Act - next request should be delayed by Retry-After duration
        let delay = service.acquire_permit(host).await.unwrap();

        // Assert - should respect Retry-After delay
        assert!(delay >= Duration::from_millis(400)); // Allow some tolerance
        assert!(delay <= Duration::from_millis(600));
    }

    // TDD: Test that service is generic over Registry trait
    #[tokio::test]
    async fn should_work_with_any_registry_implementation() {
        // Arrange - demonstrate generic nature by explicitly typing
        let registry: Arc<InMemoryRegistry> = Arc::new(InMemoryRegistry::new());
        let config = RateLimitConfig::default();

        // Act - service works with any Registry implementation
        let service: RateLimiterService<InMemoryRegistry> = RateLimiterService::new(registry, config);

        // Assert - basic functionality works
        let delay = service.acquire_permit("test.com".to_string()).await.unwrap();
        assert_eq!(delay, Duration::ZERO);
    }
}