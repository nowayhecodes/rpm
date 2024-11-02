use thiserror::Error;

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Package {0} not found")]
    PackageNotFound(String),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    
    #[error("Invalid URL: {0}")]
    UrlError(#[from] url::ParseError),
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