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

  let args = Cli::parse();

  println!("Starting crawl at: {}", args.start_url);
  let fetcher_config = spyydr::components::fetcher::ReqwestFetcherConfig {
    user_agent: Some(args.user_agent.clone()),
    max_retries: args.max_retries,
    timeout: Some(std::time::Duration::from_secs(args.request_timeout)),
  };
  let fetcher = spyydr::components::fetcher::ReqwestFetcher::new(fetcher_config);
  let extractor = spyydr::components::extractor::HtmlExtractor;
  let frontier = spyydr::components::frontier::BfsFrontier::default();
  let canonicalizer = spyydr::components::canonicalizer::NoQueryCanonicalizer;
  let policy =
    spyydr::components::policy::SimplePolicy::new(args.max_depth, args.whitelist, args.blacklist);
  let registry = Arc::new(spyydr::components::registry::InMemoryRegistry::new());

  // Monitoring setup.
  let console_monitor = spyydr::components::monitor::ConsoleMonitor;
  let mut monitors: Vec<Box<dyn spyydr::components::monitor::Monitor + Send + Sync>> =
    vec![Box::new(console_monitor)];
  let progress_monitor = spyydr::components::monitor::ProgressMonitor::default();
  progress_monitor.spawn_reporter(args.progress_interval);
  monitors.push(Box::new(progress_monitor));
  if let Some(output_path) = &args.output {
    let saving_monitor = spyydr::components::monitor::SavingMonitor::new(output_path.clone());
    monitors.push(Box::new(saving_monitor));
  }
  let monitor = spyydr::components::monitor::MultiMonitor(monitors);

  let mut engine =
    spyydr::orchestration::engine::Engine::new(frontier, registry.clone(), monitor).await;
  let broker_svc = engine.get_broker_svc().clone();
  let registry = engine.get_registry();

  // Triggering shutdown signal via Ctrl+C.
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

  engine.init(&args.start_url).await?;
  let mut agent_handles = vec![];
  let num_agents = args.agents;
  for _agent_id in 0..num_agents {
    let agent = spyydr::orchestration::agent::Agent::new(
      fetcher.clone(),
      extractor.clone(),
      canonicalizer.clone(),
      policy.clone(),
      registry.clone(),
      broker_svc.clone(),
    )
    .await;
    let shutdown = shutdown.clone();
    let handle = tokio::spawn(async move {
      agent.run(shutdown).await;
    });
    agent_handles.push(handle);
  }

  // Wait until agents finish.
  for handle in agent_handles {
    handle.await.unwrap();
  }

  // Wait for engine shutdown.
  engine.wait_for_shutdown().await?;

  println!("Crawl finished gracefully.");

  Ok(())
}
