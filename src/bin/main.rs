use std::sync::Arc;

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
  let fetcher = spyydr::components::fetcher::ReqwestFetcher::default();
  let extractor = spyydr::components::extractor::HtmlExtractor;
  let frontier = spyydr::components::frontier::BfsFrontier::default();
  let canonicalizer = spyydr::components::canonicalizer::NoQueryCanonicalizer;
  let policy = spyydr::components::policy::SimplePolicy::new(None, None, None);
  let registry = Arc::new(spyydr::components::registry::InMemoryRegistry::new());

  let mut engine = spyydr::engine::Engine::new(frontier, registry.clone()).await;
  let broker_svc = engine.get_broker_svc().clone();
  let registry = engine.get_registry();

  engine.init(&args.start_url).await?;
  let mut agent_handles = vec![];
  for _agent_id in 0..1 {
    let agent = spyydr::agent::Agent::new(
      fetcher.clone(),
      extractor.clone(),
      canonicalizer.clone(),
      policy.clone(),
      registry.clone(),
      broker_svc.clone(),
    )
    .await;
    let handle = tokio::spawn(async move {
      agent.run_infinite_loop().await;
    });
    agent_handles.push(handle);
  }

  // Wait for handles - currently never ending.
  for handle in agent_handles {
    handle.await.unwrap();
  }

  // Err(spyydr::error::Error::Unexpected("test error".to_string()))?

  Ok(())
}
