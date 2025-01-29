use crate::error::DependencyError;
use crate::package::{Package, PackageJson};
use crate::registry::RegistryClient;
use semver::{Version, VersionReq};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version_req: VersionReq,
    pub resolved_version: Option<Version>,
}

pub struct DependencyResolver {
    registry: Arc<RegistryClient>,
    resolved_deps: Arc<Mutex<HashMap<String, Version>>>,
    processing: Arc<Mutex<HashSet<String>>>,
}

impl DependencyResolver {
    pub fn new(registry: Arc<RegistryClient>) -> Self {
        Self {
            registry,
            resolved_deps: Arc::new(Mutex::new(HashMap::new())),
            processing: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub async fn resolve_dependencies(
        &self,
        package_json: &PackageJson,
    ) -> Result<Vec<Package>, DependencyError> {
        let mut resolved_packages = Vec::new();
        let deps = self.collect_all_dependencies(package_json);

        for (name, version_req) in deps {
            let package = self.resolve_single_dependency(&name, &version_req).await?;
            resolved_packages.push(package);
        }

        Ok(resolved_packages)
    }

    pub async fn resolve_single_dependency(
        &self,
        name: &str,
        version_req: &VersionReq,
    ) -> Result<Package, DependencyError> {
        {
            let mut processing = self.processing.lock().await;
            if processing.contains(name) {
                return Err(DependencyError::CircularDependency(name.to_string()));
            }
            processing.insert(name.to_string());
        }

        {
            let resolved = self.resolved_deps.lock().await;
            if let Some(version) = resolved.get(name) {
                if version_req.matches(version) {
                    let mut processing = self.processing.lock().await;
                    processing.remove(name);
                    return Ok(self
                        .registry
                        .fetch_package_info(name, Some(&version.to_string()))
                        .await?);
                }
            }
        }

        let package = self.registry.fetch_package_info(name, None).await?;

        {
            let mut resolved = self.resolved_deps.lock().await;
            resolved.insert(name.to_string(), package.version.clone());
        }

        {
            let mut processing = self.processing.lock().await;
            processing.remove(name);
        }

        Ok(package)
    }

    fn collect_all_dependencies(&self, package_json: &PackageJson) -> HashMap<String, VersionReq> {
        let mut all_deps = HashMap::new();

        if let Some(deps) = &package_json.dependencies {
            for (name, version) in deps {
                if let Ok(version_req) = VersionReq::parse(version) {
                    all_deps.insert(name.clone(), version_req);
                }
            }
        }

        if let Some(dev_deps) = &package_json.dev_dependencies {
            for (name, version) in dev_deps {
                if let Ok(version_req) = VersionReq::parse(version) {
                    all_deps.insert(name.clone(), version_req);
                }
            }
        }

        all_deps
    }
}
