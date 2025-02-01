use crate::{
    error::{RpmError, RpmResult},
    package::Package,
    registry::RegistryClient,
    verification::{ChecksumIntegrityChecker, Verification},
    cache::PackageCache,
    profiling::MemoryProfile,
};
use anyhow::Result;
use flate2::read::GzDecoder;
use futures::future::try_join_all;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use std::path::PathBuf;
use std::sync::Arc;
use tar::Archive;
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::Semaphore;
use url;

pub struct PackageInstaller {
    global: bool,
    registry: Arc<RegistryClient>,
    install_path: PathBuf,
    http_client: Client,
    concurrent_limit: Arc<Semaphore>,
    cache: PackageCache,
    memory_profile: MemoryProfile,
}

impl PackageInstaller {
    pub fn new(global: bool, cache: PackageCache, memory_profile: MemoryProfile) -> Self {
        let install_path = if global {
            PathBuf::from("/usr/local/lib/node_modules")
        } else {
            PathBuf::from("node_modules")
        };

        Self {
            global,
            registry: Arc::new(RegistryClient::new()),
            install_path,
            http_client: Client::new(),
            concurrent_limit: Arc::new(Semaphore::new(8)),
            cache,
            memory_profile,
        }
    }

    pub async fn install_packages(&self, packages: &[String]) -> Result<()> {
        fs::create_dir_all(&self.install_path).await?;

        let m = MultiProgress::new();
        let total_progress = m.add(ProgressBar::new(packages.len() as u64));
        total_progress.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})")
            .progress_chars("#>-"));

        let tasks: Vec<_> = packages.iter().map(|package| {
            let installer = self.clone();
            let package = package.clone();
            let pb = m.add(ProgressBar::new(4)); // Download, Verify, Extract, Scripts
            pb.set_style(ProgressStyle::default_bar()
                .template("{spinner:.green} {msg} [{wide_bar:.cyan/blue}] {pos}/{len}")
                .progress_chars("#>-"));
            pb.set_message(format!("Installing {}", package));

            tokio::spawn(async move {
                let _permit = installer.concurrent_limit.acquire().await?;
                let result = installer.install_package(&package, pb.clone()).await;
                pb.finish_and_clear();
                result
            })
        }).collect();

        let handle = tokio::spawn(async move {
            m.join().unwrap();
        });

        let results = try_join_all(tasks).await?;
        for result in results {
            result?;
            total_progress.inc(1);
        }

        handle.await?;
        total_progress.finish_with_message("All packages installed successfully!");
        Ok(())
    }

    async fn install_package(&self, package_name: &str, progress: ProgressBar) -> Result<()> {
        // Download phase
        progress.set_message(format!("Downloading {}", package_name));
        let package_info = self.registry.fetch_package_info(package_name, None).await?;
        let package_data = self.download_package(&package_info).await?;
        progress.inc(1);

        // Verify phase
        progress.set_message(format!("Verifying {}", package_name));
        ChecksumIntegrityChecker::verify_package(&package_data, &package_info.dist.shasum)?;
        progress.inc(1);

        // Extract phase
        progress.set_message(format!("Extracting {}", package_name));
        let package_path = self.extract_package(&package_data, package_name).await?;
        progress.inc(1);

        // Run scripts in sandbox
        progress.set_message(format!("Running scripts for {}", package_name));
        let sandbox = Sandbox::new(&package_path);
        if let Err(e) = sandbox.run_script("npm run prepare").await {
            log::warn!("Failed to run prepare script: {}", e);
        }
        progress.inc(1);

        Ok(())
    }

    async fn download_package(&self, package: &Package) -> RpmResult<Vec<u8>> {
        // Check cache first
        if let Some(cached_path) = self.cache.get(&package.name, &package.version).await? {
            log::debug!("Using cached version of {} {}", package.name, package.version);
            return Ok(fs::read(&cached_path).await?);
        }

        let response = self.http_client
            .get(&package.dist.tarball)
            .send()
            .await
            .map_err(|e| RpmError::DownloadError {
                package: package.name.clone(),
                url: url::Url::parse(&package.dist.tarball)?,
                source: e,
            })?;

        if !response.status().is_success() {
            return Err(RpmError::DownloadError {
                package: package.name.clone(),
                url: url::Url::parse(&package.dist.tarball)?,
                source: reqwest::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("HTTP {}", response.status())
                )),
            });
        }

        // Stream the download and track memory usage
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| RpmError::DownloadError {
                package: package.name.clone(),
                url: url::Url::parse(&package.dist.tarball)?,
                source: e,
            })?;
            
            self.memory_profile.allocate(chunk.len());
            bytes.extend_from_slice(&chunk);
        }

        // Cache the downloaded package
        self.cache.put(&package.name, &package.version, &bytes).await?;

        Ok(bytes)
    }

    async fn extract_package(&self, package_data: &[u8], package_name: &str) -> RpmResult<PathBuf> {
        let package_path = self.install_path.join(package_name);
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().join(format!("{}.tgz", package_name));

        // Track memory for temporary files
        self.memory_profile.allocate(package_data.len());
        
        // Write to temporary file using buffered writer
        let file = tokio::fs::File::create(&temp_path).await?;
        let mut writer = BufWriter::new(file);
        writer.write_all(package_data).await?;
        writer.flush().await?;

        // Extract in a blocking task
        let package_path_clone = package_path.clone();
        tokio::task::spawn_blocking(move || -> RpmResult<()> {
            let tar_gz = std::fs::File::open(temp_path)?;
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);

            archive.unpack(&package_path_clone).map_err(|e| RpmError::ExtractionError {
                package: package_name.to_string(),
                path: package_path_clone.clone(),
                source: e,
            })?;

            Ok(())
        }).await??;

        // Cleanup temporary memory allocation
        self.memory_profile.deallocate(package_data.len());

        Ok(package_path)
    }
}

impl Clone for PackageInstaller {
    fn clone(&self) -> Self {
        Self {
            global: self.global,
            registry: Arc::clone(&self.registry),
            install_path: self.install_path.clone(),
            http_client: self.http_client.clone(),
            concurrent_limit: Arc::clone(&self.concurrent_limit),
            cache: self.cache.clone(),
            memory_profile: self.memory_profile.clone(),
        }
    }
}
