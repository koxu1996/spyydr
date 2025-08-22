use url::Url;

use crate::error::Error;
use crate::types::FetchedPage;

/// Fetches static content of web pages.
pub trait Fetcher {
  /// Fetch a URL and return a Page even for non-2xx (so Reporter can print).
  #[allow(async_fn_in_trait)]
  async fn fetch(&self, url: &Url) -> Result<FetchedPage, Error>;
}

/// A no-op fetcher that simulates a successful fetch without making any network requests.
pub struct NoopFetcher;

impl Fetcher for NoopFetcher {
  async fn fetch(&self, url: &Url) -> Result<FetchedPage, Error> {
    // Simulate a successful fetch with a dummy response.
    Ok(FetchedPage {
      url: url.clone(),
      status: 200,
      body: "Noop fetcher response".to_string(),
      // elapsed: Duration::from_millis(0), // Placeholder for elapsed time
    })
  }
}
