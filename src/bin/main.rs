use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;
use color_eyre::eyre;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::cli::Cli;

mod cli;

#[tokio::main]
async fn main() -> eyre::Result<()> {
  color_eyre::install()?;

  // Enable console logger, defaulting to "info" level.
  let filter = tracing_subscriber::EnvFilter::builder()
    .with_default_directive("info".parse()?)
    .from_env()?;
  let layer = tracing_subscriber::fmt::layer();
  tracing_subscriber::registry()
    .with(filter)
    .with(layer)
    .init();

  // Parse command-line arguments.
  let args = Cli::parse();

  // Create shared shutdown signal, triggered via Ctrl+C.
  let shutdown = Arc::new(AtomicBool::new(false));
  {
    let shutdown = shutdown.clone();
    tokio::spawn(async move {
      tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");
      eprintln!("Received Ctrl+C, shutting down...");
      shutdown.store(true, Ordering::Relaxed);
    });
  }

  // Create engine components.
  let frontier = spyydr::components::frontier::BfsFrontier::default();
  let registry = Arc::new(spyydr::components::registry::InMemoryRegistry::new());
  let (monitor, progress_monitor_handle) = create_monitor(args.clone(), shutdown.clone());
  let mut engine =
    spyydr::orchestration::engine::Engine::new(frontier, registry.clone(), monitor).await;
  let broker_svc = engine.get_broker_svc().clone();
  let registry = engine.get_registry();

  // Pass the start URL to the engine.
  engine.init(&args.start_url).await?;

  // Run the agents.
  let mut agent_handles = vec![];
  let num_agents = args.agents;
  for _agent_id in 0..num_agents {
    // Clone shared components.
    let args = args.clone();
    let registry = registry.clone();
    let broker_svc = broker_svc.clone();
    let shutdown = shutdown.clone();

    // Spawn agent handle.
    let handle = tokio::spawn(async move {
      // Construct stateless components.
      let fetcher = create_fetcher(&args);
      let extractor = create_extractor();
      let canonicalizer = create_canonicalizer();
      let policy = create_policy(&args);

      // Construct agent.
      let agent = spyydr::orchestration::agent::Agent::new(
        fetcher,
        extractor,
        canonicalizer,
        policy,
        registry,
        broker_svc,
      )
      .await;

      // Run agent until shutdown signal is received.
      agent.run(shutdown).await;
    });
    agent_handles.push(handle);
  }

  // Wait until agents finish.
  for handle in agent_handles {
    handle.await.unwrap();
  }

  // Wait until progress monitor finishes.
  progress_monitor_handle.await.unwrap();

  // Drop broker service to allow the broker worker to finish.
  drop(broker_svc);

  // Wait for engine shutdown.
  engine.wait_for_shutdown().await?;

  println!("Crawling stopped gracefully.");

  Ok(())
}

fn create_fetcher(args: &Cli) -> spyydr::components::fetcher::ReqwestFetcher {
  let config = spyydr::components::fetcher::ReqwestFetcherConfig {
    user_agent: Some(args.user_agent.clone()),
    max_retries: args.max_retries,
    timeout: Some(std::time::Duration::from_secs(args.request_timeout)),
  };
  spyydr::components::fetcher::ReqwestFetcher::new(config)
}

fn create_extractor() -> impl spyydr::components::extractor::Extractor {
  spyydr::components::extractor::HtmlExtractor
}

fn create_canonicalizer() -> impl spyydr::components::canonicalizer::Canonicalizer {
  spyydr::components::canonicalizer::NoQueryCanonicalizer
}

fn create_policy(args: &Cli) -> impl spyydr::components::policy::Policy {
  spyydr::components::policy::SimplePolicy::new(
    args.max_depth,
    args.whitelist.clone(),
    args.blacklist.clone(),
  )
}

fn create_monitor(
  args: Cli,
  shutdown: Arc<AtomicBool>,
) -> (
  impl spyydr::components::monitor::Monitor,
  tokio::task::JoinHandle<()>,
) {
  let mut monitors: Vec<Box<dyn spyydr::components::monitor::Monitor + Send + Sync>> =
    vec![Box::new(spyydr::components::monitor::ConsoleMonitor)];

  let progress_monitor = spyydr::components::monitor::ProgressMonitor::default();
  let progress_handle = progress_monitor.spawn_reporter(args.progress_interval, shutdown.clone());
  monitors.push(Box::new(progress_monitor));

  if let Some(output_path) = &args.output {
    let saving_monitor = spyydr::components::monitor::SavingMonitor::new(output_path.clone());
    monitors.push(Box::new(saving_monitor));
  }

  let multi_monitor = spyydr::components::monitor::MultiMonitor(monitors);
  (multi_monitor, progress_handle)
}
