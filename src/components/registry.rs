use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use url::Url;

use crate::types::{RegistryError, RegistryItemStatus};

pub trait Registry: Send + Sync {
  // Generic state operations
  fn get<T>(&self, key: &str) -> Result<Option<T>, RegistryError>
  where
    T: for<'de> Deserialize<'de>;

  fn set<T>(&self, key: &str, value: &T) -> Result<(), RegistryError>
  where
    T: Serialize;

  fn set_with_ttl<T>(&self, key: &str, value: &T, ttl: Duration) -> Result<(), RegistryError>
  where
    T: Serialize;

  fn remove(&self, key: &str) -> Result<bool, RegistryError>;

  // URL-specific convenience methods (implemented on top of generic storage)
  /// Insert as Discovered. Returns true if it was not present before.
  fn register(&self, url: &Url) -> bool {
    let key = format!("url:{}", url.as_str());
    let current: Option<RegistryItemStatus> = self.get(&key).unwrap_or(None);
    if current.is_none() {
      let _ = self.set(&key, &RegistryItemStatus::Discovered);
      true
    } else {
      false
    }
  }

  /// Mark as Fetched. Returns true if it changed from Discovered -> Fetched.
  /// If it wasn't present, you may choose to insert as Fetched; here we do.
  fn mark_fetched(&self, url: &Url) -> bool {
    let key = format!("url:{}", url.as_str());
    let current: Option<RegistryItemStatus> = self.get(&key).unwrap_or(None);
    let was_different = current != Some(RegistryItemStatus::Fetched);
    let _ = self.set(&key, &RegistryItemStatus::Fetched);
    was_different
  }

  /// Current status if tracked.
  fn status(&self, url: &Url) -> Option<RegistryItemStatus> {
    let key = format!("url:{}", url.as_str());
    self.get(&key).unwrap_or(None)
  }

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
  // Unified storage for all state types: (data, expiry)
  storage: DashMap<String, (Vec<u8>, Option<Instant>)>,
}

impl InMemoryRegistry {
  pub fn new() -> Self {
    Self {
      storage: DashMap::new(),
    }
  }

  /// Optional helpers for metrics (updated for unified storage)
  pub fn counts(&self) -> (usize, usize) {
    let (mut disc, mut fetch) = (0, 0);
    for entry in self.storage.iter() {
      if entry.key().starts_with("url:") {
        if let Ok(Some(status)) = self.get::<RegistryItemStatus>(entry.key()) {
          match status {
            RegistryItemStatus::Discovered => disc += 1,
            RegistryItemStatus::Fetched => fetch += 1,
          }
        }
      }
    }
    (disc, fetch)
  }
}

impl Registry for InMemoryRegistry {
  fn get<T>(&self, key: &str) -> Result<Option<T>, RegistryError>
  where
    T: for<'de> Deserialize<'de>,
  {
    if let Some(entry) = self.storage.get(key) {
      let (data, expiry) = entry.value();

      // Check TTL
      if let Some(exp) = expiry {
        if Instant::now() > *exp {
          drop(entry); // Release read lock
          self.storage.remove(key);
          return Ok(None);
        }
      }

      let value: T = serde_json::from_slice(data)?;
      Ok(Some(value))
    } else {
      Ok(None)
    }
  }

  fn set<T>(&self, key: &str, value: &T) -> Result<(), RegistryError>
  where
    T: Serialize,
  {
    let data = serde_json::to_vec(value)?;
    self.storage.insert(key.to_string(), (data, None));
    Ok(())
  }

  fn set_with_ttl<T>(&self, key: &str, value: &T, ttl: Duration) -> Result<(), RegistryError>
  where
    T: Serialize,
  {
    let data = serde_json::to_vec(value)?;
    let expiry = Some(Instant::now() + ttl);
    self.storage.insert(key.to_string(), (data, expiry));
    Ok(())
  }

  fn remove(&self, key: &str) -> Result<bool, RegistryError> {
    Ok(self.storage.remove(key).is_some())
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

  // TDD: Test for new generic state functionality
  #[test]
  fn should_store_and_retrieve_generic_state() {
    // Arrange
    let registry = InMemoryRegistry::new();

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestState {
      value: String,
      count: u32,
    }

    let test_state = TestState {
      value: "test".to_string(),
      count: 42,
    };

    // Act
    let result = registry.set("test_key", &test_state);
    assert!(result.is_ok());

    let retrieved: Option<TestState> = registry.get("test_key").unwrap();

    // Assert
    assert_eq!(retrieved, Some(test_state));
  }

  #[test]
  fn should_return_none_for_nonexistent_key() {
    // Arrange
    let registry = InMemoryRegistry::new();

    // Act
    let result: Option<String> = registry.get("nonexistent").unwrap();

    // Assert
    assert_eq!(result, None);
  }

  #[test]
  fn should_handle_ttl_expiration() {
    // Arrange
    let registry = InMemoryRegistry::new();
    let test_value = "expiring_value".to_string();

    // Act - set with very short TTL
    registry.set_with_ttl("ttl_key", &test_value, Duration::from_millis(1)).unwrap();

    // Wait for expiration
    std::thread::sleep(Duration::from_millis(10));

    let result: Option<String> = registry.get("ttl_key").unwrap();

    // Assert
    assert_eq!(result, None);
  }
}
