use std::time::{Duration, SystemTime};
use url::Url;

/// Content fetched from a URL.
#[derive(Clone, Debug)]
pub struct FetchedContent {
  pub url: Url,
  pub status: u16,
  pub body: String,
}

/// Verdict from the crawling policy.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyVerdict {
  pub allowed: bool,
  pub reason: &'static str,
}

impl PolicyVerdict {
  pub const ALLOW: Self = Self {
    allowed: true,
    reason: "",
  };

  pub fn deny(reason: &'static str) -> Self {
    Self {
      allowed: false,
      reason,
    }
  }
}

/// Status of a registry item.
#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RegistryItemStatus {
  Discovered,
  Fetched,
}

/// A task to crawl a specific URL.
#[derive(Clone, Debug)]
pub struct CrawlTask {
  pub url: Url,
  pub depth: u32,
  pub parent: Option<Url>,
}

/// Errors that can occur during registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
  #[error("Serialization failed: {0}")]
  Serialization(#[from] serde_json::Error),
  #[error("State not found: {key}")]
  NotFound { key: String },
  #[error("Invalid operation: {message}")]
  InvalidOperation { message: String },
}

/// Configuration for rate limiting service.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
  pub requests_per_second_per_host: f64,
  pub burst_size: u32,
  pub enable_retry_after: bool,
  pub max_backoff_delay: Duration,
  pub inactive_host_ttl: Duration,
}

impl Default for RateLimitConfig {
  fn default() -> Self {
    Self {
      requests_per_second_per_host: 1.0,
      burst_size: 5,
      enable_retry_after: true,
      max_backoff_delay: Duration::from_secs(300),
      inactive_host_ttl: Duration::from_secs(3600),
    }
  }
}

/// Per-host rate limiting state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HostRateLimitState {
  pub last_request: SystemTime,
  pub tokens: f64,
  pub tokens_per_second: f64,
  pub burst_size: u32,
  pub retry_after_until: Option<SystemTime>,
}

impl HostRateLimitState {
  pub fn new(requests_per_second: f64, burst_size: u32) -> Self {
    Self {
      last_request: SystemTime::now(),
      tokens: burst_size as f64,
      tokens_per_second: requests_per_second,
      burst_size,
      retry_after_until: None,
    }
  }

  /// Calculate delay needed before next request and update token bucket state.
  pub fn calculate_delay(&mut self, now: SystemTime) -> Duration {
    // Check retry-after first
    if let Some(retry_until) = self.retry_after_until {
      if now < retry_until {
        return retry_until.duration_since(now).unwrap_or_default();
      } else {
        self.retry_after_until = None;
      }
    }

    // Replenish tokens based on time elapsed
    self.replenish_tokens(now);

    // Try to consume a token
    if self.tokens >= 1.0 {
      self.tokens -= 1.0;
      Duration::ZERO
    } else {
      // Calculate delay needed to get one token
      let tokens_needed = 1.0 - self.tokens;
      Duration::from_secs_f64(tokens_needed / self.tokens_per_second)
    }
  }

  fn replenish_tokens(&mut self, now: SystemTime) {
    let elapsed = now.duration_since(self.last_request).unwrap_or_default();
    let new_tokens = elapsed.as_secs_f64() * self.tokens_per_second;
    self.tokens = (self.tokens + new_tokens).min(self.burst_size as f64);
    self.last_request = now;
  }
}

/// Errors that can occur during rate limiting operations.
#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
  #[error("Registry error: {0}")]
  Registry(#[from] RegistryError),
  #[error("Service communication failed")]
  Communication,
  #[error("Service shutdown")]
  Shutdown,
}

/// Configuration for robots.txt service.
#[derive(Debug, Clone)]
pub struct RobotsTxtConfig {
  pub enabled: bool,
  pub user_agent: String,
  pub cache_ttl: Duration,
  pub fetch_timeout: Duration,
}

impl Default for RobotsTxtConfig {
  fn default() -> Self {
    Self {
      enabled: true,
      user_agent: "SpyydrBot/1.0".to_string(),
      cache_ttl: Duration::from_secs(24 * 60 * 60), // 24 hours
      fetch_timeout: Duration::from_secs(10),
    }
  }
}

/// Cached robots.txt data for a host.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedRobotsTxt {
  pub host: String,
  pub content: Option<String>, // None if robots.txt was not found or invalid
  pub fetched_at: SystemTime,
}

impl CachedRobotsTxt {
  pub fn new(host: String, content: Option<String>) -> Self {
    Self {
      host,
      content,
      fetched_at: SystemTime::now(),
    }
  }

  pub fn is_expired(&self, ttl: Duration) -> bool {
    SystemTime::now()
      .duration_since(self.fetched_at)
      .unwrap_or_default()
      > ttl
  }
}

/// Errors that can occur during robots.txt operations.
#[derive(Debug, thiserror::Error)]
pub enum RobotsTxtError {
  #[error("Registry error: {0}")]
  Registry(#[from] RegistryError),
  #[error("HTTP fetch error: {0}")]
  Fetch(String),
  #[error("Service communication failed")]
  Communication,
  #[error("Service shutdown")]
  Shutdown,
}
