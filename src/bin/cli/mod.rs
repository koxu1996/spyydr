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
}
