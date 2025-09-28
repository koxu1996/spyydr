use std::path::PathBuf;

use clap::{Parser, ValueHint};

#[derive(Parser, Debug, Clone)]
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
It starts from a seed URL, follows HTML links to discover new pages, and reports them.

NOTE: This CLI is a minimal example built on modular crawler components."#,
  after_help = "Project: https://github.com/koxu1996/spyydr"
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
    default_value = "SpyydrBot/1.0 (+https://github.com/koxu1996/spyydr)"
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

  /// Disable URL canonicalization (removing query parameters).
  #[arg(long)]
  pub disable_canonicalizer: bool,

  /// Maximum number of pages to fetch before stopping
  #[arg(long)]
  pub max_pages: Option<usize>,

  /// Maximum requests per second per host (rate limiting)
  #[arg(long, default_value_t = 1.0)]
  pub rate_limit_rps: f64,

  /// Respect robots.txt files when crawling
  #[arg(long, default_value_t = true)]
  pub respect_robots_txt: bool,

  /// User-Agent to use for robots.txt checks (defaults to main user-agent)
  #[arg(long)]
  pub robots_txt_user_agent: Option<String>,

  /// Cache TTL for robots.txt files in seconds
  #[arg(long, default_value_t = 86400)] // 24 hours
  pub robots_txt_cache_ttl: u64,
}

/// Checks that the number of agents is a positive integer.
fn validate_agents(s: &str) -> Result<usize, String> {
  let val: usize = s.parse().map_err(|_| format!("Invalid number: {s}"))?;
  if val == 0 {
    Err("Number of agents must be greater than 0".into())
  } else {
    Ok(val)
  }
}
