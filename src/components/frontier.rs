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

  /// Check if the frontier is empty.
  fn is_empty(&self) -> bool {
    self.len() == 0
  }
}

/// Frontier that will process requests in the order they were added (FIFO).
#[derive(Debug, Default)]
pub struct BfsFrontier {
  deque: VecDeque<CrawlTask>,
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::CrawlTask;
  use url::Url;

  #[test]
  fn bfs_is_fifo() {
    let mut f = BfsFrontier::default();

    let a = CrawlTask {
      url: Url::parse("https://example.com/a").unwrap(),
      depth: 0,
      parent: None,
    };
    let b = CrawlTask {
      url: Url::parse("https://example.com/b").unwrap(),
      depth: 0,
      parent: None,
    };
    let c = CrawlTask {
      url: Url::parse("https://example.com/c").unwrap(),
      depth: 0,
      parent: None,
    };

    f.add(a);
    f.add(b);
    f.add(c);

    assert_eq!(f.pop().unwrap().url.path(), "/a");
    assert_eq!(f.pop().unwrap().url.path(), "/b");
    assert_eq!(f.pop().unwrap().url.path(), "/c");
    assert!(f.pop().is_none());
  }
}
