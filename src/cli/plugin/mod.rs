use super::{PluginCLI, PluginCommand};
use anyhow::Result;

pub mod build;

pub async fn parse(args: &PluginCLI) -> Result<()> {
    match &args.command {
        PluginCommand::Build {
            plugin_path,
            output_path,
            tmp_output_path,
            build_as_root,
            output_filename_source,
        } => {
            build::Builder::new(
                plugin_path.into(),
                output_path.into(),
                tmp_output_path.into(),
                *build_as_root,
                output_filename_source.clone(),
            )?
            .run()
            .await
        }
        PluginCommand::New => todo!(),
        PluginCommand::Deploy => todo!(),
    }
}
