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
