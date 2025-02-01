use crate::error::{RpmError, RpmResult};
use async_trait::async_trait;
use sha2::{Sha256, Digest};
use std::path::{Path, PathBuf};
use tokio::fs;
use log::{debug, info, warn};
use std::time::{SystemTime, Duration};

pub struct CacheConfig {
    pub cache_dir: PathBuf,
    pub max_size: u64,        // Maximum cache size in bytes
    pub ttl: Duration,        // Time-to-live for cached packages
    pub cleanup_interval: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rpm");

        Self {
            cache_dir,
            max_size: 1024 * 1024 * 1024, // 1GB
            ttl: Duration::from_secs(7 * 24 * 60 * 60), // 1 week
            cleanup_interval: Duration::from_secs(60 * 60), // 1 hour
        }
    }
}

pub struct PackageCache {
    config: CacheConfig,
    last_cleanup: SystemTime,
}

impl PackageCache {
    pub async fn new(config: CacheConfig) -> RpmResult<Self> {
        fs::create_dir_all(&config.cache_dir).await
            .map_err(|e| RpmError::CacheError(format!("Failed to create cache directory: {}", e)))?;

        let cache = Self {
            config,
            last_cleanup: SystemTime::now(),
        };

        cache.init_cleanup_task();
        Ok(cache)
    }

    fn init_cleanup_task(&self) {
        let config = self.config.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(config.cleanup_interval).await;
                if let Err(e) = Self::cleanup_old_packages(&config).await {
                    warn!("Cache cleanup failed: {}", e);
                }
            }
        });
    }

    pub async fn get(&self, package: &str, version: &str) -> RpmResult<Option<PathBuf>> {
        let cache_key = self.generate_cache_key(package, version);
        let cache_path = self.config.cache_dir.join(cache_key);

        if cache_path.exists() {
            let metadata = fs::metadata(&cache_path).await
                .map_err(|e| RpmError::CacheError(format!("Failed to read cache metadata: {}", e)))?;

            let age = SystemTime::now()
                .duration_since(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH))
                .unwrap_or(Duration::from_secs(0));

            if age <= self.config.ttl {
                debug!("Cache hit for package {} version {}", package, version);
                return Ok(Some(cache_path));
            }
        }

        Ok(None)
    }

    pub async fn put(&self, package: &str, version: &str, data: &[u8]) -> RpmResult<PathBuf> {
        let cache_key = self.generate_cache_key(package, version);
        let cache_path = self.config.cache_dir.join(cache_key);

        // Check cache size before writing
        if let Err(e) = self.ensure_cache_size(data.len() as u64).await {
            warn!("Failed to ensure cache size: {}", e);
        }

        fs::write(&cache_path, data).await
            .map_err(|e| RpmError::CacheError(format!("Failed to write to cache: {}", e)))?;

        info!("Cached package {} version {}", package, version);
        Ok(cache_path)
    }

    fn generate_cache_key(&self, package: &str, version: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!("{}-{}", package, version).as_bytes());
        hex::encode(hasher.finalize())
    }

    async fn ensure_cache_size(&self, new_size: u64) -> RpmResult<()> {
        let mut total_size = new_size;
        let mut entries = Vec::new();

        let mut dir = fs::read_dir(&self.config.cache_dir).await
            .map_err(|e| RpmError::CacheError(format!("Failed to read cache directory: {}", e)))?;

        while let Some(entry) = dir.next_entry().await
            .map_err(|e| RpmError::CacheError(format!("Failed to read cache entry: {}", e)))? {
            
            let metadata = entry.metadata().await
                .map_err(|e| RpmError::CacheError(format!("Failed to read entry metadata: {}", e)))?;
            
            total_size += metadata.len();
            entries.push((entry.path(), metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
        }

        if total_size > self.config.max_size {
            // Sort by modification time, oldest first
            entries.sort_by_key(|(_path, time)| *time);

            // Remove oldest entries until we're under the limit
            for (path, _) in entries {
                if total_size <= self.config.max_size {
                    break;
                }

                if let Ok(metadata) = fs::metadata(&path).await {
                    total_size -= metadata.len();
                    if let Err(e) = fs::remove_file(&path).await {
                        warn!("Failed to remove cache entry {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn cleanup_old_packages(config: &CacheConfig) -> RpmResult<()> {
        let mut dir = fs::read_dir(&config.cache_dir).await
            .map_err(|e| RpmError::CacheError(format!("Failed to read cache directory: {}", e)))?;

        while let Some(entry) = dir.next_entry().await
            .map_err(|e| RpmError::CacheError(format!("Failed to read cache entry: {}", e)))? {
            
            let metadata = entry.metadata().await
                .map_err(|e| RpmError::CacheError(format!("Failed to read entry metadata: {}", e)))?;

            let age = SystemTime::now()
                .duration_since(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH))
                .unwrap_or(Duration::from_secs(0));

            if age > config.ttl {
                if let Err(e) = fs::remove_file(entry.path()).await {
                    warn!("Failed to remove old cache entry {}: {}", entry.path().display(), e);
                }
            }
        }

        Ok(())
    }
} 