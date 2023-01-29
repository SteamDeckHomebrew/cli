mod cli;
mod plugin;

use clap::Parser;
use cli::{Command, CLI};
use flexi_logger::Logger;

#[tokio::main]
async fn main() {
    Logger::try_with_env_or_str("debug")
        .unwrap()
        .start()
        .unwrap();

    let cli = CLI::parse();

    match &cli.command {
        Command::Plugin(args) => cli::plugin::parse(args).await.unwrap(),
    }
}
