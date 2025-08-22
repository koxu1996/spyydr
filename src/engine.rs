use crate::components::extractor::Extractor;
use crate::components::fetcher::Fetcher;
use crate::error::Error;

pub struct Engine<F: Fetcher, E: Extractor> {
  fetcher: F,
  extractor: E,
}

impl<F: Fetcher, E: Extractor> Engine<F, E> {
  pub fn new(fetcher: F, extractor: E) -> Self {
    Engine { fetcher, extractor }
  }

  pub async fn run_once(&self, url: &str) -> Result<(), Error> {
    let url = url::Url::parse(url).map_err(|e| Error::Unexpected(e.to_string()))?;
    let fetched_content = self.fetcher.fetch(&url).await?;

    println!("Fetched content of {:?}", fetched_content.url);

    let links = self.extractor.find_links(&fetched_content)?;
    println!("Extracted {} links", links.len());

    Ok(())
  }
}
