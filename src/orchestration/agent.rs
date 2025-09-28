use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::components::canonicalizer::Canonicalizer;
use crate::components::extractor::Extractor;
use crate::components::fetcher::Fetcher;
use crate::components::policy::AsyncPolicy;
use crate::components::registry::Registry;
use crate::error::Error;
use crate::orchestration::broker::CrawlBrokerService;
use crate::services::rate_limiter::RateLimiterService;
use crate::types::CrawlTask;

pub struct Agent<F: Fetcher, E: Extractor, C: Canonicalizer, AP: AsyncPolicy, R: Registry> {
  /// Stateless components.
  fetcher: F,
  extractor: E,
  canonicalizer: C,
  async_policy: AP,
  /// Single shared registry for all agents.
  registry: Arc<R>,
  /// Interface to the broker service (task orchestration).
  broker_svc: CrawlBrokerService,
  /// Rate limiter service for coordinated rate limiting across agents.
  rate_limiter: Arc<RateLimiterService<R>>,
}

impl<F: Fetcher, E: Extractor, C: Canonicalizer, AP: AsyncPolicy, R: Registry + 'static> Agent<F, E, C, AP, R> {
  pub fn new(
    fetcher: F,
    extractor: E,
    canonicalizer: C,
    async_policy: AP,
    registry: Arc<R>,
    broker_svc: CrawlBrokerService,
    rate_limiter: Arc<RateLimiterService<R>>,
  ) -> Self {
    tracing::info!("Initializing new Agent instance");
    Agent {
      fetcher,
      extractor,
      canonicalizer,
      async_policy,
      registry,
      broker_svc,
      rate_limiter,
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
    let task_url = task.url.clone();
    let task_depth = task.depth;

    // Apply rate limiting before fetching
    let host = task_url.host_str().unwrap_or("unknown").to_string();
    let delay = self.rate_limiter.acquire_permit(host.clone()).await
      .map_err(|e| Error::Unexpected(format!("Rate limiter error: {}", e)))?;

    if delay > std::time::Duration::ZERO {
      tracing::info!("Rate limiting: waiting {:?} for host {}", delay, host);
      tokio::time::sleep(delay).await;
    }

    let fetched_content = self.fetcher.fetch(&task_url).await?;
    tracing::info!(
      "Fetched content from {} with status: {}",
      fetched_content.url,
      fetched_content.status
    );

    // Record response for rate limiting (especially 429 responses)
    self.rate_limiter.record_response(host, fetched_content.status, None).await
      .map_err(|e| Error::Unexpected(format!("Rate limiter error: {}", e)))?;

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

    // Check against the async policy.
    let child_depth = task_depth + 1;
    let mut admitted_links = Vec::new();
    for child_link in &canonical_links {
      let verdict = self.async_policy.evaluate(child_link, &task_url, child_depth).await;
      if verdict.allowed {
        tracing::debug!("Link allowed: {}", child_link);
        admitted_links.push(child_link.clone());
      } else {
        tracing::warn!("Link {} denied by policy: {}", child_link, verdict.reason);
      }
    }

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
      .event_task_done(task, fetched_content.status, links.len(), new_links.len())
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
  pub async fn run(&self, shutdown: Arc<AtomicBool>) {
    tracing::info!("Starting agent crawling loop");
    loop {
      if shutdown.load(Ordering::Relaxed) {
        tracing::info!("Agent shutting down by signal");
        break;
      }

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

    tracing::info!("Agent has exited the crawling loop");
  }
}
