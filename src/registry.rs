use crate::config::Config;
use crate::error::RegistryError;
use crate::package::Package;
use reqwest::Client;
use semver::{Version, VersionReq};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use url::Url;

pub struct RegistryClient {
    client: Arc<Client>,
    registry_url: Url,
    timeout: Duration,
    /// In-memory cache of all available version strings per package name.
    /// Avoids redundant full-metadata fetches during version-aware BFS resolution.
    version_cache: Arc<RwLock<HashMap<String, Vec<Version>>>>,
}

impl RegistryClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(32)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client: Arc::new(client),
            registry_url: Url::parse("https://registry.npmjs.org").expect("Invalid registry URL"),
            timeout: Duration::from_secs(30),
            version_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_config(config: &Config) -> Self {
        let client = Client::builder()
            .timeout(config.get_timeout())
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(32)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client: Arc::new(client),
            registry_url: config.registry_url.clone(),
            timeout: config.get_timeout(),
            version_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn fetch_package_info(
        &self,
        name: &str,
        version: Option<&str>,
    ) -> Result<Package, RegistryError> {
        let url = match version {
            Some(v) => self.registry_url.join(&format!("/{}/{}", name, v))?,
            None => self.registry_url.join(&format!("/{}/latest", name))?,
        };

        let response = self
            .client
            .get(url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(RegistryError::NetworkError)?;

        if !response.status().is_success() {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        let package_data = response
            .json()
            .await
            .map_err(|e| RegistryError::DeserializationError(e.to_string()))?;

        Ok(package_data)
    }

    /// Returns all stable (non-prerelease) versions available for a package.
    /// Results are cached in memory to avoid repeated full-metadata fetches.
    pub async fn fetch_package_versions(&self, name: &str) -> Result<Vec<Version>, RegistryError> {
        // Serve from cache when available.
        {
            let cache = self.version_cache.read().await;
            if let Some(versions) = cache.get(name) {
                return Ok(versions.clone());
            }
        }

        let url = self.registry_url.join(&format!("/{}", name))?;
        let response = self
            .client
            .get(url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(RegistryError::NetworkError)?;

        if !response.status().is_success() {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| RegistryError::DeserializationError(e.to_string()))?;

        let mut versions: Vec<Version> = data["versions"]
            .as_object()
            .map(|obj| {
                obj.keys()
                    .filter_map(|v| Version::parse(v).ok())
                    .filter(|v| v.pre.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        versions.sort();

        {
            let mut cache = self.version_cache.write().await;
            cache.insert(name.to_string(), versions.clone());
        }

        Ok(versions)
    }

    /// Fetches package metadata for the highest version that satisfies `version_req`.
    /// Falls back to `latest` when `version_req` is `None` or `*`.
    pub async fn fetch_best_matching(
        &self,
        name: &str,
        version_req: Option<&VersionReq>,
    ) -> Result<Package, RegistryError> {
        let Some(req) = version_req else {
            return self.fetch_package_info(name, None).await;
        };

        if *req == VersionReq::STAR {
            return self.fetch_package_info(name, None).await;
        }

        let versions = self.fetch_package_versions(name).await?;

        let best = versions
            .into_iter()
            .filter(|v| req.matches(v))
            .max()
            .ok_or_else(|| {
                RegistryError::PackageNotFound(format!("{} matching {}", name, req))
            })?;

        self.fetch_package_info(name, Some(&best.to_string())).await
    }
}
