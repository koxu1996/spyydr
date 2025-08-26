use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
  #[error("HTTP fetch error: {0}")]
  HttpFetch(#[from] reqwest::Error),

  #[error("URL parse error: {0}")]
  UrlParse(#[from] url::ParseError),

  #[error("Failed to send command to broker: {0}")]
  BrokerSendError(
    #[from] tokio::sync::mpsc::error::SendError<crate::orchestration::broker::BrokerCommand>,
  ),

  #[error("Failed to receive response from broker: {0}")]
  BrokerRecvError(#[from] tokio::sync::oneshot::error::RecvError),

  /// Programmer error.
  #[error("Unexpected error: {0}")]
  Unexpected(String),
}
