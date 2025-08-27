use url::Url;

use crate::error::Error;

/// Converts URLs to a canonical form.
pub trait Canonicalizer {
  fn canonicalize(&self, url: &Url) -> Result<Url, Error>;
}

/// Canonicalizer that is focused on base URL, ignoring query parameters (+ fragment).
/// Example: `http://foo.bar/posts?page=123` gets turned into `http://foo.bar/posts`.
#[derive(Clone, Debug)]
pub struct QueryCanonicalizer {
  enabled: bool,
}

impl QueryCanonicalizer {
  pub fn new(enabled: bool) -> Self {
    Self { enabled }
  }
}

impl Default for QueryCanonicalizer {
  fn default() -> Self {
    Self { enabled: true }
  }
}

impl Canonicalizer for QueryCanonicalizer {
  #[tracing::instrument(skip(self), fields(input = %url))]
  fn canonicalize(&self, url: &Url) -> Result<Url, Error> {
    if !self.enabled {
      tracing::trace!("canonicalization disabled; returning original URL");
      return Ok(url.clone());
    }

    let mut new_url = url.clone();

    // Remove query.
    if new_url.query().is_some() {
      tracing::trace!(old_query = ?new_url.query(), "dropping query");
      new_url.set_query(None);
    }

    // Remove fragment too.
    if new_url.fragment().is_some() {
      tracing::trace!(old_fragment = ?new_url.fragment(), "dropping fragment");
      new_url.set_fragment(None);
    }

    // NOTE: We could also update host to lowercase, but that is extreme case.

    // Log changes.
    let before = url.as_str();
    let after = new_url.as_str();
    if after != before {
      tracing::debug!(%before, %after, "canonicalized URL changed");
    } else {
      tracing::trace!(%after, "canonicalized URL unchanged");
    }

    Ok(new_url)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use url::Url;

  fn canon(s: &str) -> String {
    let url = Url::parse(s).unwrap();
    let c = QueryCanonicalizer::default();
    c.canonicalize(&url).unwrap().to_string()
  }

  #[test]
  fn removes_query_and_fragment() {
    assert_eq!(
      canon("http://foo.bar/posts?page=123#top"),
      "http://foo.bar/posts"
    );
    assert_eq!(canon("https://foo.bar/#frag"), "https://foo.bar/");
    assert_eq!(canon("https://foo.bar/?q=1"), "https://foo.bar/");
  }

  #[test]
  fn sets_root_path_if_empty() {
    // `url::Url` already normalizes http(s) with empty path to trailing slash,
    // but this test ensures our canonical form matches that expectation.
    assert_eq!(canon("http://example.com"), "http://example.com/");
    assert_eq!(canon("https://example.com"), "https://example.com/");
  }

  #[test]
  fn noop_mode_returns_input() {
    let input = "http://example.com/?a=1#frag";
    let url = Url::parse(input).unwrap();
    let c = QueryCanonicalizer::new(false);
    let out = c.canonicalize(&url).unwrap();
    assert_eq!(out.as_str(), input);
  }
}
