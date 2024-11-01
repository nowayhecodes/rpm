use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VerificationError {
    #[error("Package checksum verification failed: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

pub struct ChecksumIntegrityChecker;

pub trait Verification {
    fn verify_package(package_data: &[u8], expected_shasum: &str) -> Result<(), VerificationError>;
}

impl Verification for ChecksumIntegrityChecker {
    fn verify_package(package_data: &[u8], expected_shasum: &str) -> Result<(), VerificationError> {
        let mut hasher = Sha256::new();
        hasher.update(package_data);
        let actual_hash = hasher.finalize();

        let actual_shasum = hex::encode(actual_hash);

        if actual_shasum != expected_shasum {
            return Err(VerificationError::ChecksumMismatch {
                expected: expected_shasum.to_string(),
                actual: actual_shasum,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_package_success() {
        let package_data = b"test package data";

        let expected_shasum = "0f94aa4b597462a8eb1abb5399a4ca32d30e3a118aaf7ef89a5b2435c8461157";

        assert!(ChecksumIntegrityChecker::verify_package(package_data, expected_shasum).is_ok());
    }

    #[test]
    fn test_verify_package_failure() {
        let package_data = b"test package data";

        let wrong_shasum = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = ChecksumIntegrityChecker::verify_package(package_data, wrong_shasum);
        assert!(result.is_err());

        match result {
            Err(VerificationError::ChecksumMismatch { expected, actual }) => {
                assert_eq!(expected, wrong_shasum);
                assert_eq!(
                    actual,
                    "0f94aa4b597462a8eb1abb5399a4ca32d30e3a118aaf7ef89a5b2435c8461157"
                );
            }
            _ => panic!("Expected ChecksumMismatch error"),
        }
    }

    #[test]
    fn test_verify_package_empty_data() {
        let package_data = b"";

        let expected_shasum = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

        assert!(ChecksumIntegrityChecker::verify_package(package_data, expected_shasum).is_ok());
    }
}
