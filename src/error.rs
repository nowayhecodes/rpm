use thiserror::Error;

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