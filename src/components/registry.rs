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
          true // Changed Discovered -> Fetched.
        } else {
          false // Already Fetched.
        }
      }
      Entry::Vacant(v) => {
        v.insert(RegistryItemStatus::Fetched);
        true // Newly inserted as Fetched
      }
    }
  }

  fn status(&self, url: &Url) -> Option<RegistryItemStatus> {
    self.map.get(url.as_str()).map(|v| *v.value())
  }
}
