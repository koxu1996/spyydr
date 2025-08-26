use std::marker::PhantomData;
use std::sync::Arc;

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
  pub async fn new(frontier: Fr, registry: Arc<R>, monitor: M) -> Self {
    let (broker_svc, rx) = CrawlBrokerService::new_bounded(DEFAULT_BROKER_CAPACITY).await;
    let broker_handle = BrokerWorker::spawn(rx, frontier, monitor);
    Engine {
      registry,
      broker_handle,
      broker_svc,
      broker_frontier_type: PhantomData,
      monitor_type: PhantomData,
    }
  }

  pub async fn init(&mut self, start_url: &str) -> Result<(), Error> {
    // Initialize the frontier with the starting URL.
    let url = url::Url::parse(start_url)?;
    let task = crate::types::CrawlTask {
      url,
      depth: 0,     // Starting depth.
      parent: None, // No parent for the initial task.
    };

    // Mark link as discovered in the registry.
    self.registry.register(&task.url);

    // Push the start task to the frontier (via broker).
    self.broker_svc.push_task(task).await?;

    Ok(())
  }

  pub fn get_registry(&self) -> Arc<R> {
    self.registry.clone()
  }

  pub fn get_broker_svc(&self) -> &CrawlBrokerService {
    &self.broker_svc
  }
}
