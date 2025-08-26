use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::components::frontier::Frontier;
use crate::components::monitor::Monitor;
use crate::error::Error;
use crate::types::CrawlTask;

/// Service that provides a thread-safe interface to the crawling queue (frontier).
/// IMPORTANT: This requires running a worker thread to handle requests via an MPSC channel.
#[derive(Clone)]
pub struct CrawlBrokerService {
  tx: mpsc::Sender<BrokerCommand>,
}

impl CrawlBrokerService {
  /// Creates a new broker service with a bounded MPSC channel.
  /// IMPORTANT: Use `rx` to receive commands in a worker.
  pub async fn new_bounded(capacity: usize) -> (Self, mpsc::Receiver<BrokerCommand>) {
    let (tx, rx) = mpsc::channel(capacity);

    let service = Self { tx };
    (service, rx)
  }

  /// Pushes a new task to the crawling queue.
  pub async fn push_task(&self, task: CrawlTask) -> Result<(), Error> {
    tracing::debug!(url = %task.url, depth = task.depth, "Sending PushTask to broker");
    let (resp_tx, resp_rx) = oneshot::channel();
    let cmd = BrokerCommand::PushTask { task, resp_tx };

    // Send the command to the worker thread.
    self.tx.send(cmd).await?;

    // Wait for ACK.
    resp_rx.await?;

    Ok(())
  }

  /// Pops a task from the crawling queue.
  pub async fn pop_task(&self) -> Result<Option<CrawlTask>, Error> {
    tracing::debug!("Sending PopTask to broker");
    let (resp_tx, resp_rx) = oneshot::channel();
    let cmd = BrokerCommand::PopTask { resp_tx };

    // Send the command to the worker thread.
    self.tx.send(cmd).await?;

    // Wait for the response.
    let maybe_task = match resp_rx.await? {
      Some(task) => {
        tracing::debug!(url = %task.url, depth = task.depth, "Received task from broker");
        Some(task)
      }
      None => {
        tracing::debug!("Broker returned no task");
        None
      }
    };

    Ok(maybe_task)
  }

  /// Notifies the broker that a task has been completed.
  pub async fn event_task_done(
    &self,
    url: url::Url,
    status: u16,
    links_found: usize,
    admitted_links: usize,
  ) {
    tracing::debug!(
        %url,
        status,
        links_found,
        admitted_links,
        "Sending EventTaskDone to broker"
    );
    let cmd = BrokerCommand::EventTaskDone {
      url,
      status,
      links_found,
      admitted_links,
    };

    // Send the command to the worker thread. No ACK needed.
    if let Err(e) = self.tx.send(cmd).await {
      tracing::warn!("Failed to send EventTaskDone: {}", e);
    }
  }
}

/// Commands that can be sent to the worker thread.
pub enum BrokerCommand {
  PushTask {
    task: CrawlTask,
    resp_tx: oneshot::Sender<()>, // ACK
  },
  PopTask {
    resp_tx: oneshot::Sender<Option<CrawlTask>>,
  },
  EventTaskDone {
    url: url::Url,
    status: u16,
    links_found: usize,
    admitted_links: usize,
  },
}

/// Worker that processes commands from the MPSC channel.
pub struct BrokerWorker<Fr: Frontier, M: Monitor> {
  frontier: Fr,
  monitor: M,
  rx: mpsc::Receiver<BrokerCommand>,
}

impl<Fr: Frontier + Send + 'static, M: Monitor + Send + 'static> BrokerWorker<Fr, M> {
  /// Spawns a new broker worker thread.
  pub fn spawn(rx: mpsc::Receiver<BrokerCommand>, frontier: Fr, monitor: M) -> JoinHandle<()> {
    tokio::spawn(async move {
      Self {
        frontier,
        rx,
        monitor,
      }
      .run()
      .await;
    })
  }

  /// Runs the worker loop, processing commands from the channel.
  async fn run(&mut self) {
    tracing::info!("Broker worker started");
    while let Some(cmd) = self.rx.recv().await {
      match cmd {
        BrokerCommand::PushTask { task, resp_tx } => {
          tracing::debug!(url = %task.url, depth = task.depth, "Received PushTask");
          self.frontier.add(task);
          if let Err(_) = resp_tx.send(()) {
            tracing::warn!("PushTask response channel dropped before ACK");
          }
        }
        BrokerCommand::PopTask { resp_tx } => {
          tracing::debug!("Received PopTask");
          let maybe_task = match self.frontier.pop() {
            Some(task) => {
              tracing::debug!(url = %task.url, depth = task.depth, "Popped task from frontier");
              Some(task)
            }
            None => {
              tracing::debug!("No tasks available in frontier");
              None
            }
          };
          if let Err(_) = resp_tx.send(maybe_task) {
            tracing::warn!("PopTask response channel dropped");
          }
        }
        BrokerCommand::EventTaskDone {
          url,
          status,
          links_found,
          admitted_links,
        } => {
          tracing::info!(
              %url,
              status,
              links_found,
              admitted_links,
              "Received EventTaskDone"
          );
          self
            .monitor
            .on_task_ready(url, status, links_found, admitted_links);
          // No ACK needed.
        }
      }
    }

    tracing::warn!("Broker worker exited: MPSC receiver closed");
  }
}
