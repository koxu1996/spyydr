/// Handles crawling events.
pub trait Monitor {
  /// Called when a URL is successfully fetched and processed.
  fn on_task_ready(&self, url: url::Url, status: u16, links_found: usize, admitted_links: usize);
}

/// Simple console reporter that prints events to stdout.
#[derive(Clone, Debug, Default)]
pub struct ConsoleMonitor;

impl Monitor for ConsoleMonitor {
  fn on_task_ready(&self, url: url::Url, status: u16, links_found: usize, admitted_links: usize) {
    println!(
      "✓ Visited: {} (status: {}, {} links found, {} added to frontier)",
      url, status, links_found, admitted_links
    );
  }
}
