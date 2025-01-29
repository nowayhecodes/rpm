use crate::error::DownloadError;
use tokio::sync::Semaphore;
use anyhow::Result;
use reqwest::Client;

pub struct ConcurrentDownloader {
    semaphore: Semaphore,
    client: Client,
}

impl ConcurrentDownloader {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Semaphore::new(max_concurrent),
            client: reqwest::Client::new(),
        }
    }

    pub async fn download(&self, url: &str) -> Result<Vec<u8>, DownloadError> {
        let _permit = self.semaphore.acquire().await?;
        let response = self.client.get(url).send().await?;
        Ok(response.bytes().await?.to_vec())
    }
} 