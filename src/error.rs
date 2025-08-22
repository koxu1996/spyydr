use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
  #[error("Unexpected error: {0}")]
  Unexpected(String),
}
