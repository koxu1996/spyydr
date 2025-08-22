use url::Url;

use crate::{error::Error, types::FetchedContent};

/// Extracts absolute links from the web content.
pub trait Extractor {
  /// Extract absolute, resolved URLs from the fetched page.
  /// NOTE: It does NOT deduplicate, does NOT canonicalize, and does NOT apply policy (it is handled by other components later).
  fn find_links(&self, page: &FetchedContent) -> Result<Vec<Url>, Error>;
}

