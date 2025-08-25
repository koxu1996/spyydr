use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
  #[error("URL parse error: {0}")]
  UrlParse(#[from] url::ParseError),

  /// Programmer error.
  #[error("Unexpected error: {0}")]
  Unexpected(String),
}
