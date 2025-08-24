use url::Url;

use crate::types::PolicyVerdict;

/// Policy decides if newly discovered link should be admitted.
pub trait Policy {
  fn evaluate(&self, candidate: &Url, from: &Url, depth: u32) -> PolicyVerdict;
}

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
      .any(|pat| host == pat || host.ends_with(&format!(".{pat}")))
  }
}

impl Policy for SimplePolicy {
  fn evaluate(&self, candidate: &Url, _from: &Url, depth: u32) -> PolicyVerdict {
    // Check crawling depth.
    if let Some(md) = self.max_depth {
      if depth > md {
        return PolicyVerdict::deny("depth");
      }
    }

    // Check against the whitelist and blacklist.
    let Some(h) = Self::host(candidate) else {
      return PolicyVerdict::deny("no-host");
    };
    if !self.whitelist.is_empty() && !Self::matches_suffix(&self.whitelist, &h) {
      return PolicyVerdict::deny("whitelist");
    }
    if !self.blacklist.is_empty() && Self::matches_suffix(&self.blacklist, &h) {
      return PolicyVerdict::deny("blacklist");
    }

    PolicyVerdict::ALLOW
  }
}
