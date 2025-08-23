use url::Url;

use crate::types::PolicyVerdict;

/// Policy decides if newly discovered link should be admitted.
pub trait Policy: Send + Sync {
  fn evaluate(&self, candidate: &Url, from: &Url, depth: u32) -> PolicyVerdict;
}
