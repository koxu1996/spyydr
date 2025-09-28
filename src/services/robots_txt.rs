use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use url::Url;

use crate::components::registry::Registry;
use crate::types::{CachedRobotsTxt, RobotsTxtConfig, RobotsTxtError};

/// Commands for the robots.txt service.
pub enum RobotsTxtCommand {
    CheckUrl {
        url: Url,
        user_agent: String,
        response_tx: oneshot::Sender<bool>,
    },
    Shutdown,
}

/// Robots.txt service that fetches, caches, and validates URL permissions.
pub struct RobotsTxtService<R> {
    command_tx: mpsc::UnboundedSender<RobotsTxtCommand>,
    handle: Option<JoinHandle<()>>,
    _phantom: PhantomData<R>,
}

impl<R: Registry + 'static> RobotsTxtService<R> {
    pub fn new(registry: Arc<R>, config: RobotsTxtConfig) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let worker = RobotsTxtWorker {
            registry,
            config,
            command_rx,
        };

        let handle = tokio::spawn(worker.run());

        Self {
            command_tx,
            handle: Some(handle),
            _phantom: PhantomData,
        }
    }

    /// Check if a URL is allowed to be crawled according to robots.txt.
    /// Returns true if allowed, false if disallowed.
    pub async fn is_allowed(&self, url: Url, user_agent: &str) -> Result<bool, RobotsTxtError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(RobotsTxtCommand::CheckUrl {
                url,
                user_agent: user_agent.to_string(),
                response_tx,
            })
            .map_err(|_| RobotsTxtError::Communication)?;

        response_rx.await.map_err(|_| RobotsTxtError::Communication)
    }
}

impl<R> Drop for RobotsTxtService<R> {
    fn drop(&mut self) {
        let _ = self.command_tx.send(RobotsTxtCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

/// Internal worker that processes robots.txt commands.
struct RobotsTxtWorker<R> {
    registry: Arc<R>,
    config: RobotsTxtConfig,
    command_rx: mpsc::UnboundedReceiver<RobotsTxtCommand>,
}

impl<R: Registry> RobotsTxtWorker<R> {
    async fn run(mut self) {
        while let Some(command) = self.command_rx.recv().await {
            match command {
                RobotsTxtCommand::CheckUrl {
                    url,
                    user_agent,
                    response_tx,
                } => {
                    let allowed = self.check_url_allowed(&url, &user_agent).await;
                    let _ = response_tx.send(allowed);
                }
                RobotsTxtCommand::Shutdown => break,
            }
        }
    }

    async fn check_url_allowed(&self, url: &Url, user_agent: &str) -> bool {
        // If robots.txt checking is disabled, always allow
        if !self.config.enabled {
            return true;
        }

        // Extract host from URL (including port if present)
        let host = match url.host_str() {
            Some(h) => {
                // Include port if present
                match url.port() {
                    Some(port) => format!("{}:{}", h, port),
                    None => h.to_string(),
                }
            },
            None => {
                tracing::debug!(%url, "URL has no host, allowing");
                return true;
            }
        };


        // Get or fetch robots.txt for this host
        let robots_txt_content = match self.get_robots_txt(&host).await {
            Ok(content) => content,
            Err(e) => {
                tracing::warn!(%host, error = %e, "Failed to get robots.txt, allowing access");
                return true; // Default to allowing when robots.txt cannot be fetched
            }
        };

        // If no robots.txt content, allow everything
        let Some(content) = robots_txt_content else {
            tracing::debug!(%host, "No robots.txt found, allowing");
            return true;
        };

        // Parse robots.txt and check if URL is allowed
        match texting_robots::Robot::new(user_agent, content.as_bytes()) {
            Ok(robot) => {
                let allowed = robot.allowed(url.as_str());
                tracing::debug!(%url, %user_agent, %allowed, "Robots.txt check completed");
                allowed
            }
            Err(e) => {
                tracing::warn!(%host, error = %e, "Failed to parse robots.txt, allowing access");
                true // Default to allowing when robots.txt cannot be parsed
            }
        }
    }

    async fn get_robots_txt(&self, host: &str) -> Result<Option<String>, RobotsTxtError> {
        let cache_key = format!("robots_txt:{}", host);

        // Check cache first
        if let Ok(Some(cached)) = self.registry.get::<CachedRobotsTxt>(&cache_key) {
            if !cached.is_expired(self.config.cache_ttl) {
                tracing::debug!(%host, "Using cached robots.txt");
                return Ok(cached.content);
            }
            tracing::debug!(%host, "Cached robots.txt expired, refetching");
        }

        // Fetch robots.txt from the host
        // Try HTTPS first, then fall back to HTTP for test environments
        let robots_url = if host.contains("127.0.0.1") || host.contains("localhost") {
            format!("http://{}/robots.txt", host)
        } else {
            format!("https://{}/robots.txt", host)
        };

        let content = self.fetch_robots_txt(&robots_url).await?;

        // Cache the result
        let cached = CachedRobotsTxt::new(host.to_string(), content.clone());
        if let Err(e) = self.registry.set_with_ttl(&cache_key, &cached, self.config.cache_ttl) {
            tracing::warn!(%host, error = %e, "Failed to cache robots.txt");
        }

        Ok(content)
    }

    async fn fetch_robots_txt(&self, url: &str) -> Result<Option<String>, RobotsTxtError> {
        tracing::debug!(%url, "Fetching robots.txt");

        // Create HTTP client with timeout
        let client = reqwest::Client::builder()
            .timeout(self.config.fetch_timeout)
            .user_agent(&self.config.user_agent)
            .build()
            .map_err(|e| RobotsTxtError::Fetch(format!("Failed to create client: {}", e)))?;

        // Fetch robots.txt
        match client.get(url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.text().await {
                        Ok(content) => {
                            tracing::debug!(%url, "Successfully fetched robots.txt");
                            Ok(Some(content))
                        }
                        Err(e) => {
                            tracing::debug!(%url, error = %e, "Failed to read robots.txt body");
                            Ok(None)
                        }
                    }
                } else if response.status() == 404 {
                    tracing::debug!(%url, "No robots.txt found (404)");
                    Ok(None)
                } else {
                    tracing::debug!(%url, status = %response.status(), "Robots.txt fetch failed with status");
                    Ok(None)
                }
            }
            Err(e) => {
                tracing::debug!(%url, error = %e, "Failed to fetch robots.txt");
                Ok(None) // Network errors default to allowing
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::registry::InMemoryRegistry;
    use httpmock::prelude::*;

    fn create_test_service() -> RobotsTxtService<InMemoryRegistry> {
        let registry = Arc::new(InMemoryRegistry::new());
        let config = RobotsTxtConfig::default();
        RobotsTxtService::new(registry, config)
    }

    fn create_test_service_with_config(config: RobotsTxtConfig) -> RobotsTxtService<InMemoryRegistry> {
        let registry = Arc::new(InMemoryRegistry::new());
        RobotsTxtService::new(registry, config)
    }

    #[tokio::test]
    async fn should_allow_when_disabled() {
        let registry = Arc::new(InMemoryRegistry::new());
        let config = RobotsTxtConfig {
            enabled: false,
            ..Default::default()
        };
        let service = RobotsTxtService::new(registry, config);

        let url = Url::parse("https://example.com/forbidden").unwrap();
        let allowed = service.is_allowed(url, "TestBot").await.unwrap();

        assert!(allowed, "Should allow when robots.txt checking is disabled");
    }

    #[tokio::test]
    async fn should_allow_urls_without_host() {
        let service = create_test_service();

        let url = Url::parse("mailto:test@example.com").unwrap();
        let allowed = service.is_allowed(url, "TestBot").await.unwrap();

        assert!(allowed, "Should allow URLs without host");
    }

    #[tokio::test]
    async fn should_handle_missing_robots_txt_gracefully() {
        let service = create_test_service();

        // This will fail to fetch robots.txt (network error) and should default to allowing
        let url = Url::parse("https://nonexistent-domain-that-should-not-exist.com/").unwrap();
        let allowed = service.is_allowed(url, "TestBot").await.unwrap();

        assert!(allowed, "Should allow when robots.txt cannot be fetched");
    }

    #[tokio::test]
    async fn should_handle_404_robots_txt() {
        let server = MockServer::start();

        // Mock 404 response for robots.txt
        server.mock(|when, then| {
            when.method(GET)
                .path("/robots.txt");
            then.status(404);
        });

        let service = create_test_service();
        let url = Url::parse(&format!("http://{}/some-page", server.address())).unwrap();
        let allowed = service.is_allowed(url, "TestBot").await.unwrap();

        assert!(allowed, "Should allow when robots.txt returns 404");
    }

    #[tokio::test]
    async fn should_respect_disallow_all_robots_txt() {
        let server = MockServer::start();

        // Mock robots.txt that disallows everything
        let robots_content = r#"
User-agent: *
Disallow: /
"#;
        server.mock(|when, then| {
            when.method(GET)
                .path("/robots.txt");
            then.status(200)
                .header("content-type", "text/plain")
                .body(robots_content);
        });

        let service = create_test_service();
        let url = Url::parse(&format!("http://{}/any-page", server.address())).unwrap();
        let allowed = service.is_allowed(url, "TestBot").await.unwrap();

        assert!(!allowed, "Should deny access when robots.txt disallows all");
    }

    #[tokio::test]
    async fn should_respect_disallow_specific_path() {
        let server = MockServer::start();

        // Mock robots.txt that disallows specific paths
        let robots_content = r#"
User-agent: *
Allow: /
Disallow: /admin/
Disallow: /private/
"#;
        server.mock(|when, then| {
            when.method(GET)
                .path("/robots.txt");
            then.status(200)
                .header("content-type", "text/plain")
                .body(robots_content);
        });

        let service = create_test_service();

        // Test allowed path
        let allowed_url = Url::parse(&format!("http://{}/public/page", server.address())).unwrap();
        let allowed = service.is_allowed(allowed_url, "TestBot").await.unwrap();
        assert!(allowed, "Should allow access to public paths");

        // Test disallowed path
        let disallowed_url = Url::parse(&format!("http://{}/admin/panel", server.address())).unwrap();
        let allowed = service.is_allowed(disallowed_url, "TestBot").await.unwrap();
        assert!(!allowed, "Should deny access to /admin/ paths");

        // Test another disallowed path
        let disallowed_url2 = Url::parse(&format!("http://{}/private/data", server.address())).unwrap();
        let allowed = service.is_allowed(disallowed_url2, "TestBot").await.unwrap();
        assert!(!allowed, "Should deny access to /private/ paths");
    }

    #[tokio::test]
    async fn should_respect_user_agent_specific_rules() {
        let server = MockServer::start();

        // Mock robots.txt with user-agent specific rules
        let robots_content = r#"
User-agent: BadBot
Disallow: /

User-agent: TestBot
Allow: /public/
Disallow: /private/

User-agent: *
Disallow: /admin/
"#;
        server.mock(|when, then| {
            when.method(GET)
                .path("/robots.txt");
            then.status(200)
                .header("content-type", "text/plain")
                .body(robots_content);
        });

        let service = create_test_service();

        // Test TestBot access to allowed path
        let public_url = Url::parse(&format!("http://{}/public/page", server.address())).unwrap();
        let allowed = service.is_allowed(public_url, "TestBot").await.unwrap();
        assert!(allowed, "TestBot should be allowed to access /public/");

        // Test TestBot access to disallowed path
        let private_url = Url::parse(&format!("http://{}/private/data", server.address())).unwrap();
        let allowed = service.is_allowed(private_url, "TestBot").await.unwrap();
        assert!(!allowed, "TestBot should be denied access to /private/");

        // Test BadBot access (should be denied everything)
        let any_url = Url::parse(&format!("http://{}/anything", server.address())).unwrap();
        let allowed = service.is_allowed(any_url, "BadBot").await.unwrap();
        assert!(!allowed, "BadBot should be denied access to everything");

        // Test unknown bot access to admin (should be denied by wildcard rule)
        let admin_url = Url::parse(&format!("http://{}/admin/panel", server.address())).unwrap();
        let allowed = service.is_allowed(admin_url, "UnknownBot").await.unwrap();
        assert!(!allowed, "Unknown bots should be denied access to /admin/");
    }

    #[tokio::test]
    async fn should_cache_robots_txt() {
        let server = MockServer::start();

        let robots_content = r#"
User-agent: *
Disallow: /private/
"#;
        let robots_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/robots.txt");
            then.status(200)
                .header("content-type", "text/plain")
                .body(robots_content);
        });

        let service = create_test_service();

        // First request - should fetch robots.txt
        let url1 = Url::parse(&format!("http://{}/public/page1", server.address())).unwrap();
        let allowed1 = service.is_allowed(url1, "TestBot").await.unwrap();
        assert!(allowed1);

        // Second request - should use cached robots.txt
        let url2 = Url::parse(&format!("http://{}/public/page2", server.address())).unwrap();
        let allowed2 = service.is_allowed(url2, "TestBot").await.unwrap();
        assert!(allowed2);

        // Third request to disallowed path - should use cached robots.txt
        let url3 = Url::parse(&format!("http://{}/private/secret", server.address())).unwrap();
        let allowed3 = service.is_allowed(url3, "TestBot").await.unwrap();
        assert!(!allowed3);

        // Should have only made one HTTP request due to caching
        robots_mock.assert_hits(1);
    }

    #[tokio::test]
    async fn should_handle_server_errors_gracefully() {
        let server = MockServer::start();

        // Mock 500 error for robots.txt
        server.mock(|when, then| {
            when.method(GET)
                .path("/robots.txt");
            then.status(500)
                .body("Internal Server Error");
        });

        let service = create_test_service();
        let url = Url::parse(&format!("http://{}/some-page", server.address())).unwrap();
        let allowed = service.is_allowed(url, "TestBot").await.unwrap();

        assert!(allowed, "Should allow when robots.txt returns server error");
    }

    #[tokio::test]
    async fn should_handle_invalid_robots_txt_content() {
        let server = MockServer::start();

        // Mock invalid robots.txt content
        server.mock(|when, then| {
            when.method(GET)
                .path("/robots.txt");
            then.status(200)
                .header("content-type", "text/plain")
                .body("This is not valid robots.txt content at all!");
        });

        let service = create_test_service();
        let url = Url::parse(&format!("http://{}/some-page", server.address())).unwrap();
        let allowed = service.is_allowed(url, "TestBot").await.unwrap();

        assert!(allowed, "Should allow when robots.txt content is invalid");
    }

    #[tokio::test]
    async fn should_respect_custom_user_agent() {
        let server = MockServer::start();

        let robots_content = r#"
User-agent: CustomBot
Disallow: /secret/

User-agent: *
Allow: /
"#;
        server.mock(|when, then| {
            when.method(GET)
                .path("/robots.txt");
            then.status(200)
                .header("content-type", "text/plain")
                .body(robots_content);
        });

        let config = RobotsTxtConfig {
            enabled: true,
            user_agent: "CustomBot".to_string(),
            ..Default::default()
        };
        let service = create_test_service_with_config(config);

        // Test access to secret path with CustomBot - should be denied
        let secret_url = Url::parse(&format!("http://{}/secret/data", server.address())).unwrap();
        let allowed = service.is_allowed(secret_url, "CustomBot").await.unwrap();
        assert!(!allowed, "CustomBot should be denied access to /secret/");

        // Test access to public path with CustomBot - should be allowed
        let public_url = Url::parse(&format!("http://{}/public/data", server.address())).unwrap();
        let allowed = service.is_allowed(public_url, "CustomBot").await.unwrap();
        assert!(allowed, "CustomBot should be allowed access to /public/");
    }

    #[tokio::test]
    async fn should_handle_timeout() {
        let server = MockServer::start();

        // Mock slow response that will timeout
        server.mock(|when, then| {
            when.method(GET)
                .path("/robots.txt");
            then.status(200)
                .delay(std::time::Duration::from_secs(2))
                .body("User-agent: *\nDisallow: /");
        });

        let config = RobotsTxtConfig {
            enabled: true,
            fetch_timeout: std::time::Duration::from_millis(100), // Very short timeout
            ..Default::default()
        };
        let service = create_test_service_with_config(config);

        let url = Url::parse(&format!("http://{}/some-page", server.address())).unwrap();
        let allowed = service.is_allowed(url, "TestBot").await.unwrap();

        assert!(allowed, "Should allow when robots.txt request times out");
    }
}