use crate::types::CrawlTask;

/// Storage for crawling items, along with basic scheduling.
pub trait Frontier {
  /// Add a newly discovered request.
  fn add(&mut self, req: CrawlTask);

  /// Pop the next request to process.
  fn pop(&mut self) -> Option<CrawlTask>;

  /// Current number of pending requests.
  fn len(&self) -> usize;
}
