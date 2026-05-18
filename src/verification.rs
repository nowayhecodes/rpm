use sha1::{Digest, Sha1};
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
        let mut hasher = Sha1::new();
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

        let expected_shasum = "0dc143eeeaec03dc8e7d61a631a0752e73bd401e";

        assert!(ChecksumIntegrityChecker::verify_package(package_data, expected_shasum).is_ok());
    }

    #[test]
    fn test_verify_package_failure() {
        let package_data = b"test package data";

        let wrong_shasum = "0000000000000000000000000000000000000000";

        let result = ChecksumIntegrityChecker::verify_package(package_data, wrong_shasum);
        assert!(result.is_err());

        match result {
            Err(VerificationError::ChecksumMismatch { expected, actual }) => {
                assert_eq!(expected, wrong_shasum);
                assert_ne!(actual, "0000000000000000000000000000000000000000");
            }
            _ => panic!("Expected ChecksumMismatch error"),
        }
    }

    #[test]
    fn test_verify_package_empty_data() {
        let package_data = b"";

        let expected_shasum = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

        assert!(ChecksumIntegrityChecker::verify_package(package_data, expected_shasum).is_ok());
    }
}
