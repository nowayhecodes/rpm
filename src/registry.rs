use crate::error::RegistryError;
use crate::package::Package;
use reqwest::Client;
use std::time::Duration;
use url::Url;
use std::sync::Arc;

pub struct RegistryClient {
    client: Arc<Client>,
    registry_url: Url,
    timeout: Duration,
}

impl RegistryClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(32)
            .build()
            .expect("Failed to create HTTP client");

        let registry_url = Url::parse("https://registry.npmjs.org")
            .expect("Invalid registry URL");

        Self {
            client: Arc::new(client),
            registry_url,
            timeout: Duration::from_secs(30),
        }
    }

    pub async fn fetch_package_info(
        &self,
        name: &str,
        version: Option<&str>,
    ) -> Result<Package, RegistryError> {
        let url = match version {
            Some(v) => self.registry_url.join(&format!("/{}/{}/-/{}-{}.tgz", name, v, name, v))?,
            None => self.registry_url.join(&format!("/{}/latest", name))?,
        };

        let response = self.client
            .get(url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| RegistryError::NetworkError(e))?;

        if !response.status().is_success() {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        let package_data = response.json().await
            .map_err(|e| RegistryError::DeserializationError(e.to_string()))?;
        
        Ok(package_data)
    }
}
