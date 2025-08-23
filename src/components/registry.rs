use url::Url;

use crate::types::RegistryItemStatus;

pub trait Registry: Send + Sync {
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
