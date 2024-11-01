use serde::{Deserialize, Serialize};
use semver::Version;
use std::collections::HashMap;
use tokio::fs;
use anyhow::Result;

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
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<HashMap<String, String>>,
}

impl PackageJson {
    pub async fn load() -> Result<Self> {
        let content = fs::read_to_string("package.json").await?;
        Ok(serde_json::from_str(&content)?)
    }

    pub async fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write("package.json", content).await?;
        Ok(())
    }

    pub fn remove_dependency(&mut self, package: &str) {
        if let Some(deps) = &mut self.dependencies {
            deps.remove(package);
        }
        if let Some(dev_deps) = &mut self.dev_dependencies {
            dev_deps.remove(package);
        }
    }
} 