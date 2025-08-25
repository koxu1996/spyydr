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
    let response = self.client.get(url.as_str()).send().await?;

    let status = response.status().as_u16();
    tracing::debug!(status, "received response");

    let body = response.text().await?;
    tracing::trace!(body_len = body.len(), "body read");

    Ok(FetchedContent {
      url: url.clone(),
      status,
      body,
      // elapsed: Duration::from_millis(0), // Placeholder for elapsed time
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use httpmock::prelude::*;
  use url::Url;

  #[tokio::test]
  async fn reqwest_fetcher_returns_200_body() {
    let server = MockServer::start();
    let m = server.mock(|when, then| {
      when.method(GET).path("/ok");
      then
        .status(200)
        .header("content-type", "text/plain")
        .body("hello world");
    });

    let fetcher = ReqwestFetcher {
      client: reqwest::Client::new(),
    };

    let url = Url::parse(&format!("{}/ok", server.base_url())).unwrap();
    let page = fetcher.fetch(&url).await.expect("fetch ok");

    m.assert();
    assert_eq!(page.url, url);
    assert_eq!(page.status, 200);
    assert_eq!(page.body, "hello world");
  }

  #[tokio::test]
  async fn reqwest_fetcher_surfaces_non_2xx_status() {
    let server = MockServer::start();
    let m = server.mock(|when, then| {
      when.method(GET).path("/notfound");
      then
        .status(404)
        .header("content-type", "text/plain")
        .body("missing");
    });

    let fetcher = ReqwestFetcher {
      client: reqwest::Client::new(),
    };

    let url = Url::parse(&format!("{}/notfound", server.base_url())).unwrap();
    let page = fetcher
      .fetch(&url)
      .await
      .expect("fetch ok even for non-2xx");

    m.assert();
    assert_eq!(page.url, url);
    assert_eq!(page.status, 404);
    assert_eq!(page.body, "missing");
  }

  #[tokio::test]
  async fn reqwest_fetcher_network_error_maps_to_error() {
    // Point to a port with no listener to force a connection error quickly.
    // Using 127.0.0.1:1 is a common trick (privileged/likely closed).
    let url = Url::parse("http://127.0.0.1:1/").unwrap();

    let fetcher = ReqwestFetcher {
      client: reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap(),
    };

    fetcher.fetch(&url).await.expect_err("should error");
  }
}
