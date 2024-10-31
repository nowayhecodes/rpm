use clap::Parser;
mod cli;
mod error;
mod install;
mod package;
mod registry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = cli::Cli::parse();
    cli.execute().await?;
    Ok(())
}
