use anyhow::{anyhow, Result};
use boolinator::Boolinator;
use log::info;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub enum CustomBackend {
    Dockerfile,
    None,
}

#[derive(Clone)]
pub struct Plugin {
    pub meta: PluginFile,
    #[allow(dead_code)]
    pub root: PathBuf,
    pub custom_backend: CustomBackend,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PluginFile {
    pub name: String,
    pub author: String,

    // TODO: Use a Vec<Flag> enum
    pub flags: Vec<String>,
}


impl Plugin {
    fn find_custom_backend(plugin_root: &Path) -> Result<CustomBackend> {
        let backend_directory = plugin_root.join("backend");

        let has_backend_directory = backend_directory.exists();
        let has_dockerfile = backend_directory.join("Dockerfile").exists();

        match (has_backend_directory, has_dockerfile) {
            (false, _) => Ok(CustomBackend::None),
            (true, true) => Ok(CustomBackend::Dockerfile),
            (true, false) => Err(anyhow!(
                "Backend directory found, but no Dockerfile or entrypoint.sh. If you're using a custom backend, refer to the documentation for information on how to build it. If not, remove the `backend` directory."
            )),
        }
    }

    fn find_frontend(plugin_root: &Path) -> Result<()> {
        info!("Looking for package.json...");
        plugin_root
            .join("package.json")
            .exists()
            .as_result((), anyhow!("Could not find package.json"))
    }

    fn find_pluginfile(plugin_root: &Path) -> Result<PluginFile> {
        info!("Looking for plugin.json...");
        let pluginfile_location = plugin_root.join("plugin.json");

        plugin_root
            .join("plugin.json")
            .exists()
            .as_result(pluginfile_location, anyhow!("Could not find plugin.json"))
            .and_then(|pluginfile| std::fs::read_to_string(pluginfile).map_err(Into::into))
            .and_then(|str| serde_json::from_str::<PluginFile>(&str).map_err(Into::into))
    }

    pub fn new(plugin_root: PathBuf) -> Result<Self> {
        Plugin::find_frontend(&plugin_root)?;

        Ok(Self {
            meta: Plugin::find_pluginfile(&plugin_root)?,
            custom_backend: Plugin::find_custom_backend(&plugin_root)?,
            root: plugin_root.clone(),
        })
    }
}
