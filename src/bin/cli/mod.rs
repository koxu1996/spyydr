use std::path::PathBuf;

use clap::{Parser, ValueHint};

#[derive(Parser, Debug)]
#[command(
  name = "Spyydr 🕷️",
  version,
  author = "Andrzej Broński <koxu1996@gmail.com>",
  long_about = None,
  about = r#"
 _____                       _
/  ___|                     | |
\ `--. _ __  _   _ _   _  __| |_ __
 `--. \ '_ \| | | | | | |/ _` | '__|
/\__/ / |_) | |_| | |_| | (_| | |
\____/| .__/ \__, |\__, |\__,_|_|
      | |     __/ | __/ |
      |_|    |___/ |___/

Spyydr 🕷️ - A simple yet powerful web crawler built in Rust.
It takes a starting URL, crawls linked pages, and prints out discovered links."#,
  after_help = "Project: https://gitlab.com/andrzej1_1/spyydr"
)]
pub struct Cli {
  /// Starting URL (seed) for the crawl.
  #[arg(value_name = "START_URL", value_hint = ValueHint::Url)]
  pub start_url: String,

  /// Number of concurrent agents (crawlers) to run.
  #[arg(short, long, default_value_t = 1, value_parser = validate_agents)]
  pub agents: usize,

  /// User-Agent header.
  #[arg(
    long,
    default_value = "SpyydrBot/1.0 (+https://gitlab.com/andrzej1_1/spyydr)"
  )]
  pub user_agent: String,

  /// Max number of retries on transient HTTP errors (e.g. 429, 5xx).
  #[arg(long, default_value_t = 5)]
  pub max_retries: usize,

  /// Timeout in seconds for each HTTP request.
  #[arg(long, default_value_t = 10)]
  pub request_timeout: u64,

  /// Maximum crawl depth (0 = only start URL, 1 = start + direct links, etc.).
  #[arg(long)]
  pub max_depth: Option<u32>,

  /// Only allow crawling domains that match these suffixes (case-insensitive).
  #[arg(long, value_delimiter = ',', num_args = 1..)]
  pub whitelist: Option<Vec<String>>,

  /// Disallow crawling domains that match these suffixes (case-insensitive).
  #[arg(long, value_delimiter = ',', num_args = 1..)]
  pub blacklist: Option<Vec<String>>,

  /// Interval in seconds for printing progress reports.
  #[arg(long, default_value_t = 10)]
  pub progress_interval: u64,

  /// Save crawled pages as JSON lines to the given file.
  #[arg(long, value_name = "FILE")]
  pub output: Option<PathBuf>,
}

fn validate_agents(s: &str) -> Result<usize, String> {
  let val: usize = s.parse().map_err(|_| format!("Invalid number: {s}"))?;
  if val == 0 {
    Err("Number of agents must be greater than 0".into())
  } else {
    Ok(val)
  }
}
