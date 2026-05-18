use crate::{
    cache::PackageCache,
    error::{RpmError, RpmResult},
    package::Package,
    profiling::MemoryProfile,
    registry::RegistryClient,
    sandbox::Sandbox,
    verification::{ChecksumIntegrityChecker, Verification},
};
use flate2::read::GzDecoder;
use futures::future::try_join_all;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde_json;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tar::Archive;
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::{Mutex, Semaphore};
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
        Self::new_in_project(global, cache, memory_profile, ".")
    }

    pub fn new_in_project(
        global: bool,
        cache: PackageCache,
        memory_profile: MemoryProfile,
        project_dir: impl Into<PathBuf>,
    ) -> Self {
        let install_path = if global {
            PathBuf::from("/usr/local/lib/node_modules")
        } else {
            project_dir.into().join("node_modules")
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

    pub async fn install_packages(&self, packages: &[String]) -> RpmResult<()> {
        fs::create_dir_all(&self.install_path).await?;

        let bar = Arc::new(ProgressBar::new(packages.len() as u64));
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}",
            )
            .expect("valid progress template")
            .progress_chars("#>-"),
        );
        bar.enable_steady_tick(Duration::from_millis(80));

        // Every package name that has been queued. The mutex lets concurrent
        // tasks claim new packages atomically without duplicating work.
        let queued: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(
            packages.iter().cloned().collect(),
        ));

        // BFS over the dependency tree. Each round installs one level in
        // parallel; tasks return their direct dep names, which become the
        // next round. This avoids recursive async futures (which hit Send
        // bounds that the compiler can't satisfy across opaque impl Future
        // return types).
        let mut current_batch: Vec<String> = packages.to_vec();
        while !current_batch.is_empty() {
            let tasks: Vec<_> = current_batch
                .into_iter()
                .map(|package| {
                    let installer = self.clone();
                    let bar = Arc::clone(&bar);
                    tokio::spawn(async move { installer.install_one(package, bar).await })
                })
                .collect();

            let mut next_raw: Vec<String> = Vec::new();
            for result in try_join_all(tasks).await? {
                next_raw.extend(result?);
            }

            // Keep only deps not already claimed by any other task.
            let next_batch: Vec<String> = {
                let mut lock = queued.lock().await;
                next_raw
                    .into_iter()
                    .filter(|name| lock.insert(name.clone()))
                    .collect()
            };

            if !next_batch.is_empty() {
                bar.inc_length(next_batch.len() as u64);
            }
            current_batch = next_batch;
        }

        bar.finish_and_clear();
        Ok(())
    }

    /// Downloads, verifies, and extracts one package.
    /// Returns the names of its direct dependencies for the caller to queue.
    async fn install_one(
        &self,
        package_name: String,
        bar: Arc<ProgressBar>,
    ) -> RpmResult<Vec<String>> {
        let permit = self.concurrent_limit.acquire().await?;

        bar.set_message(format!("downloading {}", package_name));
        let package_info = self.registry.fetch_package_info(&package_name, None).await?;
        let package_data = self.download_package(&package_info).await?;

        bar.set_message(format!("verifying {}", package_name));
        ChecksumIntegrityChecker::verify_package(&package_data, &package_info.dist.shasum)?;

        bar.set_message(format!("extracting {}", package_name));
        let package_path = self.extract_package(&package_data, &package_name).await?;

        let dep_names: Vec<String> = package_info.dependencies.into_keys().collect();

        drop(permit);
        bar.inc(1);

        // Run lifecycle scripts only if the package defines a "prepare" script.
        // npm tarballs unpack into a "package/" subdirectory; check there first.
        let pkg_subdir = package_path.join("package");
        let script_dir = if pkg_subdir.exists() { pkg_subdir } else { package_path };
        let pkg_json_path = script_dir.join("package.json");
        let has_prepare = if let Ok(content) = fs::read_to_string(&pkg_json_path).await {
            serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|v| v["scripts"]["prepare"].as_str().map(|s| !s.is_empty()))
                .unwrap_or(false)
        } else {
            false
        };
        if has_prepare {
            let sandbox = Sandbox::new(&script_dir);
            if let Err(e) = sandbox.run_script("npm run prepare").await {
                log::warn!("Failed to run prepare script for {}: {}", package_name, e);
            }
        }

        Ok(dep_names)
    }

    async fn download_package(&self, package: &Package) -> RpmResult<Vec<u8>> {
        // Check cache first
        let package_version = package.version.to_string();
        if let Some(cached_path) = self.cache.get(&package.name, &package_version).await? {
            log::debug!(
                "Using cached version of {} {}",
                package.name,
                package.version
            );
            return Ok(fs::read(&cached_path).await?);
        }

        let tarball_url = url::Url::parse(&package.dist.tarball)?;
        let response = self
            .http_client
            .get(&package.dist.tarball)
            .send()
            .await
            .map_err(|e| RpmError::DownloadError {
                package: package.name.clone(),
                url: tarball_url.clone(),
                source: e,
            })?
            .error_for_status()
            .map_err(|e| RpmError::DownloadError {
                package: package.name.clone(),
                url: tarball_url.clone(),
                source: e,
            })?;

        // Stream the download and track memory usage
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| RpmError::DownloadError {
                package: package.name.clone(),
                url: tarball_url.clone(),
                source: e,
            })?;

            self.memory_profile.allocate(chunk.len());
            bytes.extend_from_slice(&chunk);
        }

        // Cache the downloaded package
        self.cache
            .put(&package.name, &package_version, &bytes)
            .await?;

        Ok(bytes)
    }

    async fn extract_package(&self, package_data: &[u8], package_name: &str) -> RpmResult<PathBuf> {
        let package_path = self.install_path.join(package_name);
        let package_name = package_name.to_string();
        let temp_dir = tempfile::tempdir()?;
        // Scoped packages like "@nestjs/common" contain a slash; flatten it so the
        // temp filename doesn't introduce a non-existent subdirectory.
        let safe_name = package_name.replace('/', "__");
        let temp_path = temp_dir.path().join(format!("{}.tgz", safe_name));

        // For scoped packages the parent directory (e.g. node_modules/@nestjs) must
        // exist before extraction.
        if let Some(parent) = package_path.parent() {
            fs::create_dir_all(parent).await?;
        }

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

            // Extract to a staging directory that lives next to the final destination
            // so both paths are on the same filesystem (enabling cheap rename).
            let staging_path = package_path_clone.with_extension("__staging");
            if staging_path.exists() {
                std::fs::remove_dir_all(&staging_path)?;
            }

            archive
                .unpack(&staging_path)
                .map_err(|e| RpmError::ExtractionError {
                    package: package_name.to_string(),
                    path: staging_path.clone(),
                    source: e,
                })?;

            // npm tarballs always contain a top-level "package/" directory.
            // Promote its contents to the final destination so that
            // node_modules/<name>/package.json exists (not node_modules/<name>/package/package.json).
            let npm_subdir = staging_path.join("package");
            let src = if npm_subdir.exists() {
                npm_subdir
            } else {
                staging_path.clone()
            };

            if package_path_clone.exists() {
                std::fs::remove_dir_all(&package_path_clone)?;
            }
            std::fs::rename(&src, &package_path_clone).map_err(|e| RpmError::ExtractionError {
                package: package_name.to_string(),
                path: package_path_clone.clone(),
                source: e,
            })?;

            // Remove leftover staging dir (only present when npm_subdir branch was taken).
            if staging_path.exists() {
                let _ = std::fs::remove_dir_all(&staging_path);
            }

            Ok(())
        })
        .await??;

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
