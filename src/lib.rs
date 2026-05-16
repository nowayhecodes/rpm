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

pub use cli::Cli;
pub use package::PackageJson;
pub use security::SecurityChecker;

#[derive(Clone)]
pub struct AppContext {
    pub memory_profile: MemoryProfile,
    pub package_cache: PackageCache,
}
