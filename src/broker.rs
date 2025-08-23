use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::components::frontier::Frontier;
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
    let (resp_tx, resp_rx) = oneshot::channel();
    let cmd = BrokerCommand::PushTask { task, resp_tx };

    // Send the command to the worker thread.
    self.tx.send(cmd).await.unwrap(); // TODO: Handle error.

    // Wait for ACK.
    resp_rx.await.unwrap(); // TODO: Handle error.

    Ok(())
  }

  /// Pops a task from the crawling queue.
  pub async fn pop_task(&self) -> Result<Option<CrawlTask>, Error> {
    let (resp_tx, resp_rx) = oneshot::channel();
    let cmd = BrokerCommand::PopTask { resp_tx };

    // Send the command to the worker thread.
    self.tx.send(cmd).await.unwrap(); // TODO: Handle error.

    // Wait for the response.
    let maybe_task = resp_rx.await.unwrap(); // TODO: Handle error.

    Ok(maybe_task)
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
}

/// Worker that processes commands from the MPSC channel.
pub struct BrokerWorker<Fr: Frontier> {
  frontier: Fr,
  rx: mpsc::Receiver<BrokerCommand>,
}

impl<Fr: Frontier + Send + 'static> BrokerWorker<Fr> {
  /// Spawns a new broker worker thread.
  pub fn spawn(rx: mpsc::Receiver<BrokerCommand>, frontier: Fr) -> JoinHandle<()> {
    tokio::spawn(async move {
      Self { frontier, rx }.run().await;
    })
  }

  /// Runs the worker loop, processing commands from the channel.
  async fn run(&mut self) {
    while let Some(cmd) = self.rx.recv().await {
      match cmd {
        BrokerCommand::PushTask { task, resp_tx } => {
          self.frontier.add(task);
          let _ = resp_tx.send(()); // ACK
        }
        BrokerCommand::PopTask { resp_tx } => {
          let maybe_task = self.frontier.pop();
          let _ = resp_tx.send(maybe_task);
        }
      }
    }
  }
}
