use url::Url;

use crate::error::Error;
use crate::types::FetchedContent;

/// Fetches static content of web pages.
pub trait Fetcher {
  /// Fetch a URL and return a Page even for non-2xx (so Reporter can print).
  #[allow(async_fn_in_trait)]
  async fn fetch(&self, url: &Url) -> Result<FetchedContent, Error>;
}

/// A no-op fetcher that simulates a successful fetch without making any network requests.
pub struct NoopFetcher;

impl Fetcher for NoopFetcher {
  async fn fetch(&self, url: &Url) -> Result<FetchedContent, Error> {
    // Simulate a successful fetch with a dummy response.
    Ok(FetchedContent {
      url: url.clone(),
      status: 200,
      body: "Noop fetcher response".to_string(),
      // elapsed: Duration::from_millis(0), // Placeholder for elapsed time
    })
  }
}

/// Reqwest-based fetcher.
#[derive(Clone, Default)]
pub struct ReqwestFetcher {
  client: reqwest::Client,
}

impl Fetcher for ReqwestFetcher {
  async fn fetch(&self, url: &Url) -> Result<FetchedContent, Error> {
    let response = self
      .client
      .get(url.as_str())
      .send()
      .await
      .map_err(|e| Error::Unexpected(format!("Failed to fetch {}: {}", url, e)))?;

    let status = response.status().as_u16();
    let body = response
      .text()
      .await
      .map_err(|e| Error::Unexpected(format!("Failed to read body from {}: {}", url, e)))?;

    Ok(FetchedContent {
      url: url.clone(),
      status,
      body,
      // elapsed: Duration::from_millis(0), // Placeholder for elapsed time
    })
  }
}
