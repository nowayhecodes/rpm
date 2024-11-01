use crate::install::PackageInstaller;
use crate::package::PackageJson;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tokio::fs;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Install {
        #[arg(required = true)]
        packages: Vec<String>,

        #[arg(short, long)]
        global: bool,
    },

    Remove {
        #[arg(required = true)]
        packages: Vec<String>,

        #[arg(short, long)]
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
                        
                        // Update package.json if it exists and we're in local mode
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
