/// Handles crawling events.
pub trait Monitor {
  /// Called when a URL is successfully fetched and processed.
  fn on_task_ready(&self, task: &CrawlTask, status: u16, links_found: usize, admitted_links: usize);
}

/// Simple console reporter that prints events to stdout.
#[derive(Clone, Debug, Default)]
pub struct ConsoleMonitor;

impl Monitor for ConsoleMonitor {
  fn on_task_ready(
    &self,
    task: &CrawlTask,
    status: u16,
    links_found: usize,
    admitted_links: usize,
  ) {
    println!(
      "✓ Visited: {} (status: {}, {} links found, {} added to frontier)",
      task.url, status, links_found, admitted_links
    );
  }
}

/// Composite monitor that forwards events to multiple monitors.
pub struct MultiMonitor(pub Vec<Box<dyn Monitor + Send + Sync>>);

impl Monitor for MultiMonitor {
  fn on_task_ready(&self, task: &CrawlTask, status: u16, links: usize, admitted: usize) {
    for m in &self.0 {
      m.on_task_ready(task, status, links, admitted);
    }
  }
}

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Monitor that tracks progress and periodically reports it.
#[derive(Clone, Default)]
pub struct ProgressMonitor {
  discovered: Arc<AtomicUsize>,
  fetched: Arc<AtomicUsize>,
}

impl ProgressMonitor {
  pub fn spawn_reporter(
    &self,
    interval_secs: u64,
    shutdown: Arc<AtomicBool>,
  ) -> tokio::task::JoinHandle<()> {
    let discovered = self.discovered.clone();
    let fetched = self.fetched.clone();
    let interval = Duration::from_secs(interval_secs);

    tokio::spawn(async move {
      let mut last_report = Instant::now();

      loop {
        if shutdown.load(Ordering::Relaxed) {
          tracing::info!("Progress monitor shutting down by signal");
          break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await; // Faster polling for shutdown.

        if last_report.elapsed() >= interval {
          let d = discovered.load(Ordering::Relaxed);
          let f = fetched.load(Ordering::Relaxed);
          println!("📊 Progress: {}/{}", f, d);

          last_report = Instant::now();
        }
      }
    })
  }
}

impl Monitor for ProgressMonitor {
  fn on_task_ready(
    &self,
    _url: &CrawlTask,
    _status: u16,
    _links_found: usize,
    admitted_links: usize,
  ) {
    self.fetched.fetch_add(1, Ordering::Relaxed);
    self.discovered.fetch_add(admitted_links, Ordering::Relaxed);
  }
}

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::types::CrawlTask;

#[derive(serde::Serialize)]
pub struct OutputRow {
  pub url: String,
  pub status: u16,
  pub depth: u32,
  pub parent: Option<String>,
}

/// Monitor that saves crawl results to a file in JSON lines format.
pub struct SavingMonitor {
  writer: std::sync::Mutex<BufWriter<File>>,
}

impl SavingMonitor {
  pub fn new(path: PathBuf) -> Self {
    let file = File::create(path).expect("failed to create output file");
    let writer = BufWriter::new(file);
    Self {
      writer: std::sync::Mutex::new(writer),
    }
  }
}

impl Monitor for SavingMonitor {
  fn on_task_ready(&self, task: &CrawlTask, status: u16, _found: usize, _admitted: usize) {
    let row = OutputRow {
      url: task.url.to_string(),
      status,
      depth: task.depth,
      parent: task.parent.as_ref().map(|p| p.to_string()),
    };

    if let Ok(json) = serde_json::to_string(&row) {
      let mut writer = self.writer.lock().unwrap();
      let _ = writeln!(writer, "{json}");
      let _ = writer.flush(); // Ensure data is written promptly.
    }
  }
}
