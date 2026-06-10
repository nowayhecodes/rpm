use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LockedDependency {
    pub version: String,
    /// Tarball URL from the npm registry (`dist.tarball`).
    pub resolved: String,
    /// SHA-1 integrity hash (`dist.shasum`).
    pub integrity: String,
    pub requires: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LockFile {
    pub name: String,
    pub version: String,
    pub lockfile_version: u32,
    pub requires: bool,
    pub dependencies: HashMap<String, LockedDependency>,
}

impl LockFile {
    /// Creates a new, empty lock file for the project identified by `name` and `version`.
    pub fn new(name: String, version: String) -> Self {
        Self {
            name,
            version,
            lockfile_version: 1,
            requires: true,
            dependencies: HashMap::new(),
        }
    }

    pub async fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&content)?)
    }

    pub async fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).await?;
        Ok(())
    }

    pub fn add_dependency(
        &mut self,
        name: String,
        version: String,
        resolved: String,
        integrity: String,
        requires: Option<HashMap<String, String>>,
    ) {
        self.dependencies.insert(name, LockedDependency {
            version,
            resolved,
            integrity,
            requires,
        });
    }

    pub fn get_dependency(&self, name: &str) -> Option<&LockedDependency> {
        self.dependencies.get(name)
    }
}
