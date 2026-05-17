use clap::Parser;
use log::info;

use rpm::{
    cache::{CacheConfig, PackageCache},
    error::RpmResult,
    logging::{setup_logging, LoggingConfig},
    profiling::MemoryProfile,
    AppContext, Cli,
};

#[tokio::main]
async fn main() -> RpmResult<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Setup logging based on verbosity flag
    let logging_config = LoggingConfig {
        level: if cli.verbose {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        },
        show_timestamps: true,
        show_module_path: cli.verbose,
        color: true,
    };
    setup_logging(logging_config);

    // Initialize memory profiling
    let memory_profile = MemoryProfile::new(1024 * 1024 * 1024); // 1GB threshold

    // Initialize package cache
    let cache_config = CacheConfig::default();
    let cache_dir = cache_config.cache_dir.clone();
    let package_cache = PackageCache::new(cache_config).await?;

    info!("RPM package manager initialized");
    info!("Cache directory: {}", cache_dir.display());

    // Create application context with shared resources
    let context = AppContext {
        memory_profile: memory_profile.clone(),
        package_cache: package_cache.clone(),
        project_dir: std::env::current_dir()?,
    };

    // Execute CLI command with context
    cli.execute_with_context(context).await?;

    // Log final memory usage statistics
    info!("Peak memory usage: {} bytes", memory_profile.peak_usage());

    Ok(())
}
