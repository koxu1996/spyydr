use std::collections::VecDeque;

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

#[derive(Debug)]
pub struct BfsFrontier {
  deque: VecDeque<CrawlTask>,
}

impl BfsFrontier {
  pub fn new() -> Self {
    Self {
      deque: VecDeque::new(),
    }
  }
}

impl Frontier for BfsFrontier {
  fn add(&mut self, req: CrawlTask) {
    self.deque.push_back(req);
  }

  fn pop(&mut self) -> Option<CrawlTask> {
    self.deque.pop_front()
  }

  fn len(&self) -> usize {
    self.deque.len()
  }
}
