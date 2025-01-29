use semver::{Version, VersionReq};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VersionError {
    #[error("Invalid version requirement: {0}")]
    InvalidRequirement(String),
    #[error("No compatible version found for {package} (requirement: {requirement})")]
    NoCompatibleVersion { package: String, requirement: String },
}

pub struct VersionManager {
    version_constraints: HashMap<String, VersionReq>,
    resolved_versions: HashMap<String, Version>,
}

impl VersionManager {
    pub fn new() -> Self {
        Self {
            version_constraints: HashMap::new(),
            resolved_versions: HashMap::new(),
        }
    }

    pub fn add_constraint(&mut self, package: String, requirement: &str) -> Result<(), VersionError> {
        let req = VersionReq::parse(requirement)
            .map_err(|_| VersionError::InvalidRequirement(requirement.to_string()))?;
        self.version_constraints.insert(package, req);
        Ok(())
    }

    pub fn resolve_version(&mut self, package: &str, available_versions: &[Version]) -> Result<Version, VersionError> {
        let requirement = self.version_constraints.get(package)
            .ok_or_else(|| VersionError::NoCompatibleVersion {
                package: package.to_string(),
                requirement: "*".to_string(),
            })?;

        let compatible_version = available_versions.iter()
            .filter(|v| requirement.matches(v))
            .max()
            .ok_or_else(|| VersionError::NoCompatibleVersion {
                package: package.to_string(),
                requirement: requirement.to_string(),
            })?;

        let version = compatible_version.clone();
        self.resolved_versions.insert(package.to_string(), version.clone());
        Ok(version)
    }

    pub fn get_resolved_version(&self, package: &str) -> Option<&Version> {
        self.resolved_versions.get(package)
    }
} 