use crate::components::fetcher::Fetcher;
use crate::error::Error;

pub struct Engine<F: Fetcher> {
  fetcher: F,
}

impl<F: Fetcher> Engine<F> {
  pub fn new(fetcher: F) -> Self {
    Engine { fetcher }
  }

  pub async fn run(&self, url: &str) -> Result<(), Error> {
    let url = url::Url::parse(url).map_err(|e| Error::Unexpected(e.to_string()))?;
    let fetched_content = self.fetcher.fetch(&url).await?;

    println!("Fetched content of {:?}", fetched_content.url);

    Ok(())
  }
}
