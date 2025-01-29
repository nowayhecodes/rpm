use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct LockedDependency {
    version: String,
    resolved: String,
    integrity: String,
    requires: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    name: String,
    version: String,
    lockfile_version: u32,
    requires: bool,
    dependencies: HashMap<String, LockedDependency>,
}

impl LockFile {
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