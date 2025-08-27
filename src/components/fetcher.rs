use backon::{ExponentialBuilder, Retryable};
use reqwest::StatusCode;
use url::Url;

use crate::error::Error;
use crate::types::FetchedContent;

/// Fetches static content of web pages.
pub trait Fetcher {
  /// Fetch an URL and return a response, even for non-2xx.
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

#[derive(Clone, Debug)]
pub struct ReqwestFetcherConfig {
  pub user_agent: Option<String>,
  pub max_retries: usize,
  pub timeout: Option<std::time::Duration>,
}

impl Default for ReqwestFetcherConfig {
  fn default() -> Self {
    Self {
      user_agent: None,
      max_retries: 5,
      timeout: None,
    }
  }
}

/// Reqwest-based fetcher.
#[derive(Clone, Default)]
pub struct ReqwestFetcher {
  client: reqwest::Client,
  config: ReqwestFetcherConfig,
}

impl ReqwestFetcher {
  /// Create a new ReqwestFetcher with configuration.
  pub fn new(config: ReqwestFetcherConfig) -> Result<Self, Error> {
    let mut builder = reqwest::Client::builder();

    // Set User-Agent if specified.
    if let Some(user_agent) = config.user_agent.clone() {
      builder = builder.user_agent(user_agent);
    }

    // Set request timeout if specified.
    if let Some(timeout) = config.timeout {
      builder = builder.timeout(timeout);
    }

    let client = builder
      .build()
      .map_err(|e| Error::Unexpected(format!("Failed to build HTTP client: {}", e)))?;

    Ok(Self { client, config })
  }
}

impl Fetcher for ReqwestFetcher {
  #[tracing::instrument(skip(self), fields(url=%url))]
  async fn fetch(&self, url: &Url) -> Result<FetchedContent, Error> {
    let url_str = url.as_str().to_string();
    let client = self.client.clone();

    // Define the fetch operation.
    let fetch_op = || async {
      tracing::debug!("Sending HTTP GET");
      let resp = client
        .get(&url_str)
        .send()
        .await
        .map_err(Error::HttpFetch)?;
      let status = resp.status();
      if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        tracing::warn!(status = %status, "Transient HTTP error, will retry");
        return Err(Error::TransientHttp(status.as_u16()));
      }
      Ok(resp)
    };

    // Try fetching URL with retries.
    let max_retries = self.config.max_retries;
    let response = fetch_op
      .retry(
        ExponentialBuilder::default()
          .with_max_times(max_retries)
          .with_jitter(),
      )
      .notify(|err, dur| {
        tracing::warn!(error = %err, wait = ?dur, "Retrying HTTP fetch");
      })
      .await?;

    let status = response.status().as_u16();
    tracing::debug!(status, "received response");

    // Read body, but only if Content-Type is text/html.
    // Other types (images, binaries) will just get empty body.
    let is_html = response
      .headers()
      .get(reqwest::header::CONTENT_TYPE)
      .and_then(|v| v.to_str().ok())
      .map(|ct| ct.starts_with("text/html"))
      .unwrap_or(false);
    let body = match is_html {
      true => {
        tracing::debug!("HTML content, reading body");
        response.text().await?
      }
      false => {
        tracing::debug!("non-HTML content, replacing with empty body");
        String::new()
      }
    };
    tracing::debug!(body_len = body.len(), "body read");
    tracing::trace!(body = %body, "body content");

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
      config: ReqwestFetcherConfig::default(),
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
      config: ReqwestFetcherConfig::default(),
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
      config: ReqwestFetcherConfig {
        user_agent: None,
        max_retries: 0,
        timeout: None,
      },
    };

    fetcher.fetch(&url).await.expect_err("should error");
  }
}
