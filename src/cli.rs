use crate::install::PackageInstaller;
use anyhow::Result;
use clap::{Parser, Subcommand};

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
                // TODO: Implement package removal
                println!("Removing packages: {:?}, global: {}", packages, global);
            }
        }
        Ok(())
    }
}
