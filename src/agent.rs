use std::sync::Arc;

use crate::broker::CrawlBrokerService;
use crate::components::canonicalizer::Canonicalizer;
use crate::components::extractor::Extractor;
use crate::components::fetcher::Fetcher;
use crate::components::policy::Policy;
use crate::components::registry::Registry;
use crate::error::Error;
use crate::types::CrawlTask;

pub struct Agent<F: Fetcher, E: Extractor, C: Canonicalizer, P: Policy, R: Registry> {
  /// Stateless components.
  fetcher: F,
  extractor: E,
  canonicalizer: C,
  policy: P,
  /// Single shared registry for all agents.
  registry: Arc<R>,
  /// Interface to the broker service (task orchestration).
  broker_svc: CrawlBrokerService,
}

impl<F: Fetcher, E: Extractor, C: Canonicalizer, P: Policy, R: Registry> Agent<F, E, C, P, R> {
  pub async fn new(
    fetcher: F,
    extractor: E,
    canonicalizer: C,
    policy: P,
    registry: Arc<R>,
    broker_svc: CrawlBrokerService,
  ) -> Self {
    Agent {
      fetcher,
      extractor,
      canonicalizer,
      policy,
      registry,
      broker_svc,
    }
  }

  // Get next item from the frontier (via broker).
  pub async fn get_next_task(&self) -> Result<Option<CrawlTask>, Error> {
    let maybe_task = self.broker_svc.pop_task().await.unwrap(); // TODO: Handle error properly.

    Ok(maybe_task)
  }

  /// Runs core logic of task processing.
  pub async fn run_once(&self, task: CrawlTask) -> Result<(), Error> {
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
      self.broker_svc.push_task(task).await.unwrap(); // TODO: Handle error properly.
    }

    Ok(())
  }
}
