use anyhow::Result;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use tokio::fs;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies", skip_serializing_if = "Option::is_none")]
    pub dev_dependencies: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

impl PackageJson {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: "1.0.0".to_string(),
            description: None,
            main: Some("index.js".to_string()),
            types: None,
            scripts: Some(BTreeMap::from([(
                "test".to_string(),
                "echo \"Error: no test specified\" && exit 1".to_string(),
            )])),
            dependencies: None,
            dev_dependencies: None,
            license: Some("ISC".to_string()),
        }
    }

    pub async fn load() -> Result<Self> {
        let content = fs::read_to_string("package.json").await?;
        Ok(serde_json::from_str(&content)?)
    }

    pub async fn save(&self) -> Result<()> {
        self.save_to("package.json").await
    }

    pub async fn save_to(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).await?;
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

    pub async fn load_from(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&content)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs;

    async fn create_test_package_json(dir: &std::path::Path) -> Result<()> {
        let package_json = PackageJson {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            main: None,
            types: None,
            scripts: None,
            dependencies: Some(HashMap::from([
                ("express".to_string(), "^4.17.1".to_string()),
                ("react".to_string(), "^17.0.2".to_string()),
            ])),
            dev_dependencies: Some(HashMap::from([
                ("typescript".to_string(), "^4.5.4".to_string()),
                ("jest".to_string(), "^27.4.7".to_string()),
            ])),
            license: None,
        };

        let content = serde_json::to_string_pretty(&package_json)?;
        fs::write(dir.join("package.json"), content).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_load_package_json() -> Result<()> {
        let temp_dir = tempdir()?;
        create_test_package_json(temp_dir.path()).await?;

        let package_json = PackageJson::load_from(temp_dir.path().join("package.json")).await?;

        assert_eq!(package_json.name, "test-package");
        assert_eq!(package_json.version, "1.0.0");

        let dependencies = package_json.dependencies.unwrap();
        assert_eq!(dependencies.get("express").unwrap(), "^4.17.1");
        assert_eq!(dependencies.get("react").unwrap(), "^17.0.2");

        let dev_dependencies = package_json.dev_dependencies.unwrap();
        assert_eq!(dev_dependencies.get("typescript").unwrap(), "^4.5.4");
        assert_eq!(dev_dependencies.get("jest").unwrap(), "^27.4.7");

        Ok(())
    }

    #[tokio::test]
    async fn test_save_package_json() -> Result<()> {
        let temp_dir = tempdir()?;

        let package_json = PackageJson {
            name: "save-test".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            main: None,
            types: None,
            scripts: None,
            dependencies: Some(HashMap::from([(
                "lodash".to_string(),
                "^4.17.21".to_string(),
            )])),
            dev_dependencies: None,
            license: None,
        };

        let package_json_path = temp_dir.path().join("package.json");
        package_json.save_to(&package_json_path).await?;

        let content = fs::read_to_string(package_json_path).await?;
        let loaded: PackageJson = serde_json::from_str(&content)?;

        assert_eq!(loaded.name, "save-test");
        assert_eq!(
            loaded.dependencies.unwrap().get("lodash").unwrap(),
            "^4.17.21"
        );
        assert!(loaded.dev_dependencies.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_dependency() -> Result<()> {
        let temp_dir = tempdir()?;
        create_test_package_json(temp_dir.path()).await?;

        let mut package_json = PackageJson::load_from(temp_dir.path().join("package.json")).await?;

        package_json.remove_dependency("express");
        package_json.remove_dependency("typescript");

        let dependencies = package_json.dependencies.unwrap();
        assert!(!dependencies.contains_key("express"));
        assert!(dependencies.contains_key("react"));

        let dev_dependencies = package_json.dev_dependencies.unwrap();
        assert!(!dev_dependencies.contains_key("typescript"));
        assert!(dev_dependencies.contains_key("jest"));

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_nonexistent_dependency() -> Result<()> {
        let temp_dir = tempdir()?;
        create_test_package_json(temp_dir.path()).await?;

        let mut package_json = PackageJson::load_from(temp_dir.path().join("package.json")).await?;

        package_json.remove_dependency("nonexistent-package");

        let dependencies = package_json.dependencies.unwrap();
        assert_eq!(dependencies.len(), 2);
        assert!(dependencies.contains_key("express"));
        assert!(dependencies.contains_key("react"));

        Ok(())
    }
}
