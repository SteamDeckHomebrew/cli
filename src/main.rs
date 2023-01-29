mod cli;
mod plugin;

use anyhow::Result;
use clap::Parser;
use cli::{Command, CLI};
use flexi_logger::Logger;

fn main() -> Result<()> {
    Logger::try_with_env_or_str("debug")?.start()?;

    let cli = CLI::parse();

    match &cli.command {
        Command::Plugin(args) => cli::plugin::parse(args),
    }
}
