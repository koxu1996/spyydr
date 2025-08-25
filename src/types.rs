use url::Url;

#[derive(Clone, Debug)]
pub struct FetchedContent {
  pub url: Url,
  pub status: u16,
  pub body: String,
  // pub elapsed: Duration,
}

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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RegistryItemStatus {
  Discovered,
  Fetched,
}

#[derive(Clone, Debug)]
pub struct CrawlTask {
  pub url: Url,
  pub depth: u32,
  pub parent: Option<Url>,
}
