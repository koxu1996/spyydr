use clap::Parser;

use crate::cli::Cli;

mod cli;

fn main() {
  let args = Cli::parse();

  println!("Starting crawl at: {}", args.start_url);
}
