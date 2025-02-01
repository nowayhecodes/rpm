use thiserror::Error;
use std::path::PathBuf;
use url::Url;

#[derive(Error, Debug)]
pub enum RpmError {
    #[error("Failed to download package {package} from {url}: {source}")]
    DownloadError {
        package: String,
        url: Url,
        source: reqwest::Error,
    },

    #[error("Package {0} not found in registry")]
    PackageNotFound(String),

    #[error("Invalid version constraint: {0}")]
    InvalidVersion(String),

    #[error("Failed to parse package.json: {0}")]
    PackageJsonError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to extract package {package} to {path}: {source}")]
    ExtractionError {
        package: String,
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Security vulnerability found in {package} version {version}: {details}")]
    SecurityVulnerability {
        package: String,
        version: String,
        details: String,
    },

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Memory limit exceeded: {0}")]
    MemoryError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Dependency resolution error: {0}")]
    DependencyError(String),

    #[error("Verification failed: {0}")]
    VerificationError(String),
}

pub type RpmResult<T> = Result<T, RpmError>;

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Package {0} not found")]
    PackageNotFound(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Invalid URL: {0}")]
    UrlError(#[from] url::ParseError),

    #[error("Failed to deserialize package data: {0}")]
    DeserializationError(String),
}

#[derive(Error, Debug)]
pub enum InstallError {
    #[error("Failed to download package: {0}")]
    DownloadError(String),

    #[error("Failed to extract package: {0}")]
    ExtractionError(String),

    #[error("Package verification failed: {0}")]
    _VerificationError(String),
}

#[derive(Error, Debug)]
pub enum DependencyError {
    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),
    
    #[error("Registry error: {0}")]
    RegistryError(#[from] RegistryError),
}

#[derive(Error, Debug)]
pub enum ConcurrencyError {
    #[error("Failed to download package: {0}")]
    DownloadError(String),
}

#[derive(Error, Debug)]
pub enum ResolverError {
    #[error("Failed to resolve dependencies: {0}")]
    DependencyResolutionError(String),
}

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("Failed to download package: {0}")]
    DownloadError(String),
}

impl From<tokio::sync::AcquireError> for DownloadError {
    fn from(_: tokio::sync::AcquireError) -> Self {
        Self::DownloadError("Failed to acquire semaphore".to_string())
    }
}

impl From<reqwest::Error> for DownloadError {
    fn from(e: reqwest::Error) -> Self {
        Self::DownloadError(e.to_string())
    }
}

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("No safe version found for package {0}")]
    NoSafeVersion(String),
    
    #[error("Failed to check security: {0}")]
    CheckFailed(String),
}