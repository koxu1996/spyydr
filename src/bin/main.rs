use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;
use color_eyre::eyre;
use tokio::task::JoinSet;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use spyydr::components::{canonicalizer, extractor, fetcher, frontier, monitor, policy, registry};
use spyydr::services::rate_limiter::RateLimiterService;
use spyydr::services::robots_txt::RobotsTxtService;
use spyydr::types::{RateLimitConfig, RobotsTxtConfig};

use crate::cli::Cli;

mod cli;

#[tokio::main]
async fn main() -> eyre::Result<()> {
  color_eyre::install()?;

  // Enable logging.
  init_tracing()?;

  // Parse command-line arguments.
  let args = Cli::parse();

  // Create shared shutdown signal, triggered via Ctrl+C.
  let shutdown = Arc::new(AtomicBool::new(false));
  {
    let shutdown = shutdown.clone();
    tokio::spawn(async move {
      tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");
      tracing::warn!("Received Ctrl+C, shutting down...");
      shutdown.store(true, Ordering::Relaxed);
    });
  }

  // Create engine components.
  let frontier = frontier::BfsFrontier::default();
  let registry = Arc::new(registry::InMemoryRegistry::new());

  // Create rate limiter service
  let rate_limit_config = RateLimitConfig {
    requests_per_second_per_host: args.rate_limit_rps,
    ..Default::default()
  };
  let rate_limiter = Arc::new(RateLimiterService::new(registry.clone(), rate_limit_config));
  tracing::info!("Rate limiter configured: {} RPS per host", args.rate_limit_rps);

  // Create robots.txt service
  let robots_txt_config = RobotsTxtConfig {
    enabled: args.respect_robots_txt,
    user_agent: args.robots_txt_user_agent.clone().unwrap_or_else(|| args.user_agent.clone()),
    cache_ttl: std::time::Duration::from_secs(args.robots_txt_cache_ttl),
    fetch_timeout: std::time::Duration::from_secs(args.request_timeout),
  };
  let robots_txt_service = Arc::new(RobotsTxtService::new(registry.clone(), robots_txt_config.clone()));
  tracing::info!("Robots.txt service configured: enabled={}, user_agent={}",
    robots_txt_config.enabled, robots_txt_config.user_agent);

  let (monitor, progress_monitor_handle) = create_monitor(args.clone(), shutdown.clone());
  let max_tasks_processed = args.max_pages;
  let mut engine = spyydr::orchestration::engine::Engine::new(
    frontier,
    registry.clone(),
    monitor,
    max_tasks_processed,
    shutdown.clone(),
  )
  .await;
  let broker_svc = engine.get_broker_svc().clone();
  let registry = engine.get_registry();

  // Pass the start URL to the engine.
  engine.init(&args.start_url).await?;

  // Run the agents.
  let mut agent_set = JoinSet::new();
  let num_agents = args.agents;
  for _agent_id in 0..num_agents {
    // Clone shared components.
    let args = args.clone();
    let registry = registry.clone();
    let broker_svc = broker_svc.clone();
    let rate_limiter = rate_limiter.clone();
    let robots_txt_service = robots_txt_service.clone();
    let shutdown = shutdown.clone();

    // Spawn agent handle.
    agent_set.spawn(async move {
      // Construct stateless components.
      let fetcher = create_fetcher(&args);
      let extractor = create_extractor(&args);
      let canonicalizer = create_canonicalizer(&args);
      let async_policy = create_robots_txt_policy(&args, robots_txt_service);

      // Construct agent.
      let agent = spyydr::orchestration::agent::Agent::new(
        fetcher,
        extractor,
        canonicalizer,
        async_policy,
        registry,
        broker_svc,
        rate_limiter,
      );

      // Run agent until shutdown signal is received.
      agent.run(shutdown).await;
    });
  }

  // Wait until agents finish.
  agent_set.join_all().await;

  // Wait until progress monitor finishes.
  progress_monitor_handle.await.unwrap();

  // Drop broker service to allow the broker worker to finish.
  drop(broker_svc);

  // Wait for engine shutdown.
  engine.wait_for_shutdown().await?;

  tracing::info!("Crawling stopped gracefully.");

  Ok(())
}

// Configure console logger, defaulting to "info" level.
fn init_tracing() -> eyre::Result<()> {
  let filter = tracing_subscriber::EnvFilter::builder()
    .with_default_directive("info".parse()?)
    .from_env()?;
  let layer = tracing_subscriber::fmt::layer();
  tracing_subscriber::registry()
    .with(filter)
    .with(layer)
    .try_init()?;
  Ok(())
}

fn create_fetcher(args: &Cli) -> fetcher::ReqwestFetcher {
  let config = fetcher::ReqwestFetcherConfig {
    user_agent: Some(args.user_agent.clone()),
    max_retries: args.max_retries,
    timeout: Some(std::time::Duration::from_secs(args.request_timeout)),
  };
  fetcher::ReqwestFetcher::new(config).unwrap()
}

fn create_extractor(_args: &Cli) -> impl extractor::Extractor {
  extractor::HtmlExtractor
}

fn create_canonicalizer(args: &Cli) -> impl canonicalizer::Canonicalizer {
  let enabled = !args.disable_canonicalizer;
  canonicalizer::QueryCanonicalizer::new(enabled)
}

fn create_robots_txt_policy(
  args: &Cli,
  robots_txt_service: Arc<RobotsTxtService<registry::InMemoryRegistry>>,
) -> policy::RobotsTxtPolicy<registry::InMemoryRegistry> {
  let simple_policy = policy::SimplePolicy::new(
    args.max_depth,
    args.whitelist.clone(),
    args.blacklist.clone(),
  );
  let user_agent = args.robots_txt_user_agent.clone().unwrap_or_else(|| args.user_agent.clone());
  policy::RobotsTxtPolicy::new(simple_policy, robots_txt_service, user_agent)
}

fn create_monitor(
  args: Cli,
  shutdown: Arc<AtomicBool>,
) -> (impl monitor::Monitor, tokio::task::JoinHandle<()>) {
  let mut monitors: Vec<Box<dyn monitor::Monitor + Send + Sync>> =
    vec![Box::new(monitor::ConsoleMonitor)];

  let progress_monitor = monitor::ProgressMonitor::default();
  let progress_handle = progress_monitor.spawn_reporter(args.progress_interval, shutdown.clone());
  monitors.push(Box::new(progress_monitor));

  if let Some(output_path) = &args.output {
    let saving_monitor = monitor::SavingMonitor::new(output_path.clone());
    monitors.push(Box::new(saving_monitor));
  }

  let multi_monitor = monitor::MultiMonitor(monitors);
  (multi_monitor, progress_handle)
}
