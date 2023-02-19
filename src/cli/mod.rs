pub mod plugin;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct CLI {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Plugin(PluginCLI),
}

#[derive(Parser)]
pub struct PluginCLI {
    #[command(subcommand)]
    command: PluginCommand,
}

#[derive(clap::ValueEnum, Clone)]
pub enum FilenameSource {
    PluginName,
    Directory,
}

#[derive(Subcommand)]
pub enum PluginCommand {
    Build {
        #[arg(default_value = "./")]
        plugin_path: PathBuf,

        #[arg(short, long, default_value = "./out")]
        output_path: PathBuf,

        #[arg(short, long, default_value = "/tmp/decky")]
        tmp_output_path: PathBuf,

        #[arg(short, long, default_value = "false")]
        build_as_root: bool,

        #[arg(short = 's', long, value_enum, default_value = "plugin-name")]
        output_filename_source: FilenameSource,
    },
    New,
    Deploy,
}
