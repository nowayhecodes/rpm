use serde::{Deserialize, Serialize};
use semver::Version;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: Version,
    pub dependencies: HashMap<String, String>,
    pub dist: PackageDistribution,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageDistribution {
    pub tarball: String,
    pub shasum: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageJson {
    pub name: String,
    pub version: String,
    pub dependencies: Option<HashMap<String, String>>,
    pub dev_dependencies: Option<HashMap<String, String>>,
} 