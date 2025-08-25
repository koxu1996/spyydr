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
  #[tracing::instrument(skip(self, page), fields(base=%page.url, body_len=page.body.len()))]
  fn find_links(&self, page: &FetchedContent) -> Result<Vec<Url>, Error> {
    // Parse html and extract all `<a href>`s.
    let doc = scraper::Html::parse_document(&page.body);
    let a_sel: scraper::Selector = scraper::Selector::parse("a[href]")
      .map_err(|e| Error::Unexpected(format!("failed to create selector: {}", e)))?;

    let mut links = Vec::new();
    for a in doc.select(&a_sel) {
      // Get trimmed href value.
      let href = match a.value().attr("href") {
        Some(h) if !h.trim().is_empty() => h.trim(),
        _ => {
          tracing::trace!("skip: missing/empty href");
          continue;
        }
      };

      // Quick-skip obvious non-links/fragments.
      if href.starts_with('#') {
        tracing::trace!(%href, "skip: fragment-only");
        continue;
      }
      if href.starts_with("javascript:") {
        tracing::trace!(%href, "skip: javascript");
        continue;
      }
      if href.starts_with("mailto:") {
        tracing::trace!(%href, "skip: mailto");
        continue;
      }
      if href.starts_with("tel:") {
        tracing::trace!(%href, "skip: tel");
        continue;
      }
      if href.starts_with("data:") {
        tracing::trace!(%href, "skip: data");
        continue;
      }

      // Resolve relative URLs.
      // TODO: Consider supporting `<base href="..">`.
      tracing::trace!(%href, "resolving");
      let absolute_url = Url::options().base_url(Some(&page.url)).parse(href)?;
      tracing::debug!(%absolute_url, "link found");
      links.push(absolute_url);
    }

    tracing::debug!(count = links.len(), "extraction finished");
    Ok(links)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use url::Url;

  const BASE: &str = "https://example.com/root/";

  // Minimal mock for fetched content.
  fn fc(base: &str, body: &str) -> FetchedContent {
    FetchedContent {
      url: Url::parse(base).expect("valid base"),
      body: body.to_string(),
      status: 200,
    }
  }

  // Run extractor and return URLs as strings for easy assertions.
  fn extract_to_strings(body: &str) -> Vec<String> {
    let page = fc(BASE, body);
    HtmlExtractor::default()
      .find_links(&page)
      .expect("extraction ok")
      .into_iter()
      .map(|u| u.to_string())
      .collect()
  }

  #[test]
  fn extracts_absolute_links() {
    let got = extract_to_strings(
      r#"
      <a href="https://example.com/a">A</a>
      <a href="https://example.com/B?x=1#frag">B</a>
    "#,
    );
    let expected = vec!["https://example.com/a", "https://example.com/B?x=1#frag"];
    assert_eq!(got, expected);
  }

  #[test]
  fn resolves_relative_links_against_base() {
    let got = extract_to_strings(
      r#"
      <a href="/abs">abs</a>
      <a href="rel">rel</a>
      <a href="./dot">dot</a>
      <a href="../up">up</a>
      <a href="rel#sec">fragrel</a>
      <a href="?q=1">qonly</a>
    "#,
    );
    let expected = vec![
      "https://example.com/abs",
      "https://example.com/root/rel",
      "https://example.com/root/dot",
      "https://example.com/up",
      "https://example.com/root/rel#sec",
      "https://example.com/root/?q=1",
    ];
    assert_eq!(got, expected);
  }

  #[test]
  fn skips_non_link_schemes_and_fragments() {
    let got = extract_to_strings(
      r##"
      <a href="#top">frag</a>
      <a href="javascript:void(0)">js</a>
      <a href="mailto:user@example.com">mail</a>
      <a href="tel:+123456789">tel</a>
      <a href="data:text/plain,hello">data</a>
      <a href="https://example.com/ok">ok</a>
    "##,
    );
    let expected = vec!["https://example.com/ok"];
    assert_eq!(got, expected);
  }

  #[test]
  fn trims_and_skips_empty_href() {
    let got = extract_to_strings(
      r#"
      <a href="   ">empty</a>
      <a href="">empty2</a>
      <a href="  /spaced  ">ok</a>
    "#,
    );
    let expected = vec!["https://example.com/spaced"];
    assert_eq!(got, expected);
  }

  #[test]
  fn allows_links_with_inline_fragments_not_starting_with_hash() {
    // Use r## so "#only" is literal inside the raw string.
    let got = extract_to_strings(
      r##"
      <a href="page#sec">ok</a>
      <a href="#only">skip</a>
    "##,
    );
    let expected = vec!["https://example.com/root/page#sec"];
    assert_eq!(got, expected);
  }
}
