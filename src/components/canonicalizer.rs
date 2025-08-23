use url::Url;

use crate::error::Error;

/// Converts URLs to a canonical form.
pub trait Canonicalizer: Send + Sync {
  fn canonicalize(&self, url: &Url) -> Result<Url, Error>;
}
