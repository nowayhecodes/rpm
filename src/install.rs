use crate::{
    cache::PackageCache,
    concurrency::ConcurrentDownloader,
    config::Config,
    error::{RpmError, RpmResult},
    lockfile::LockFile,
    package::Package,
    profiling::MemoryProfile,
    progress::{ProgressEvent, ProgressReporter},
    registry::RegistryClient,
    sandbox::Sandbox,
    verification::{ChecksumIntegrityChecker, Verification},
    version::VersionManager,
};
use flate2::read::GzDecoder;
use futures::future::try_join_all;
use indicatif::{ProgressBar, ProgressStyle};
use semver::VersionReq;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tar::Archive;
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::Mutex;

pub struct PackageInstaller {
    global: bool,
    registry: Arc<RegistryClient>,
    install_path: PathBuf,
    project_dir: PathBuf,
    downloader: Arc<ConcurrentDownloader>,
    cache: PackageCache,
    memory_profile: MemoryProfile,
    version_manager: Arc<Mutex<VersionManager>>,
    lock_file: Arc<Mutex<LockFile>>,
    progress: Arc<ProgressReporter>,
}

impl PackageInstaller {
    pub fn new(
        global: bool,
        cache: PackageCache,
        memory_profile: MemoryProfile,
        config: &Config,
        lock_file: Arc<Mutex<LockFile>>,
    ) -> Self {
        Self::new_in_project(global, cache, memory_profile, ".", config, lock_file)
    }

    pub fn new_in_project(
        global: bool,
        cache: PackageCache,
        memory_profile: MemoryProfile,
        project_dir: impl Into<PathBuf>,
        config: &Config,
        lock_file: Arc<Mutex<LockFile>>,
    ) -> Self {
        let project_dir = project_dir.into();
        let install_path = if global {
            config.global_packages_dir.clone()
        } else {
            project_dir.join("node_modules")
        };

        let (progress, _) = ProgressReporter::new(0);

        Self {
            global,
            registry: Arc::new(RegistryClient::with_config(config)),
            install_path,
            project_dir,
            downloader: Arc::new(ConcurrentDownloader::new(config.max_concurrent_downloads)),
            cache,
            memory_profile,
            version_manager: Arc::new(Mutex::new(VersionManager::new())),
            lock_file,
            progress,
        }
    }

    /// Subscribe to install progress events.  Call before `install_packages` to
    /// avoid missing the `Started` event.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ProgressEvent> {
        self.progress.subscribe()
    }

    /// Returns a shared handle to the internal lock file so callers can read or
    /// persist it after install completes.
    pub fn lock_file(&self) -> Arc<Mutex<LockFile>> {
        Arc::clone(&self.lock_file)
    }

    /// Installs `packages` (and all transitive dependencies) using a parallel BFS
    /// strategy.  After completion the lock file is written to `rpm-lock.json` in
    /// the project directory (local installs only).
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

        self.progress.set_total(packages.len());
        self.progress
            .report_progress(ProgressEvent::Started { total: packages.len() });

        // Track every package name that has been queued so we never install the
        // same package twice (even if different parents request it).
        let queued: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(
            packages.iter().cloned().collect(),
        ));

        // Each BFS element carries the package name and the semver requirement
        // declared by its parent (None for the top-level explicit packages).
        let mut current_batch: Vec<(String, Option<VersionReq>)> = packages
            .iter()
            .cloned()
            .map(|name| (name, None))
            .collect();

        while !current_batch.is_empty() {
            let tasks: Vec<_> = current_batch
                .into_iter()
                .map(|(package, version_req)| {
                    let installer = self.clone();
                    let bar = Arc::clone(&bar);
                    tokio::spawn(async move {
                        installer.install_one(package, version_req, bar).await
                    })
                })
                .collect();

            let mut next_raw: Vec<(String, Option<VersionReq>)> = Vec::new();
            for result in try_join_all(tasks).await? {
                next_raw.extend(result?);
            }

            let next_batch: Vec<(String, Option<VersionReq>)> = {
                let mut lock = queued.lock().await;
                next_raw
                    .into_iter()
                    .filter(|(name, _)| lock.insert(name.clone()))
                    .collect()
            };

            if !next_batch.is_empty() {
                bar.inc_length(next_batch.len() as u64);
                self.progress.add_total(next_batch.len());
            }
            current_batch = next_batch;
        }

        bar.finish_and_clear();
        self.progress.report_progress(ProgressEvent::Completed);

        // Persist the lock file for local (non-global) installs.
        if !self.global {
            let lock_path = self.project_dir.join("rpm-lock.json");
            if let Err(e) = self.lock_file.lock().await.save(&lock_path).await {
                log::warn!("Failed to write rpm-lock.json: {}", e);
            }
        }

        Ok(())
    }

    /// Installs an exact set of `(name, version)` pairs from a lock file without
    /// performing BFS transitive resolution.  All packages must already be listed
    /// in the lock file (i.e., they were previously resolved).
    pub async fn install_locked_packages(&self, packages: &[(String, String)]) -> RpmResult<()> {
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

        self.progress.set_total(packages.len());
        self.progress
            .report_progress(ProgressEvent::Started { total: packages.len() });

        let tasks: Vec<_> = packages
            .iter()
            .cloned()
            .map(|(name, version)| {
                let installer = self.clone();
                let bar = Arc::clone(&bar);
                tokio::spawn(async move {
                    installer.install_one_locked(name, version, bar).await
                })
            })
            .collect();

        for result in try_join_all(tasks).await? {
            result?;
        }

        bar.finish_and_clear();
        self.progress.report_progress(ProgressEvent::Completed);

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// Downloads, verifies, and extracts one package.  Returns the (name, version_req)
    /// pairs of its declared dependencies so the BFS caller can queue them.
    async fn install_one(
        &self,
        package_name: String,
        version_req: Option<VersionReq>,
        bar: Arc<ProgressBar>,
    ) -> RpmResult<Vec<(String, Option<VersionReq>)>> {
        bar.set_message(format!("downloading {}", package_name));
        self.progress
            .report_progress(ProgressEvent::Downloaded { package: package_name.clone() });

        // Resolve the best version satisfying the declared constraint, or latest.
        let package_info = self
            .registry
            .fetch_best_matching(&package_name, version_req.as_ref())
            .await?;

        // Record the resolved version in the VersionManager for diagnostics and
        // future constraint checks.
        {
            let mut vm = self.version_manager.lock().await;
            if let Some(ref req) = version_req {
                let _ = vm.add_constraint(package_name.clone(), &req.to_string());
            }
            let _ = vm.resolve_version(&package_name, &[package_info.version.clone()]);
        }

        let package_data = self.download_package(&package_info).await?;

        bar.set_message(format!("verifying {}", package_name));
        self.progress
            .report_progress(ProgressEvent::Verified { package: package_name.clone() });
        ChecksumIntegrityChecker::verify_package(&package_data, &package_info.dist.shasum)?;

        bar.set_message(format!("extracting {}", package_name));
        let package_path = self.extract_package(&package_data, &package_name).await?;

        // Build the next BFS level: direct deps with their declared version reqs.
        let dep_pairs: Vec<(String, Option<VersionReq>)> = package_info
            .dependencies
            .iter()
            .map(|(dep_name, dep_version_str)| {
                let req = VersionReq::parse(dep_version_str).ok();
                (dep_name.clone(), req)
            })
            .collect();

        // Write this package into the shared lock file.
        {
            let requires: HashMap<String, String> = package_info.dependencies.clone();
            let mut lock = self.lock_file.lock().await;
            lock.add_dependency(
                package_name.clone(),
                package_info.version.to_string(),
                package_info.dist.tarball.clone(),
                package_info.dist.shasum.clone(),
                if requires.is_empty() { None } else { Some(requires) },
            );
        }

        bar.inc(1);
        self.progress
            .report_progress(ProgressEvent::Installed { package: package_name.clone() });

        // Run postinstall only — prepare is a development-time lifecycle hook and
        // must NOT be executed for packages installed from the npm registry. Only
        // pure `node <script>` invocations are safe without the package's devDeps
        // installed; scripts calling external tools (husky, cmake, gyp …) are skipped.
        let pkg_json_path = package_path.join("package.json");
        let postinstall_cmd: Option<String> =
            if let Ok(content) = fs::read_to_string(&pkg_json_path).await {
                serde_json::from_str::<serde_json::Value>(&content)
                    .ok()
                    .and_then(|v| {
                        v["scripts"]["postinstall"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                    })
            } else {
                None
            };

        if let Some(cmd) = postinstall_cmd {
            if let Some(node_args) = cmd.strip_prefix("node ") {
                let sandbox = Sandbox::new(&package_path);
                if let Err(e) = sandbox.run_node_script(node_args).await {
                    log::warn!("Failed to run postinstall for {}: {}", package_name, e);
                    self.progress.report_progress(ProgressEvent::Failed {
                        package: package_name.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(dep_pairs)
    }

    /// Installs one package at an exact version, skipping BFS transitive resolution.
    async fn install_one_locked(
        &self,
        package_name: String,
        version: String,
        bar: Arc<ProgressBar>,
    ) -> RpmResult<()> {
        bar.set_message(format!("downloading {}", package_name));
        self.progress
            .report_progress(ProgressEvent::Downloaded { package: package_name.clone() });

        let package_info = self
            .registry
            .fetch_package_info(&package_name, Some(&version))
            .await?;

        let package_data = self.download_package(&package_info).await?;

        bar.set_message(format!("verifying {}", package_name));
        self.progress
            .report_progress(ProgressEvent::Verified { package: package_name.clone() });
        ChecksumIntegrityChecker::verify_package(&package_data, &package_info.dist.shasum)?;

        bar.set_message(format!("extracting {}", package_name));
        self.extract_package(&package_data, &package_name).await?;

        bar.inc(1);
        self.progress
            .report_progress(ProgressEvent::Installed { package: package_name });

        Ok(())
    }

    async fn download_package(&self, package: &Package) -> RpmResult<Vec<u8>> {
        let package_version = package.version.to_string();

        if let Some(cached_path) = self.cache.get(&package.name, &package_version).await? {
            log::debug!(
                "Using cached version of {} {}",
                package.name,
                package.version
            );
            return Ok(fs::read(&cached_path).await?);
        }

        let bytes = self
            .downloader
            .download(&package.dist.tarball)
            .await
            .map_err(|e| {
                RpmError::Other(anyhow::anyhow!(
                    "Failed to download {}: {}",
                    package.name,
                    e
                ))
            })?;

        self.memory_profile.allocate(bytes.len());

        self.cache
            .put(&package.name, &package_version, &bytes)
            .await?;

        Ok(bytes)
    }

    async fn extract_package(
        &self,
        package_data: &[u8],
        package_name: &str,
    ) -> RpmResult<PathBuf> {
        let package_path = self.install_path.join(package_name);
        let package_name = package_name.to_string();
        let temp_dir = tempfile::tempdir()?;
        let safe_name = package_name.replace('/', "__");
        let temp_path = temp_dir.path().join(format!("{}.tgz", safe_name));

        if let Some(parent) = package_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        self.memory_profile.allocate(package_data.len());

        let file = tokio::fs::File::create(&temp_path).await?;
        let mut writer = BufWriter::new(file);
        writer.write_all(package_data).await?;
        writer.flush().await?;

        let package_path_clone = package_path.clone();
        tokio::task::spawn_blocking(move || -> RpmResult<()> {
            let tar_gz = std::fs::File::open(temp_path)?;
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);

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

            if staging_path.exists() {
                let _ = std::fs::remove_dir_all(&staging_path);
            }

            Ok(())
        })
        .await??;

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
            project_dir: self.project_dir.clone(),
            downloader: Arc::clone(&self.downloader),
            cache: self.cache.clone(),
            memory_profile: self.memory_profile.clone(),
            version_manager: Arc::clone(&self.version_manager),
            lock_file: Arc::clone(&self.lock_file),
            progress: Arc::clone(&self.progress),
        }
    }
}
