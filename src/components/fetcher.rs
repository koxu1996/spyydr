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
  #[tracing::instrument(skip(self), fields(url=%url))]
  async fn fetch(&self, url: &Url) -> Result<FetchedContent, Error> {
    // Simulate a successful fetch with a dummy response.
    tracing::debug!("Noop fetcher returning synthetic response");
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
  #[tracing::instrument(skip(self), fields(url=%url))]
  async fn fetch(&self, url: &Url) -> Result<FetchedContent, Error> {
    tracing::debug!("sending HTTP GET");
    let response = self
      .client
      .get(url.as_str())
      .send()
      .await
      .map_err(|e| Error::Unexpected(format!("Failed to fetch {}: {}", url, e)))?;

    let status = response.status().as_u16();
    tracing::debug!(status, "received response");

    let body = response
      .text()
      .await
      .map_err(|e| Error::Unexpected(format!("Failed to read body from {}: {}", url, e)))?;
    tracing::trace!(body_len = body.len(), "body read");

    Ok(FetchedContent {
      url: url.clone(),
      status,
      body,
      // elapsed: Duration::from_millis(0), // Placeholder for elapsed time
    })
  }
}
