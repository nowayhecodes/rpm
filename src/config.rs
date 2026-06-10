use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(with = "url_string")]
    pub registry_url: Url,
    pub cache_dir: PathBuf,
    pub global_packages_dir: PathBuf,
    pub timeout: u64,
    pub max_concurrent_downloads: usize,
    pub offline_mode: bool,
}

mod url_string {
    use serde::{Deserialize, Deserializer, Serializer};
    use url::Url;

    pub fn serialize<S>(url: &Url, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(url.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Url, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Url::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl Default for Config {
    fn default() -> Self {
        #[cfg(windows)]
        let global_packages_dir = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("npm").join("node_modules"))
            .unwrap_or_else(|| PathBuf::from(r"C:\Users\Public\npm\node_modules"));

        #[cfg(not(windows))]
        let global_packages_dir = PathBuf::from("/usr/local/lib/node_modules");

        Self {
            registry_url: Url::parse("https://registry.npmjs.org").unwrap(),
            cache_dir: PathBuf::from(".rpm/cache"),
            global_packages_dir,
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
