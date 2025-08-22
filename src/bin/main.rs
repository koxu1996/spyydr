use clap::Parser;
use color_eyre::eyre;

use crate::cli::Cli;

mod cli;

#[tokio::main]
async fn main() -> eyre::Result<()> {
  color_eyre::install()?;

  let args = Cli::parse();

  println!("Starting crawl at: {}", args.start_url);

  Err(spyydr::error::Error::Unexpected("test error".to_string()))?
}
