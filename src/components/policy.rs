use url::Url;

use crate::types::PolicyVerdict;

/// Policy decides if newly discovered link should be admitted.
pub trait Policy {
  fn evaluate(&self, candidate: &Url, from: &Url, depth: u32) -> PolicyVerdict;
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
    let v = p.evaluate(&url("https://a.test/"), &from_url(), 3);
    match v {
      PolicyVerdict::ALLOW => panic!("expected deny when depth > max_depth"),
      _ => {} // denied as expected
    }
  }

  #[test]
  fn allows_when_depth_equals_max() {
    let p = SimplePolicy::new(Some(2), None, None);
    let v = p.evaluate(&url("https://a.test/"), &from_url(), 2);
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
    let v_ok = p.evaluate(&url("https://Sub.EXAMPLE.com/path"), &from_url(), 0);
    match v_ok {
      PolicyVerdict::ALLOW => {} // ok
      _ => panic!("expected allow for whitelisted host (suffix, case-insensitive)"),
    }

    // Not in whitelist
    let v_bad = p.evaluate(&url("https://other.test/"), &from_url(), 0);
    match v_bad {
      PolicyVerdict::ALLOW => panic!("expected deny for non-whitelisted host"),
      _ => {} // denied as expected
    }
  }

  #[test]
  fn enforces_blacklist_with_suffix_match() {
    // Blacklist example.com and its subdomains.
    let p = SimplePolicy::new(None, None, Some(vec!["example.com".into()]));

    let v = p.evaluate(&url("https://api.example.com/"), &from_url(), 0);
    match v {
      PolicyVerdict::ALLOW => panic!("expected deny for blacklisted host (suffix)"),
      _ => {} // denied as expected
    }

    // A different domain should be allowed (since no whitelist)
    let v_ok = p.evaluate(&url("https://good.test/"), &from_url(), 0);
    match v_ok {
      PolicyVerdict::ALLOW => {} // ok
      _ => panic!("expected allow for non-blacklisted host"),
    }
  }

  #[test]
  fn denies_urls_without_host() {
    let p = SimplePolicy::new(None, None, None);

    // mailto/data URLs have no host
    let v1 = p.evaluate(&url("mailto:someone@example.com"), &from_url(), 0);
    let v2 = p.evaluate(&url("data:text/plain,hello"), &from_url(), 0);

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
    let v = p.evaluate(&url("https://ok.example/"), &from_url(), 0);
    match v {
      PolicyVerdict::ALLOW => {} // ok
      _ => panic!("expected allow when no limits and normal host"),
    }
  }
}
