use std::marker::PhantomData;
use std::sync::Arc;

use tokio::task::JoinHandle;

use crate::broker::BrokerWorker;
use crate::broker::CrawlBrokerService;
use crate::components::frontier::Frontier;
use crate::components::registry::Registry;
use crate::error::Error;

/// Default capacity for the MPSC channel used by the broker.
const DEFAULT_BROKER_CAPACITY: usize = 8192;

pub struct Engine<Fr: Frontier, R: Registry> {
  // Shared registry for all agents.
  registry: Arc<R>,
  // Broker is used to distribute tasks (wraps frontier).
  broker_frontier_type: PhantomData<Fr>,
  broker_handle: JoinHandle<()>,
  broker_svc: CrawlBrokerService,
}

impl<Fr: Frontier + Send + 'static, R: Registry> Engine<Fr, R> {
  pub async fn new(frontier: Fr, registry: Arc<R>) -> Self {
    let (broker_svc, rx) = CrawlBrokerService::new_bounded(DEFAULT_BROKER_CAPACITY).await;
    let broker_handle = BrokerWorker::spawn(rx, frontier);
    Engine {
      registry,
      broker_handle,
      broker_svc,
      broker_frontier_type: PhantomData,
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

  pub fn get_registry(&self) -> Arc<R> {
    self.registry.clone()
  }

  pub fn get_broker_svc(&self) -> &CrawlBrokerService {
    &self.broker_svc
  }
}
