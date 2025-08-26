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

/// Composite monitor that forwards events to multiple monitors.
pub struct MultiMonitor(pub Vec<Box<dyn Monitor + Send + Sync>>);

impl Monitor for MultiMonitor {
  fn on_task_ready(&self, url: url::Url, status: u16, links: usize, admitted: usize) {
    for m in &self.0 {
      m.on_task_ready(url.clone(), status, links, admitted);
    }
  }
}

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Monitor that tracks progress and periodically reports it.
#[derive(Clone, Default)]
pub struct ProgressMonitor {
  discovered: Arc<AtomicUsize>,
  fetched: Arc<AtomicUsize>,
}

impl ProgressMonitor {
  pub fn spawn_reporter(&self, interval_secs: u64) {
    let discovered = self.discovered.clone();
    let fetched = self.fetched.clone();

    tokio::spawn(async move {
      loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;

        let d = discovered.load(Ordering::Relaxed);
        let f = fetched.load(Ordering::Relaxed);
        println!("📊 Progress: discovered: {}, fetched: {}", d + f, f);
      }
    });
  }
}

impl Monitor for ProgressMonitor {
  fn on_task_ready(
    &self,
    _url: url::Url,
    _status: u16,
    _links_found: usize,
    admitted_links: usize,
  ) {
    self.fetched.fetch_add(1, Ordering::Relaxed);
    self.discovered.fetch_add(admitted_links, Ordering::Relaxed);
  }
}
