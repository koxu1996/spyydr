use url::Url;

#[derive(Clone, Debug)]
pub struct FetchedContent {
  pub url: Url,
  pub status: u16,
  pub body: String,
  // pub elapsed: Duration,
}
