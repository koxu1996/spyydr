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
  let fetcher = spyydr::components::fetcher::ReqwestFetcher::new();
  let extractor = spyydr::components::extractor::HtmlExtractor::default();
  let frontier = spyydr::components::frontier::BfsFrontier::new();
  let canonicalizer = spyydr::components::canonicalizer::NoQueryCanonicalizer::default();
  let policy = spyydr::components::policy::SimplePolicy::new(None, None, None);
  let registry = spyydr::components::registry::InMemoryRegistry::new();
  let mut engine = spyydr::engine::Engine::new(
    fetcher,
    extractor,
    frontier,
    canonicalizer,
    policy,
    registry,
  )
  .await;
  engine.init(&args.start_url).await?;
  for _ in 0..3 {
    let task = engine.get_next_task().await?.unwrap();
    engine.run_once(task).await?;
  }

  // Err(spyydr::error::Error::Unexpected("test error".to_string()))?

  Ok(())
}
