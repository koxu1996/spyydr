use std::sync::Arc;

use crate::components::canonicalizer::Canonicalizer;
use crate::components::extractor::Extractor;
use crate::components::fetcher::Fetcher;
use crate::components::policy::Policy;
use crate::components::registry::Registry;

pub struct Agent<F: Fetcher, E: Extractor, C: Canonicalizer, P: Policy, R: Registry> {
  /// Stateless components.
  fetcher: F,
  extractor: E,
  canonicalizer: C,
  policy: P,
  /// Single shared registry for all agents.
  registry: Arc<R>,
}

impl<F: Fetcher, E: Extractor, C: Canonicalizer, P: Policy, R: Registry> Agent<F, E, C, P, R> {
  pub async fn new(
    fetcher: F,
    extractor: E,
    canonicalizer: C,
    policy: P,
    registry: Arc<R>,
  ) -> Self {
    Agent {
      fetcher,
      extractor,
      canonicalizer,
      policy,
      registry,
    }
  }
}
