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

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Wrapper monitor that tracks progress and periodically reports it.
#[derive(Clone)]
pub struct WithProgressMonitor<M: Monitor> {
  inner: M,
  discovered: Arc<AtomicUsize>,
  fetched: Arc<AtomicUsize>,
}

impl<M: Monitor> WithProgressMonitor<M> {
  pub fn new(inner: M) -> Self {
    Self {
      inner,
      discovered: Arc::new(AtomicUsize::new(0)),
      fetched: Arc::new(AtomicUsize::new(0)),
    }
  }

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

impl<M: Monitor> Monitor for WithProgressMonitor<M> {
  fn on_task_ready(&self, url: url::Url, status: u16, links_found: usize, admitted_links: usize) {
    self
      .inner
      .on_task_ready(url, status, links_found, admitted_links);
    self.fetched.fetch_add(1, Ordering::Relaxed);
    self.discovered.fetch_add(admitted_links, Ordering::Relaxed);
  }
}
