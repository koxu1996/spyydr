use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tokio::task::JoinHandle;

use crate::components::frontier::Frontier;
use crate::components::monitor::Monitor;
use crate::components::registry::Registry;
use crate::error::Error;
use crate::orchestration::broker::BrokerWorker;
use crate::orchestration::broker::CrawlBrokerService;

/// Default capacity for the MPSC channel used by the broker.
const DEFAULT_BROKER_CAPACITY: usize = 8192;

pub struct Engine<Fr: Frontier, R: Registry, M: Monitor> {
  // Shared registry for all agents.
  registry: Arc<R>,
  // Broker is used to distribute tasks (wraps frontier).
  broker_frontier_type: PhantomData<Fr>,
  broker_handle: JoinHandle<()>,
  broker_svc: CrawlBrokerService,
  // Monitor for observing crawl events.
  monitor_type: PhantomData<M>,
}

impl<Fr: Frontier + Send + 'static, R: Registry, M: Monitor + Send + 'static> Engine<Fr, R, M> {
  pub async fn new(
    frontier: Fr,
    registry: Arc<R>,
    monitor: M,
    max_tasks_processed: Option<usize>,
    shutdown: Arc<AtomicBool>,
  ) -> Self {
    tracing::info!(
      "Initializing Engine with default broker capacity: {}",
      DEFAULT_BROKER_CAPACITY
    );
    let (broker_svc, rx) = CrawlBrokerService::new_bounded(DEFAULT_BROKER_CAPACITY).await;
    let broker_handle = BrokerWorker::spawn(rx, frontier, monitor, max_tasks_processed, shutdown);
    tracing::info!("Broker worker spawned");

    Engine {
      registry,
      broker_handle,
      broker_svc,
      broker_frontier_type: PhantomData,
      monitor_type: PhantomData,
    }
  }

  #[tracing::instrument(level = "info", skip(self))]
  pub async fn init(&mut self, start_url: &str) -> Result<(), Error> {
    // Initialize the frontier with the starting URL.
    tracing::info!("Initializing crawl with start URL: {}", start_url);
    let url = url::Url::parse(start_url)?;
    let task = crate::types::CrawlTask {
      url: url.clone(),
      depth: 0,     // Starting depth.
      parent: None, // No parent for the initial task.
    };

    // CAUTION: We do not check policy for the start URL - it's always allowed.

    // Mark link as discovered in the registry.
    self.registry.register(&task.url);
    tracing::debug!(%task.url, "Registered start URL in registry");

    // Push the start task to the frontier (via broker).
    self.broker_svc.push_task(task).await?;
    tracing::info!(%url, "Pushed start task to broker");

    Ok(())
  }

  pub fn get_registry(&self) -> Arc<R> {
    self.registry.clone()
  }

  pub fn get_broker_svc(&self) -> &CrawlBrokerService {
    &self.broker_svc
  }

  // Wait for the broker worker to finish.
  // It takes self in order to return ownership of the broker handle.
  // NOTE: It requires that other references to the broker service are dropped.
  pub async fn wait_for_shutdown(self) -> Result<(), Error> {
    tracing::info!("Waiting for broker worker to shut down");
    // Drop the broker service to allow the broker worker to finish.
    drop(self.broker_svc);
    self
      .broker_handle
      .await
      .map_err(|e| Error::Unexpected(format!("Unable to join broker worker: {}", e)))?;
    tracing::info!("Broker worker has shut down");

    Ok(())
  }
}
