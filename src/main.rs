#![feature(exit_status_error)]
mod cli;
mod docker;
mod plugin;

use anyhow::Result;
use clap::Parser;
use cli::{Command, CLI};
use flexi_logger::Logger;

#[tokio::main]
async fn main() -> Result<()> {
    Logger::try_with_env_or_str("debug")
        .unwrap()
        .start()
        .unwrap();

    let cli = CLI::parse();

    match &cli.command {
        Command::Plugin(args) => cli::plugin::parse(args).await,
    }
}
