use tokio::sync::{mpsc, oneshot};

use crate::components::frontier::Frontier;
use crate::types::CrawlTask;

/// Service that provides a thread-safe *interface* to the crawling queue (frontier).
/// It requires a separate worker to handle requests via an MPSC channel.
#[derive(Clone)]
pub struct CrawlBroker {
  tx: mpsc::Sender<Command>,
}

/// Commands that can be sent to the worker thread.
enum Command {
  PushTask {
    task: CrawlTask,
    resp_tx: oneshot::Sender<()>, // ACK
  },
  PopTask {
    resp_tx: oneshot::Sender<Option<CrawlTask>>,
  },
}

/// Worker that processes commands from the MPSC channel.
pub struct Worker<Fr: Frontier> {
  frontier: Fr,
  rx: mpsc::Receiver<Command>,
}

impl<Fr: Frontier> Worker<Fr> {
  // NOTE: Usually I would use `tokio::spawn()` instead of constructor,
  // but here we are free to run in the main thread.
  fn new(rx: mpsc::Receiver<Command>, frontier: Fr) -> Self {
    Self { frontier, rx }
  }

  /// Runs the worker loop, processing commands from the channel.
  async fn run(mut self) {
    while let Some(cmd) = self.rx.recv().await {
      match cmd {
        Command::PushTask { task, resp_tx } => {
          self.frontier.add(task);
          let _ = resp_tx.send(()); // ACK
        }
        Command::PopTask { resp_tx } => {
          let maybe_task = self.frontier.pop();
          let _ = resp_tx.send(maybe_task);
        }
      }
    }
  }
}
