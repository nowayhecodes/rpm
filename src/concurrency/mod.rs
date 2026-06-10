use crate::error::DownloadError;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// HTTP downloader that caps the number of in-flight requests via an `Arc<Semaphore>`.
/// The `Arc` wrapper makes `ConcurrentDownloader` cheaply cloneable so it can be
/// shared across all `PackageInstaller` clones spawned during a BFS install pass.
#[derive(Clone)]
pub struct ConcurrentDownloader {
    semaphore: Arc<Semaphore>,
    client: Client,
}

impl ConcurrentDownloader {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            client: Client::builder()
                .pool_idle_timeout(std::time::Duration::from_secs(90))
                .pool_max_idle_per_host(32)
                .build()
                .expect("Failed to build downloader HTTP client"),
        }
    }

    /// Downloads the resource at `url` and returns the raw bytes.
    /// Blocks until a semaphore permit is available, limiting concurrent downloads.
    pub async fn download(&self, url: &str) -> Result<Vec<u8>, DownloadError> {
        let _permit = self.semaphore.acquire().await?;
        let response = self.client.get(url).send().await?;
        Ok(response.bytes().await?.to_vec())
    }
}
