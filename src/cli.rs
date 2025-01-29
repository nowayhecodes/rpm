use crate::dependency::DependencyResolver;
use crate::install::PackageInstaller;
use crate::package::PackageJson;
use crate::registry::RegistryClient;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Install {
        #[arg(required = true)]
        packages: Vec<String>,

        #[arg(short, long, required = false)]
        global: bool,
    },

    Update {
        #[arg(required = true)]
        packages: Option<Vec<String>>,
    },

    Remove {
        #[arg(required = true)]
        packages: Vec<String>,

        #[arg(short, long, required = false)]
        global: bool,
    },
}

impl Cli {
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Commands::Install { packages, global } => {
                let installer = PackageInstaller::new(global);
                installer.install_packages(&packages).await?;
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
        }
        Ok(())
    }
}
