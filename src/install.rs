use crate::error::InstallError;
use crate::package::Package;
use crate::registry::RegistryClient;
use crate::verification::{ChecksumIntegrityChecker, Verification};
use anyhow::Result;
use flate2::read::GzDecoder;
use reqwest::Client;
use std::path::PathBuf;
use tar::Archive;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub struct PackageInstaller {
    global: bool,
    registry: RegistryClient,
    install_path: PathBuf,
    http_client: Client,
}

impl PackageInstaller {
    pub fn new(global: bool) -> Self {
        let install_path = if global {
            PathBuf::from("/usr/local/lib/node_modules")
        } else {
            PathBuf::from("node_modules")
        };

        Self {
            global,
            registry: RegistryClient::new(),
            install_path,
            http_client: Client::new(),
        }
    }

    pub async fn install_packages(&self, packages: &[String]) -> Result<()> {
        fs::create_dir_all(&self.install_path).await?;

        for package in packages {
            self.install_package(package).await?;
        }

        Ok(())
    }

    async fn install_package(&self, package_name: &str) -> Result<()> {
        let package_info = self.registry.fetch_package_info(package_name, None).await?;

        let package_data = self.download_package(&package_info).await?;

        ChecksumIntegrityChecker::verify_package(&package_data, &package_info.dist.shasum)?;

        self.extract_package(&package_data, package_name).await?;

        Ok(())
    }

    async fn download_package(&self, package: &Package) -> Result<Vec<u8>> {
        let response = self
            .http_client
            .get(&package.dist.tarball)
            .send()
            .await
            .map_err(|e| InstallError::DownloadError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(InstallError::DownloadError(format!(
                "Failed to download package: HTTP {}",
                response.status()
            ))
            .into());
        }

        let package_data = response
            .bytes()
            .await
            .map_err(|e| InstallError::DownloadError(e.to_string()))?;

        Ok(package_data.to_vec())
    }

    async fn extract_package(&self, package_data: &[u8], package_name: &str) -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().join(format!("{}.tgz", package_name));

        let mut temp_file = tokio::fs::File::create(&temp_path).await?;
        temp_file.write_all(package_data).await?;
        temp_file.flush().await?;

        let install_path = self.install_path.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let tar_gz = std::fs::File::open(temp_path)?;
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);

            archive
                .unpack(&install_path)
                .map_err(|e| InstallError::ExtractionError(e.to_string()))?;

            Ok(())
        })
        .await??;

        Ok(())
    }
}
