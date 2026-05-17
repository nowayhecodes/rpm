pub mod cache;
pub mod cli;
pub mod concurrency;
pub mod config;
pub mod dependency;
pub mod error;
pub mod install;
pub mod lockfile;
pub mod logging;
pub mod package;
pub mod profiling;
pub mod progress;
pub mod registry;
pub mod sandbox;
pub mod security;
pub mod verification;
pub mod version;

use cache::PackageCache;
use profiling::MemoryProfile;
use std::path::{Path, PathBuf};

pub use cli::Cli;
pub use package::PackageJson;
pub use security::SecurityChecker;

#[derive(Clone)]
pub struct AppContext {
    pub memory_profile: MemoryProfile,
    pub package_cache: PackageCache,
    pub project_dir: PathBuf,
}

impl AppContext {
    pub fn new(
        memory_profile: MemoryProfile,
        package_cache: PackageCache,
        project_dir: PathBuf,
    ) -> Self {
        Self {
            memory_profile,
            package_cache,
            project_dir,
        }
    }

    pub fn package_json_path(&self) -> PathBuf {
        self.project_dir.join("package.json")
    }

    pub fn node_modules_path(&self) -> PathBuf {
        self.project_dir.join("node_modules")
    }

    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }
}
