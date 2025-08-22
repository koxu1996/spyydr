use clap::Parser;
use color_eyre::eyre;

use crate::cli::Cli;

mod cli;

#[tokio::main]
async fn main() -> eyre::Result<()> {
  color_eyre::install()?;

  let args = Cli::parse();

  println!("Starting crawl at: {}", args.start_url);
  let fetcher = spyydr::components::fetcher::ReqwestFetcher::new();
  let extractor = spyydr::components::extractor::HtmlExtractor::default();
  let frontier = spyydr::components::frontier::BfsFrontier::new();
  let mut engine = spyydr::engine::Engine::new(fetcher, extractor, frontier);
  engine.init(&args.start_url)?;
  engine.run_once().await?;
  engine.run_once().await?;
  engine.run_once().await?;

  Err(spyydr::error::Error::Unexpected("test error".to_string()))?
}
