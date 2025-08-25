use crate::error::Error;
use crate::types::{FetchedContent, PolicyVerdict};
use url::Url;

/// Reports crawling events and statistics.
pub trait Reporter {
  /// Called when a URL is successfully fetched and processed.
  fn on_visit(&self, content: &FetchedContent, links_found: usize);

  /// Called when a URL is skipped due to policy restrictions.
  fn on_skip(&self, url: &Url, verdict: &PolicyVerdict);

  /// Called when an error occurs during processing.
  fn on_error(&self, url: &Url, error: &Error);

  // TODO: Add summary event?
}

/// Simple console reporter that prints events to stdout.
#[derive(Clone, Debug, Default)]
pub struct ConsoleReporter;

impl Reporter for ConsoleReporter {
  fn on_visit(&self, content: &FetchedContent, links_found: usize) {
    println!(
      "✓ Visited: {} (status: {}, {} links found)",
      content.url, content.status, links_found
    );
  }

  fn on_skip(&self, url: &Url, verdict: &PolicyVerdict) {
    println!("⚠ Skipped: {} (reason: {})", url, verdict.reason);
  }

  fn on_error(&self, url: &Url, error: &Error) {
    println!("✗ Error: {} - {}", url, error);
  }
}
