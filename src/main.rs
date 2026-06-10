use clap::Parser;
use log::info;

use rpm::{
    cache::{CacheConfig, PackageCache},
    config::Config,
    error::RpmResult,
    logging::{setup_logging, LoggingConfig},
    profiling::MemoryProfile,
    AppContext, Cli,
};

fn print_logo() {
    let g = "\x1b[36m";
    let b = "\x1b[1m";
    let r = "\x1b[0m";

    eprintln!();
    eprintln!();
    eprintln!("  {b}{g}RPM{r}  {g}The Fastest Node Package Manager  v{}{r}", env!("CARGO_PKG_VERSION"));
    eprintln!();
}

#[tokio::main]
async fn main() -> RpmResult<()> {
    let cli = Cli::parse();

    print_logo();

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

    let project_dir = std::env::current_dir()?;

    // Load runtime config from rpm.json in the project directory (falls back to defaults).
    let config = Config::load(&project_dir.join("rpm.json"))
        .await
        .unwrap_or_default();

    let memory_profile = MemoryProfile::new(1024 * 1024 * 1024); // 1 GB threshold

    let cache_config = CacheConfig::default();
    let cache_dir = cache_config.cache_dir.clone();
    let package_cache = PackageCache::new(cache_config).await?;

    info!("RPM package manager initialized");
    info!("Cache directory: {}", cache_dir.display());

    let context = AppContext {
        memory_profile: memory_profile.clone(),
        package_cache: package_cache.clone(),
        project_dir,
        config,
    };

    cli.execute_with_context(context).await?;

    info!("Peak memory usage: {} bytes", memory_profile.peak_usage());

    Ok(())
}
