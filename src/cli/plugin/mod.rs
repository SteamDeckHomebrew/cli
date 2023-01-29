use std::path::{Path, PathBuf};

use super::{PluginCLI, PluginCommand};
use anyhow::Result;

pub mod build;

pub fn parse(args: &PluginCLI) -> Result<()> {
    match &args.command {
        PluginCommand::Build {
            plugin_path,
            output_path,
        } => build::Builder::new(plugin_path.into(), output_path.into())?.run(),
        PluginCommand::New => todo!(),
        PluginCommand::Deploy => todo!(),
    }
}
