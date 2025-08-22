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
  let engine = spyydr::engine::Engine::new(fetcher, extractor);
  engine.run_once(&args.start_url).await?;

  Err(spyydr::error::Error::Unexpected("test error".to_string()))?
}
