use crate::components::canonicalizer::Canonicalizer;
use crate::components::extractor::Extractor;
use crate::components::fetcher::Fetcher;
use crate::components::policy::Policy;

pub struct Agent<F: Fetcher, E: Extractor, C: Canonicalizer, P: Policy> {
  /// Stateless components.
  fetcher: F,
  extractor: E,
  canonicalizer: C,
  policy: P,
}

impl<F: Fetcher, E: Extractor, C: Canonicalizer, P: Policy> Agent<F, E, C, P> {
  pub async fn new(fetcher: F, extractor: E, canonicalizer: C, policy: P) -> Self {
    Agent {
      fetcher,
      extractor,
      canonicalizer,
      policy,
    }
  }
}
