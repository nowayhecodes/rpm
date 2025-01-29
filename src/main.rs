use clap::Parser;

mod cli;
mod error;
mod install;
mod package;
mod registry;
mod verification;
mod lockfile;
mod progress;
mod version;
mod dependency;
mod concurrency;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = cli::Cli::try_parse();
    cli.ok().unwrap().execute().await?;
    Ok(())
}
