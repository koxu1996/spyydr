use crate::components::canonicalizer::Canonicalizer;
use crate::components::extractor::Extractor;
use crate::components::fetcher::Fetcher;
use crate::components::frontier::Frontier;
use crate::error::Error;

pub struct Engine<F: Fetcher, E: Extractor, Fr: Frontier, C: Canonicalizer> {
  fetcher: F,
  extractor: E,
  frontier: Fr,
  canonicalizer: C,
}

impl<F: Fetcher, E: Extractor, Fr: Frontier, C: Canonicalizer> Engine<F, E, Fr, C> {
  pub fn new(fetcher: F, extractor: E, frontier: Fr, canonicalizer: C) -> Self {
    Engine {
      fetcher,
      extractor,
      frontier,
      canonicalizer,
    }
  }

  pub fn init(&mut self, start_url: &str) -> Result<(), Error> {
    // Initialize the frontier with the starting URL.
    let url = match url::Url::parse(start_url) {
      Ok(url) => url,
      Err(e) => return Err(Error::Unexpected(e.to_string())), // TODO: Use a more specific error type.
    };
    let task = crate::types::CrawlTask {
      url,
      depth: 0,     // Starting depth.
      parent: None, // No parent for the initial task.
    };
    self.frontier.add(task);

    Ok(())
  }

  pub async fn run_once(&mut self) -> Result<(), Error> {
    // Get next item from the frontier.
    let task = match self.frontier.pop() {
      Some(task) => task,
      None => return Err(Error::Unexpected("No tasks in the frontier".to_string())),
    };
    let task_url = task.url;
    let task_depth = task.depth;

    let fetched_content = self.fetcher.fetch(&task_url).await?;
    tracing::info!("Fetched content of {:?}", fetched_content.url);

    let links = self.extractor.find_links(&fetched_content)?;
    tracing::info!("Extracted {} links", links.len());

    // For now just put all links into the frontier.
    // TODO: Implement policies etc.
    let tasks = links.iter().map(|link| {
      let url = link.clone();
      crate::types::CrawlTask {
        url,
        depth: task_depth + 1,          // Increment depth for child tasks.
        parent: Some(task_url.clone()), // Set parent URL.
      }
    });
    for task in tasks {
      self.frontier.add(task);
    }

    Ok(())
  }
}
