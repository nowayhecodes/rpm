use crate::error::RegistryError;
use crate::package::Package;
use reqwest::Client;
use std::time::Duration;
use url::Url;

pub struct RegistryClient {
    client: Client,
    registry_url: Url,
}

impl RegistryClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        let registry_url = Url::parse("https://registry.npmjs.org").expect("Invalid registry URL");

        Self {
            client,
            registry_url,
        }
    }

    pub async fn fetch_package_info(
        &self,
        name: &str,
        version: Option<&str>,
    ) -> Result<Package, RegistryError> {
        let url = self.registry_url.join(&format!(
            "/{}/-/{}-{}",
            name,
            name,
            version.unwrap_or("latest")
        ))?;
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        let package_data: Package = response.json().await?;
        Ok(package_data)
    }
}
