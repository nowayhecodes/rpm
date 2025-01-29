use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use url::Url;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub registry_url: Url,
    pub cache_dir: PathBuf,
    pub global_packages_dir: PathBuf,
    pub timeout: u64,
    pub max_concurrent_downloads: usize,
    pub offline_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            registry_url: Url::parse("https://registry.npmjs.org").unwrap(),
            cache_dir: PathBuf::from(".rpm/cache"),
            global_packages_dir: PathBuf::from("/usr/local/lib/node_modules"),
            timeout: 30,
            max_concurrent_downloads: 8,
            offline_mode: false,
        }
    }
}

impl Config {
    pub async fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = fs::read_to_string(path).await?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub async fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).await?;
        Ok(())
    }

    pub fn get_timeout(&self) -> Duration {
        Duration::from_secs(self.timeout)
    }
} 