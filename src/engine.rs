use std::marker::PhantomData;
use std::sync::Arc;

use tokio::task::JoinHandle;

use crate::broker::BrokerWorker;
use crate::broker::CrawlBrokerService;
use crate::components::canonicalizer::Canonicalizer;
use crate::components::extractor::Extractor;
use crate::components::fetcher::Fetcher;
use crate::components::frontier::Frontier;
use crate::components::policy::Policy;
use crate::components::registry::Registry;
use crate::error::Error;
use crate::types::CrawlTask;

/// Default capacity for the MPSC channel used by the broker.
const DEFAULT_BROKER_CAPACITY: usize = 8192;

pub struct Engine<F: Fetcher, E: Extractor, Fr: Frontier, C: Canonicalizer, P: Policy, R: Registry>
{
  fetcher: F,
  extractor: E,
  canonicalizer: C,
  policy: P,
  // Shared registry for all agents.
  registry: Arc<R>,
  // Broker is used to distribute tasks (wraps frontier).
  broker_frontier_type: PhantomData<Fr>,
  broker_handle: JoinHandle<()>,
  broker_svc: CrawlBrokerService,
}

impl<
  F: Fetcher,
  E: Extractor,
  Fr: Frontier + Send + 'static,
  C: Canonicalizer,
  P: Policy,
  R: Registry,
> Engine<F, E, Fr, C, P, R>
{
  pub async fn new(
    fetcher: F,
    extractor: E,
    frontier: Fr,
    canonicalizer: C,
    policy: P,
    registry: Arc<R>,
  ) -> Self {
    let (broker_svc, rx) = CrawlBrokerService::new_bounded(DEFAULT_BROKER_CAPACITY).await;
    let broker_handle = BrokerWorker::spawn(rx, frontier);
    Engine {
      fetcher,
      extractor,
      canonicalizer,
      policy,
      registry,
      broker_handle,
      broker_svc,
      broker_frontier_type: PhantomData::default(),
    }
  }

  pub async fn init(&mut self, start_url: &str) -> Result<(), Error> {
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

    // Push the start task to the frontier (via broker).
    self.broker_svc.push_task(task).await.unwrap(); // TODO: Handle error properly.

    Ok(())
  }

  // Get next item from the frontier (via broker).
  /// CAUTION: This code is already moved to agent.
  pub async fn get_next_task(&self) -> Result<Option<CrawlTask>, Error> {
    let maybe_task = self.broker_svc.pop_task().await.unwrap(); // TODO: Handle error properly.

    Ok(maybe_task)
  }

  /// Runs core logic of task processing.
  /// CAUTION: This code is already moved to agent.
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
