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

  /// Maximum number of unique pages to crawl.
  #[arg(long, default_value_t = 100)]
  pub max: usize,

  /// Maximum number of concurrent requests.
  #[arg(short, long, default_value_t = 5)]
  pub concurrency: usize,

  /// Custom User-Agent header.
  #[arg(
    long,
    default_value = "SpyydrBot/1.0 (+https://gitlab.com/andrzej1_1/spyydr)"
  )]
  pub user_agent: String,

  /// Whitelist: only follow links containing any of these domains (comma-separated).
  #[arg(long, value_delimiter = ',', value_name = "DOMAINS")]
  pub whitelist: Option<Vec<String>>,

  /// Blacklist: skip links containing any of these domains (comma-separated).
  #[arg(long, value_delimiter = ',', value_name = "DOMAINS")]
  pub blacklist: Option<Vec<String>>,

  /// HTTP proxy to use (e.g., http://127.0.0.1:8080).
  #[arg(long, value_name = "URL", value_hint = ValueHint::Url)]
  pub proxy: Option<String>,

  /// Timeout in seconds for each HTTP request.
  #[arg(long, default_value_t = 10)]
  pub timeout: u64,

  /// Number of times to retry a failed request (e.g., 429 or timeout).
  #[arg(long, default_value_t = 1)]
  pub retry: u32,

  /// Silent mode - do not print visited URLs and links.
  #[arg(long)]
  pub silent: bool,

  /// Output in JSON format (instead of plain text).
  #[arg(long)]
  pub json: bool,
}
