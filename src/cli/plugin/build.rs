use anyhow::{anyhow, Context, Result};
use bollard::{container::Config, service::HostConfig, Docker};
use futures::executor::block_on;
use log::info;
use std::{collections::HashMap, path::PathBuf};

use crate::plugin::Plugin;

pub struct Builder {
    docker_image: String,
    docker: Docker,

    pub plugin_root: PathBuf,
    pub output_root: PathBuf,
}

impl Builder {
    pub fn build_frontend(&self) -> Result<()> {
        let packagejson_location = self.plugin_root.join("package.json");
        let packagejson = std::fs::read_to_string(&packagejson_location)?;

        let host_config = HostConfig {
            binds: Some(vec![format!(
                "{}:{}",
                self.output_root.to_str().unwrap(),
                "/out"
            )]),
            auto_remove: Some(true),
            ..Default::default()
        };

        let builder_config: Config<&str> = Config {
            image: Some(&self.docker_image),
            host_config: Some(host_config),
            volumes: Some(
                vec![(self.output_root.to_str().unwrap(), HashMap::new())]
                    .into_iter()
                    .collect(),
            ),
            ..Default::default()
        };

        let container_id = block_on(
            self.docker
                .create_container::<&str, &str>(None, builder_config),
        )?
        .id;

        block_on(self.docker.start_container::<String>(&container_id, None))
            .context("Could not start container for building the frontend")
    }

    pub fn run(&self) -> Result<()> {
        info!("Connecting to Docker daemon");
        let plugin = Plugin::new(self.plugin_root.clone())?;
        self.build_frontend()?;

        Ok(())
    }

    pub fn new(plugin_root: PathBuf, output_root: PathBuf) -> Result<Self> {
        let docker =
            Docker::connect_with_local_defaults().context("Could not connect to Docker")?;

        Ok(Self {
            plugin_root,
            output_root,
            docker,
            docker_image: "".to_owned(),
        })
    }
}
