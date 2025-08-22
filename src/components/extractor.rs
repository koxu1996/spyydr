use url::Url;

use crate::{error::Error, types::FetchedContent};

/// Extracts absolute links from the web content.
pub trait Extractor {
  /// Extract absolute, resolved URLs from the fetched page.
  /// NOTE: It does NOT deduplicate, does NOT canonicalize, and does NOT apply policy (it is handled by other components later).
  fn find_links(&self, page: &FetchedContent) -> Result<Vec<Url>, Error>;
}

/// Extractor for HTML content.
#[derive(Default, Clone, Debug)]
pub struct HtmlExtractor;

impl Extractor for HtmlExtractor {
  fn find_links(&self, page: &FetchedContent) -> Result<Vec<Url>, Error> {
    // Parse html and extract `<a href>`s.
    let doc = scraper::Html::parse_document(&page.body);
    let a_sel = scraper::Selector::parse("a[href]").unwrap(); // TODO: Fix unwrap.

    let mut links = Vec::new();
    for a in doc.select(&a_sel) {
      // Get trimmed href value.
      let href = match a.value().attr("href") {
        Some(h) if !h.trim().is_empty() => h.trim(),
        _ => continue,
      };

      // Quick-skip obvious non-links/fragments.
      if href.starts_with('#')
        || href.starts_with("javascript:")
        || href.starts_with("mailto:")
        || href.starts_with("tel:")
        || href.starts_with("data:")
      {
        continue;
      }

      // Resolve relative URLs.
      // TODO: Consider supporting <base href> if it exists.
      let absolute_url = Url::options()
        .base_url(Some(&page.url))
        .parse(href)
        .unwrap();
      links.push(absolute_url);
    }

    Ok(links)
  }
}
