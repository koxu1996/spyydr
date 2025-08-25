use dashmap::DashMap;
use url::Url;

use crate::types::RegistryItemStatus;

pub trait Registry {
  /// Insert as Discovered. Returns true if it was not present before.
  fn register(&self, url: &Url) -> bool;

  /// Mark as Fetched. Returns true if it changed from Discovered -> Fetched.
  /// If it wasn't present, you may choose to insert as Fetched; here we do.
  fn mark_fetched(&self, url: &Url) -> bool;

  /// Current status if tracked.
  fn status(&self, url: &Url) -> Option<RegistryItemStatus>;

  /// Register many URLs as discovered and return those that were newly added.
  fn register_many<'a, I>(&self, urls: I) -> Vec<Url>
  where
    I: IntoIterator<Item = &'a Url>,
  {
    let mut out = Vec::new();
    let iter = urls.into_iter();
    if let (_, Some(upper)) = iter.size_hint() {
      out.reserve(upper); // Small optimization.
    }
    for u in iter {
      if self.register(u) {
        out.push(u.clone());
      }
    }
    out
  }
}

/// Registry for a single-process crawl state.
#[derive(Default)]
pub struct InMemoryRegistry {
  map: DashMap<String, RegistryItemStatus>,
}

impl InMemoryRegistry {
  pub fn new() -> Self {
    Self {
      map: DashMap::new(),
    }
  }

  /// Optional helpers for metrics
  pub fn counts(&self) -> (usize, usize) {
    let (mut disc, mut fetch) = (0, 0);
    for v in self.map.iter() {
      match *v.value() {
        RegistryItemStatus::Discovered => disc += 1,
        RegistryItemStatus::Fetched => fetch += 1,
      }
    }
    (disc, fetch)
  }
}

impl Registry for InMemoryRegistry {
  fn register(&self, url: &Url) -> bool {
    // Insert returns old value if existed; we want to return true only if it was NEW.
    match self.map.entry(url.as_str().to_owned()) {
      dashmap::mapref::entry::Entry::Occupied(_) => false,
      dashmap::mapref::entry::Entry::Vacant(v) => {
        tracing::debug!(%url, "registering as discovered");
        v.insert(RegistryItemStatus::Discovered);
        true
      }
    }
  }

  fn mark_fetched(&self, url: &Url) -> bool {
    use dashmap::mapref::entry::Entry;
    match self.map.entry(url.as_str().to_owned()) {
      Entry::Occupied(mut e) => {
        if *e.get() != RegistryItemStatus::Fetched {
          e.insert(RegistryItemStatus::Fetched);
          tracing::debug!(%url, "marking as fetched");
          true // Changed Discovered -> Fetched.
        } else {
          false // Already Fetched.
        }
      }
      // This case should not normally happen, but we handle it anyway.
      Entry::Vacant(v) => {
        tracing::debug!(%url, "registering as fetched (was not present)");
        v.insert(RegistryItemStatus::Fetched);
        true // Newly inserted as Fetched
      }
    }
  }

  fn status(&self, url: &Url) -> Option<RegistryItemStatus> {
    self.map.get(url.as_str()).map(|v| *v.value())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use url::Url;

  fn url(s: &str) -> Url {
    Url::parse(s).expect("valid url")
  }

  fn reg() -> InMemoryRegistry {
    InMemoryRegistry::new()
  }

  #[test]
  fn register_new_sets_discovered_and_returns_true() {
    let r = reg();
    let u = url("https://a.example/");
    assert!(r.register(&u), "first insert should return true");
    // status should be Discovered
    match r.status(&u) {
      Some(RegistryItemStatus::Discovered) => {}
      other => panic!("expected Some(Discovered), got {:?}", other),
    }
    // counts should reflect one discovered, zero fetched
    assert_eq!(r.counts(), (1, 0));
  }

  #[test]
  fn register_duplicate_returns_false_and_keeps_discovered() {
    let r = reg();
    let u = url("https://dup.example/");
    assert!(r.register(&u));
    assert!(!r.register(&u), "duplicate should return false");
    match r.status(&u) {
      Some(RegistryItemStatus::Discovered) => {}
      other => panic!("expected Some(Discovered), got {:?}", other),
    }
    assert_eq!(r.counts(), (1, 0));
  }

  #[test]
  fn mark_fetched_transitions_discovered_to_fetched_and_is_idempotent() {
    let r = reg();
    let u = url("https://t.example/");
    assert!(r.register(&u));
    // transition Discovered -> Fetched should return true
    assert!(r.mark_fetched(&u));
    match r.status(&u) {
      Some(RegistryItemStatus::Fetched) => {}
      other => panic!("expected Some(Fetched), got {:?}", other),
    }
    // second call should be a no-op and return false
    assert!(!r.mark_fetched(&u));
    assert_eq!(r.counts(), (0, 1));
  }

  #[test]
  fn mark_fetched_on_absent_inserts_as_fetched_and_returns_true() {
    let r = reg();
    let u = url("https://new.example/");
    // Not registered before; should insert as Fetched and return true
    assert!(r.mark_fetched(&u));
    match r.status(&u) {
      Some(RegistryItemStatus::Fetched) => {}
      other => panic!("expected Some(Fetched), got {:?}", other),
    }
    assert_eq!(r.counts(), (0, 1));
  }

  #[test]
  fn status_none_for_unknown_url() {
    let r = reg();
    let u = url("https://unknown.example/");
    assert!(r.status(&u).is_none());
  }

  #[test]
  fn register_many_returns_only_new_urls() {
    let r = reg();
    let a = url("https://a.example/");
    let b = url("https://b.example/");
    let c = url("https://c.example/");

    // Pre-register one
    assert!(r.register(&a));

    // Now call register_many with a (dup), b (new), c (new)
    let added = r.register_many([&a, &b, &c].iter().copied());
    // Should only include b and c, order preserved by iteration
    assert_eq!(added, vec![b.clone(), c.clone()]);

    // Status checks
    for u in [&a, &b, &c] {
      match r.status(u) {
        Some(RegistryItemStatus::Discovered) => {}
        other => panic!("expected Some(Discovered) for {u}, got {:?}", other),
      }
    }
    // counts: three discovered, zero fetched
    assert_eq!(r.counts(), (3, 0));
  }
}
