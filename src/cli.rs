use crate::{
    AppContext,
    error::RpmResult,
    install::PackageInstaller,
    package::PackageJson,
};
use clap::{Parser, Subcommand};
use log::{debug, info};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    Install {
        packages: Vec<String>,
        #[arg(short, long)]
        global: bool,
    },
    Update {
        packages: Vec<String>,
    },
    Remove {
        packages: Vec<String>,
        #[arg(short, long)]
        global: bool,
    },
    List {
        #[arg(short, long)]
        global: bool,
    },
    Audit {
        #[arg(long)]
        fix: bool,
    },
}

impl Cli {
    pub async fn execute(self, context: AppContext) -> RpmResult<()> {
        match self.command {
            Commands::Install { packages, global } => {
                debug!("Installing packages: {:?}", packages);
                let installer = PackageInstaller::new(
                    global,
                    context.package_cache,
                    context.memory_profile,
                );
                installer.install_packages(&packages).await?;
                info!("Successfully installed packages: {:?}", packages);
            }
            Commands::Update { packages } => {
                let package_json = PackageJson::load().await?;
                let registry = Arc::new(RegistryClient::new());
                let resolver = DependencyResolver::new(registry);

                match packages {
                    Some(packages) => {
                        for package in packages {
                            if let Some(deps) = &package_json.dependencies {
                                if let Some(current_version) = deps.get(&package) {
                                    println!(
                                        "Updating {} from version {}",
                                        package, current_version
                                    );

                                    let latest = resolver
                                        .resolve_single_dependency(
                                            &package,
                                            &semver::VersionReq::parse("*").unwrap(),
                                        )
                                        .await?;

                                    if latest.version
                                        > semver::Version::parse(current_version).unwrap()
                                    {
                                        let package_name = package.clone();
                                        let installer = PackageInstaller::new(false);
                                        installer.install_packages(&[package_name]).await?;
                                        println!(
                                            "Updated {} to version {}",
                                            package, latest.version
                                        );
                                    } else {
                                        println!("{} is already at the latest version", package);
                                    }
                                }
                            }
                        }
                    }

                    None => {
                        if let Some(deps) = &package_json.dependencies {
                            for (package, current_version) in deps {
                                println!(
                                    "Checking updates for {} (current: {})",
                                    package, current_version
                                );

                                let latest = resolver
                                    .resolve_single_dependency(
                                        package,
                                        &semver::VersionReq::parse("*").unwrap(),
                                    )
                                    .await?;

                                if latest.version > semver::Version::parse(current_version).unwrap()
                                {
                                    let package_name = package.clone();
                                    let installer = PackageInstaller::new(false);
                                    installer.install_packages(&[package_name]).await?;
                                    println!("Updated {} to version {}", package, latest.version);
                                } else {
                                    println!("{} is already at the latest version", package);
                                }
                            }
                        }
                    }
                }
            }
            Commands::Remove { packages, global } => {
                let base_path = if global {
                    PathBuf::from("/usr/local/lib/node_modules")
                } else {
                    PathBuf::from("node_modules")
                };

                for package in packages {
                    let package_path = base_path.join(&package);
                    if package_path.exists() {
                        fs::remove_dir_all(&package_path).await?;
                        println!("Successfully removed package: {}", package);

                        if !global {
                            if let Ok(mut package_json) = PackageJson::load().await {
                                package_json.remove_dependency(&package);
                                package_json.save().await?;
                            }
                        }
                    } else {
                        println!("Package not found: {}", package);
                    }
                }
            }
            Commands::List => {
                println!("Installed packages:");
                
                // Read package.json for local packages
                if let Ok(package_json) = PackageJson::load().await {
                    println!("\nLocal packages:");
                    if let Some(deps) = &package_json.dependencies {
                        for (name, version) in deps {
                            println!("  {} @ {}", name, version);
                        }
                    }
                    
                    if let Some(dev_deps) = &package_json.dev_dependencies {
                        println!("\nDev dependencies:");
                        for (name, version) in dev_deps {
                            println!("  {} @ {}", name, version);
                        }
                    }
                }
                
                // List global packages
                let global_dir = PathBuf::from("/usr/local/lib/node_modules");
                if global_dir.exists() {
                    println!("\nGlobal packages:");
                    let mut entries = fs::read_dir(global_dir).await?;
                    while let Some(entry) = entries.next_entry().await? {
                        if entry.file_type().await?.is_dir() {
                            if let Some(name) = entry.file_name().to_str() {
                                if let Ok(package_json) = PackageJson::load_from(entry.path().join("package.json")).await {
                                    println!("  {} @ {}", name, package_json.version);
                                }
                            }
                        }
                    }
                }
            }
            Commands::Audit { fix } => {
                println!("Auditing packages for security vulnerabilities...");
                
                let mut package_json = PackageJson::load().await?;
                let mut security_checker = SecurityChecker::new();
                let mut vulnerabilities_found = false;
                let mut fixes_applied = false;

                if let Some(deps) = &mut package_json.dependencies {
                    let mut updates = Vec::new();

                    for (name, version_str) in deps.iter() {
                        let version = Version::parse(version_str)?;
                        let vulns = security_checker.check_package(name, &version).await?;

                        if !vulns.is_empty() {
                            vulnerabilities_found = true;
                            println!("\nVulnerabilities found in {}", name);
                            
                            for vuln in &vulns {
                                println!("\nID: {}", vuln.id);
                                println!("Title: {}", vuln.title);
                                println!("Severity: {}", vuln.severity);
                                println!("Description: {}", vuln.description);
                                
                                if let Some(patched) = &vuln.patched_version {
                                    println!("Patched version: {}", patched);
                                }
                            }

                            if fix {
                                // Get all available versions from registry
                                let registry = Arc::new(RegistryClient::new());
                                let package_info = registry.fetch_package_info(name, None).await?;
                                let available_versions = vec![package_info.version];  // Simplified for now

                                if let Ok(safe_version) = security_checker
                                    .find_safe_version(name, &version, &available_versions)
                                    .await
                                {
                                    updates.push((name.clone(), safe_version.to_string()));
                                    fixes_applied = true;
                                }
                            }
                        }
                    }

                    // Apply fixes if requested
                    if fix && fixes_applied {
                        for (name, new_version) in updates {
                            deps.insert(name.clone(), new_version.clone());
                            println!("Updated {} to version {}", name, new_version);
                        }
                        package_json.save().await?;
                        println!("\nUpdated package.json with security fixes");
                    }
                }

                if !vulnerabilities_found {
                    println!("No vulnerabilities found!");
                } else if !fix {
                    println!("\nRun 'rpm audit --fix' to automatically fix these issues");
                }
            }
        }

        Ok(())
    }
}
