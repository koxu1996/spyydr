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
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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
