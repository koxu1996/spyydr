use crate::components::canonicalizer::Canonicalizer;
use crate::components::extractor::Extractor;
use crate::components::fetcher::Fetcher;
use crate::components::frontier::Frontier;
use crate::components::policy::Policy;
use crate::components::registry::Registry;
use crate::error::Error;

pub struct Engine<F: Fetcher, E: Extractor, Fr: Frontier, C: Canonicalizer, P: Policy, R: Registry>
{
  fetcher: F,
  extractor: E,
  frontier: Fr,
  canonicalizer: C,
  policy: P,
  registry: R,
}

impl<F: Fetcher, E: Extractor, Fr: Frontier, C: Canonicalizer, P: Policy, R: Registry>
  Engine<F, E, Fr, C, P, R>
{
  pub fn new(
    fetcher: F,
    extractor: E,
    frontier: Fr,
    canonicalizer: C,
    policy: P,
    registry: R,
  ) -> Self {
    Engine {
      fetcher,
      extractor,
      frontier,
      canonicalizer,
      policy,
      registry,
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

    // TODO: Mark it as visited in the registry!

    let links = self.extractor.find_links(&fetched_content)?;
    tracing::info!("Extracted {} links", links.len());

    // Canonicalize the links.
    let canonical_links = links
      .iter()
      .map(|link| self.canonicalizer.canonicalize(link))
      .collect::<Result<Vec<_>, _>>()?;

    // Check against the policy.
    let admitted_links = canonical_links
      .iter()
      .filter_map(|link| {
        let verdict = self.policy.evaluate(link, &task_url, task_depth);
        if verdict.allowed {
          Some(link.clone())
        } else {
          tracing::warn!("Link {} denied by policy: {}", link, verdict.reason);
          None
        }
      })
      .collect::<Vec<_>>();

    // Put admitted links into the registry.
    let new_links = self.registry.register_many(admitted_links.iter());

    // Put all new links into the frontier.
    let tasks = new_links.iter().map(|link| {
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
