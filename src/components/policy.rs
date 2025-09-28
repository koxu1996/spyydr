use std::sync::Arc;
use url::Url;

use crate::types::PolicyVerdict;
use crate::services::robots_txt::RobotsTxtService;
use crate::components::registry::Registry;

/// Policy decides if newly discovered link should be admitted.
pub trait Policy {
  fn evaluate(&self, candidate: &Url, from: &Url, depth: u32) -> PolicyVerdict;
}

/// Async policy trait for policies that need to perform async operations (like robots.txt checking).
pub trait AsyncPolicy {
  #[allow(async_fn_in_trait)]
  async fn evaluate(&self, candidate: &Url, from: &Url, depth: u32) -> PolicyVerdict;
}

/// Policy that enforces max depth, whitelist, and blacklist.
#[derive(Clone, Debug)]
pub struct SimplePolicy {
  max_depth: Option<u32>,
  whitelist: Vec<String>, // Lowercase domain suffixes.
  blacklist: Vec<String>, // Lowercase domain suffixes.
}

impl SimplePolicy {
  pub fn new(
    max_depth: Option<u32>,
    whitelist: Option<Vec<String>>,
    blacklist: Option<Vec<String>>,
  ) -> Self {
    Self {
      max_depth,
      whitelist: whitelist
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .collect(),
      blacklist: blacklist
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .collect(),
    }
  }

  #[inline]
  fn host(u: &Url) -> Option<String> {
    u.host_str().map(|h| h.to_ascii_lowercase())
  }

  #[inline]
  fn matches_suffix(list: &[String], host: &str) -> bool {
    list
      .iter()
      .any(|list_item| host == list_item || host.ends_with(&format!(".{list_item}")))
  }
}

impl Policy for SimplePolicy {
  fn evaluate(&self, candidate: &Url, _from: &Url, depth: u32) -> PolicyVerdict {
    // Check crawling depth.
    if let Some(md) = self.max_depth
      && depth > md
    {
      tracing::debug!(%candidate, depth, max_depth = md, decision = "deny", reason = "depth");
      return PolicyVerdict::deny("depth");
    }

    // Check against the whitelist and blacklist.
    let Some(h) = Self::host(candidate) else {
      tracing::debug!(%candidate, decision = "deny", reason = "no-host");
      return PolicyVerdict::deny("no-host");
    };
    if !self.whitelist.is_empty() && !Self::matches_suffix(&self.whitelist, &h) {
      tracing::debug!(%candidate, %h, wl_len = self.whitelist.len(), decision = "deny", reason = "whitelist");
      return PolicyVerdict::deny("whitelist");
    }
    if !self.blacklist.is_empty() && Self::matches_suffix(&self.blacklist, &h) {
      tracing::debug!(%candidate, %h, bl_len = self.blacklist.len(), decision = "deny", reason = "blacklist");
      return PolicyVerdict::deny("blacklist");
    }

    tracing::debug!(%candidate, %h, decision = "allow");
    PolicyVerdict::ALLOW
  }
}

impl AsyncPolicy for SimplePolicy {
  async fn evaluate(&self, candidate: &Url, from: &Url, depth: u32) -> PolicyVerdict {
    // Just delegate to the sync version
    Policy::evaluate(self, candidate, from, depth)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use url::Url;

  fn url(s: &str) -> Url {
    Url::parse(s).expect("valid url")
  }

  fn from_url() -> Url {
    url("https://from.example/")
  }

  #[test]
  fn denies_when_depth_exceeds_max() {
    let p = SimplePolicy::new(Some(2), None, None);
    let v = Policy::evaluate(&p, &url("https://a.test/"), &from_url(), 3);
    match v {
      PolicyVerdict::ALLOW => panic!("expected deny when depth > max_depth"),
      _ => {} // denied as expected
    }
  }

  #[test]
  fn allows_when_depth_equals_max() {
    let p = SimplePolicy::new(Some(2), None, None);
    let v = Policy::evaluate(&p, &url("https://a.test/"), &from_url(), 2);
    match v {
      PolicyVerdict::ALLOW => {} // ok
      _ => panic!("expected allow when depth == max_depth"),
    }
  }

  #[test]
  fn enforces_whitelist_case_insensitive_and_suffix() {
    // Whitelist only "example.com" (any subdomain should pass).
    let p = SimplePolicy::new(None, Some(vec!["ExAmPlE.CoM".into()]), None);

    // In whitelist (suffix match)
    let v_ok = Policy::evaluate(&p, &url("https://Sub.EXAMPLE.com/path"), &from_url(), 0);
    match v_ok {
      PolicyVerdict::ALLOW => {} // ok
      _ => panic!("expected allow for whitelisted host (suffix, case-insensitive)"),
    }

    // Not in whitelist
    let v_bad = Policy::evaluate(&p, &url("https://other.test/"), &from_url(), 0);
    match v_bad {
      PolicyVerdict::ALLOW => panic!("expected deny for non-whitelisted host"),
      _ => {} // denied as expected
    }
  }

  #[test]
  fn enforces_blacklist_with_suffix_match() {
    // Blacklist example.com and its subdomains.
    let p = SimplePolicy::new(None, None, Some(vec!["example.com".into()]));

    let v = Policy::evaluate(&p, &url("https://api.example.com/"), &from_url(), 0);
    match v {
      PolicyVerdict::ALLOW => panic!("expected deny for blacklisted host (suffix)"),
      _ => {} // denied as expected
    }

    // A different domain should be allowed (since no whitelist)
    let v_ok = Policy::evaluate(&p, &url("https://good.test/"), &from_url(), 0);
    match v_ok {
      PolicyVerdict::ALLOW => {} // ok
      _ => panic!("expected allow for non-blacklisted host"),
    }
  }

  #[test]
  fn denies_urls_without_host() {
    let p = SimplePolicy::new(None, None, None);

    // mailto/data URLs have no host
    let v1 = Policy::evaluate(&p, &url("mailto:someone@example.com"), &from_url(), 0);
    let v2 = Policy::evaluate(&p, &url("data:text/plain,hello"), &from_url(), 0);

    match v1 {
      PolicyVerdict::ALLOW => panic!("expected deny for mailto (no host)"),
      _ => {}
    }
    match v2 {
      PolicyVerdict::ALLOW => panic!("expected deny for data: (no host)"),
      _ => {}
    }
  }

  #[test]
  fn allows_simple_happy_path() {
    let p = SimplePolicy::new(None, None, None);
    let v = Policy::evaluate(&p, &url("https://ok.example/"), &from_url(), 0);
    match v {
      PolicyVerdict::ALLOW => {} // ok
      _ => panic!("expected allow when no limits and normal host"),
    }
  }
}

/// Combined policy that checks both SimplePolicy rules and robots.txt compliance.
pub struct RobotsTxtPolicy<R: Registry> {
  simple_policy: SimplePolicy,
  robots_txt_service: Arc<RobotsTxtService<R>>,
  user_agent: String,
}

impl<R: Registry> RobotsTxtPolicy<R> {
  pub fn new(
    simple_policy: SimplePolicy,
    robots_txt_service: Arc<RobotsTxtService<R>>,
    user_agent: String,
  ) -> Self {
    Self {
      simple_policy,
      robots_txt_service,
      user_agent,
    }
  }
}

impl<R: Registry + 'static> AsyncPolicy for RobotsTxtPolicy<R> {
  async fn evaluate(&self, candidate: &Url, from: &Url, depth: u32) -> PolicyVerdict {
    // First check the simple policy (depth, whitelist, blacklist)
    let simple_verdict = Policy::evaluate(&self.simple_policy, candidate, from, depth);
    if !simple_verdict.allowed {
      return simple_verdict;
    }

    // If simple policy allows, check robots.txt
    match self.robots_txt_service.is_allowed(candidate.clone(), &self.user_agent).await {
      Ok(allowed) => {
        if allowed {
          tracing::debug!(%candidate, "Allowed by robots.txt");
          PolicyVerdict::ALLOW
        } else {
          tracing::debug!(%candidate, "Denied by robots.txt");
          PolicyVerdict::deny("robots.txt")
        }
      }
      Err(e) => {
        tracing::warn!(%candidate, error = %e, "Robots.txt check failed, allowing");
        PolicyVerdict::ALLOW // Default to allowing when robots.txt check fails
      }
    }
  }
}

#[cfg(test)]
mod robots_txt_policy_tests {
  use super::*;
  use crate::components::registry::InMemoryRegistry;
  use crate::types::RobotsTxtConfig;
  use httpmock::prelude::*;

  #[tokio::test]
  async fn should_respect_simple_policy_denials() {
    let simple_policy = SimplePolicy::new(Some(1), None, None); // Max depth 1
    let registry = Arc::new(InMemoryRegistry::new());
    let robots_service = Arc::new(RobotsTxtService::new(registry, RobotsTxtConfig::default()));
    let policy = RobotsTxtPolicy::new(simple_policy, robots_service, "TestBot".to_string());

    let url = Url::parse("https://example.com/deep").unwrap();
    let from = Url::parse("https://example.com/").unwrap();

    // Depth 2 should be denied by simple policy, robots.txt shouldn't even be checked
    let verdict = policy.evaluate(&url, &from, 2).await;
    assert!(!verdict.allowed);
    assert_eq!(verdict.reason, "depth");
  }

  #[tokio::test]
  async fn should_allow_when_both_policies_allow() {
    let simple_policy = SimplePolicy::new(None, None, None); // No restrictions
    let registry = Arc::new(InMemoryRegistry::new());
    let robots_service = Arc::new(RobotsTxtService::new(registry, RobotsTxtConfig::default()));
    let policy = RobotsTxtPolicy::new(simple_policy, robots_service, "TestBot".to_string());

    let url = Url::parse("https://nonexistent-test-domain.com/").unwrap();
    let from = Url::parse("https://example.com/").unwrap();

    // Should be allowed (simple policy allows, robots.txt fails to fetch so defaults to allow)
    let verdict = policy.evaluate(&url, &from, 0).await;
    assert!(verdict.allowed);
  }

  #[tokio::test]
  async fn should_deny_when_robots_txt_forbids() {
    let server = MockServer::start();

    // Mock robots.txt that disallows /admin/
    let robots_content = r#"
User-agent: *
Allow: /
Disallow: /admin/
"#;
    server.mock(|when, then| {
      when.method(GET)
          .path("/robots.txt");
      then.status(200)
          .header("content-type", "text/plain")
          .body(robots_content);
    });

    let simple_policy = SimplePolicy::new(None, None, None); // No restrictions
    let registry = Arc::new(InMemoryRegistry::new());
    let robots_service = Arc::new(RobotsTxtService::new(registry, RobotsTxtConfig::default()));
    let policy = RobotsTxtPolicy::new(simple_policy, robots_service, "TestBot".to_string());

    let admin_url = Url::parse(&format!("http://{}/admin/panel", server.address())).unwrap();
    let from = Url::parse("https://example.com/").unwrap();

    // Should be denied by robots.txt
    let verdict = policy.evaluate(&admin_url, &from, 0).await;
    assert!(!verdict.allowed);
    assert_eq!(verdict.reason, "robots.txt");
  }

  #[tokio::test]
  async fn should_allow_when_robots_txt_permits() {
    let server = MockServer::start();

    // Mock robots.txt that allows /public/ but disallows /admin/
    let robots_content = r#"
User-agent: *
Allow: /public/
Disallow: /admin/
"#;
    server.mock(|when, then| {
      when.method(GET)
          .path("/robots.txt");
      then.status(200)
          .header("content-type", "text/plain")
          .body(robots_content);
    });

    let simple_policy = SimplePolicy::new(None, None, None); // No restrictions
    let registry = Arc::new(InMemoryRegistry::new());
    let robots_service = Arc::new(RobotsTxtService::new(registry, RobotsTxtConfig::default()));
    let policy = RobotsTxtPolicy::new(simple_policy, robots_service, "TestBot".to_string());

    let public_url = Url::parse(&format!("http://{}/public/page", server.address())).unwrap();
    let from = Url::parse("https://example.com/").unwrap();

    // Should be allowed by robots.txt
    let verdict = policy.evaluate(&public_url, &from, 0).await;
    assert!(verdict.allowed);
  }

  #[tokio::test]
  async fn should_prioritize_simple_policy_over_robots_txt() {
    let server = MockServer::start();

    // Mock robots.txt that would allow access
    let robots_content = r#"
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

    // Simple policy that blacklists the domain (just the IP, not including port)
    let server_host = server.address().ip().to_string();
    let blacklist = vec![server_host];
    let simple_policy = SimplePolicy::new(None, None, Some(blacklist));
    let registry = Arc::new(InMemoryRegistry::new());
    let robots_service = Arc::new(RobotsTxtService::new(registry, RobotsTxtConfig::default()));
    let policy = RobotsTxtPolicy::new(simple_policy, robots_service, "TestBot".to_string());

    let url = Url::parse(&format!("http://{}/public/page", server.address())).unwrap();
    let from = Url::parse("https://example.com/").unwrap();

    // Should be denied by simple policy blacklist, not robots.txt
    let verdict = policy.evaluate(&url, &from, 0).await;
    assert!(!verdict.allowed);
    assert_eq!(verdict.reason, "blacklist");
  }

  #[tokio::test]
  async fn should_handle_complex_robots_txt_with_user_agent_specificity() {
    let server = MockServer::start();

    // Mock robots.txt with complex user-agent rules
    let robots_content = r#"
User-agent: BadBot
Disallow: /

User-agent: TestBot
Allow: /api/
Disallow: /private/
Disallow: /admin/

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

    let simple_policy = SimplePolicy::new(None, None, None); // No restrictions
    let registry = Arc::new(InMemoryRegistry::new());
    let robots_service = Arc::new(RobotsTxtService::new(registry, RobotsTxtConfig::default()));
    let policy = RobotsTxtPolicy::new(simple_policy, robots_service, "TestBot".to_string());

    let from = Url::parse("https://example.com/").unwrap();

    // TestBot should be allowed to access /api/
    let api_url = Url::parse(&format!("http://{}/api/data", server.address())).unwrap();
    let verdict = policy.evaluate(&api_url, &from, 0).await;
    assert!(verdict.allowed);

    // TestBot should be denied access to /private/
    let private_url = Url::parse(&format!("http://{}/private/secret", server.address())).unwrap();
    let verdict = policy.evaluate(&private_url, &from, 0).await;
    assert!(!verdict.allowed);
    assert_eq!(verdict.reason, "robots.txt");

    // TestBot should be denied access to /admin/ (explicit rule)
    let admin_url = Url::parse(&format!("http://{}/admin/panel", server.address())).unwrap();
    let verdict = policy.evaluate(&admin_url, &from, 0).await;
    assert!(!verdict.allowed);
    assert_eq!(verdict.reason, "robots.txt");
  }
}
