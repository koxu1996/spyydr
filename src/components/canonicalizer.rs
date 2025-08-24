use url::Url;

use crate::error::Error;

/// Converts URLs to a canonical form.
pub trait Canonicalizer {
  fn canonicalize(&self, url: &Url) -> Result<Url, Error>;
}

/// Canonicalizer that is focused on base URL, ignoring query parameters.
#[derive(Clone, Debug, Default)]
pub struct NoQueryCanonicalizer;

impl NoQueryCanonicalizer {
  pub fn new() -> Self {
    Self
  }
}

impl Canonicalizer for NoQueryCanonicalizer {
  fn canonicalize(&self, url: &Url) -> Result<Url, Error> {
    // Use lowercase host.
    let mut u = url.clone();
    if let Some(host) = u.host_str() {
      u.set_host(Some(&host.to_ascii_lowercase())).unwrap(); // TODO: Fix unwrap.
    }

    // Remove default ports.
    match (u.scheme(), u.port()) {
      ("http", Some(80)) => {
        let _ = u.set_port(None);
      }
      ("https", Some(443)) => {
        let _ = u.set_port(None);
      }
      _ => {}
    }

    // Set root path if empty.
    if u.path().is_empty() {
      u.set_path("/");
    }

    // Remove query and fragment.
    u.set_query(None);
    u.set_fragment(None);

    Ok(u)
  }
}
