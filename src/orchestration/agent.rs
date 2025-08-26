use std::sync::Arc;

use crate::components::canonicalizer::Canonicalizer;
use crate::components::extractor::Extractor;
use crate::components::fetcher::Fetcher;
use crate::components::policy::Policy;
use crate::components::registry::Registry;
use crate::error::Error;
use crate::orchestration::broker::CrawlBrokerService;
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
    tracing::info!("Initializing new Agent instance");
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
  #[tracing::instrument(skip(self), level = "debug")]
  pub async fn get_next_task(&self) -> Result<Option<CrawlTask>, Error> {
    tracing::debug!("Requesting next task from broker");
    let maybe_task = match self.broker_svc.pop_task().await? {
      Some(task) => {
        tracing::debug!("Received task: {:?}", task);
        Some(task)
      }
      None => {
        tracing::debug!("Broker returned no task");
        None
      }
    };

    Ok(maybe_task)
  }

  /// Runs core logic of task processing.
  #[tracing::instrument(skip(self, task), fields(url = %task.url, depth = task.depth))]
  pub async fn process_task(&self, task: CrawlTask) -> Result<(), Error> {
    tracing::info!("Processing task for URL: {}", task.url);
    let task_url = task.url;
    let task_depth = task.depth;

    let fetched_content = self.fetcher.fetch(&task_url).await?;
    tracing::info!(
      "Fetched content from {} with status: {}",
      fetched_content.url,
      fetched_content.status
    );

    // Mark item as visited in the registry.
    self.registry.mark_fetched(&task_url);
    tracing::debug!("Marked URL as fetched in registry");

    let links = self.extractor.find_links(&fetched_content)?;
    tracing::info!("Extracted {} links from {}", links.len(), task_url);

    // Canonicalize the links.
    let canonical_links = links
      .iter()
      .map(|link| self.canonicalizer.canonicalize(link))
      .collect::<Result<Vec<_>, _>>()?;
    tracing::debug!("Canonicalized {} links", canonical_links.len());

    // Check against the policy.
    let child_depth = task_depth + 1;
    let admitted_links = canonical_links
      .iter()
      .filter_map(|child_link| {
        let verdict = self.policy.evaluate(child_link, &task_url, child_depth);
        if verdict.allowed {
          tracing::debug!("Link allowed: {}", child_link);
          Some(child_link.clone())
        } else {
          tracing::warn!("Link {} denied by policy: {}", child_link, verdict.reason);
          None
        }
      })
      .collect::<Vec<_>>();

    // Put admitted links into the registry.
    let new_links = self.registry.register_many(admitted_links.iter());
    tracing::info!("Registered {} new links", new_links.len());

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
      self.broker_svc.push_task(task).await?;
    }

    // Notify broker that task is done.
    self
      .broker_svc
      .event_task_done(
        task_url.clone(),
        fetched_content.status,
        links.len(),
        new_links.len(),
      )
      .await;

    tracing::info!(
      "Task complete: {} | extracted: {} | admitted: {}",
      task_url,
      links.len(),
      new_links.len()
    );

    Ok(())
  }

  // Run crawling loop.
  pub async fn run_infinite_loop(&self) {
    tracing::info!("Starting agent crawling loop");
    loop {
      let maybe_task = self.get_next_task().await;
      match maybe_task {
        Ok(Some(task)) => {
          if let Err(e) = self.process_task(task).await {
            tracing::error!("Error processing task: {}", e);
          }
        }
        Ok(None) => {
          tracing::info!("No more tasks in the frontier, waiting for new tasks...");
          tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // Sleep before checking again.
        }
        Err(e) => {
          tracing::error!("Failed to get next task: {}", e);
          break; // Exit on error.
        }
      }
    }
  }
}
