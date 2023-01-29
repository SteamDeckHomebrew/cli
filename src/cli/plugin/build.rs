use anyhow::{Context, Result};
use bollard::{
    container::{AttachContainerOptions, AttachContainerResults, Config},
    image::CreateImageOptions,
    service::HostConfig,
    Docker,
};
use futures::{StreamExt, TryStreamExt};
use log::{debug, info};
use std::{
    io::{stdout, Write},
    path::PathBuf,
};

use crate::plugin::Plugin;

pub struct Builder {
    docker_image: String,
    docker: Docker,

    pub plugin_root: PathBuf,
    pub output_root: PathBuf,
}

impl Builder {
    pub async fn build_frontend(&self) -> Result<()> {
        info!("Building frontend");
        let packagejson_location = self.plugin_root.join("package.json");
        let _packagejson = std::fs::read_to_string(packagejson_location)?;

        let host_config = HostConfig {
            binds: Some(vec![
                format!("{}:{}", self.output_root.to_str().unwrap(), "/out"),
                format!("{}:{}", self.plugin_root.to_str().unwrap(), "/plugin"),
            ]),
            auto_remove: Some(true),
            ..Default::default()
        };

        let builder_config: Config<&str> = Config {
            image: Some(&self.docker_image),
            host_config: Some(host_config),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        debug!("Creating image");
        self.docker
            .create_image(
                Some(CreateImageOptions {
                    from_image: self.docker_image.clone(),
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await?;

        debug!("Creating container");
        let container_id = self
            .docker
            .create_container::<&str, &str>(None, builder_config)
            .await?
            .id;

        debug!("Starting container");
        self.docker
            .start_container::<String>(&container_id, None)
            .await
            .context("Could not start container for building the frontend")?;

        let AttachContainerResults {
            mut output,
            input: _,
        } = self
            .docker
            .attach_container(
                &container_id,
                Some(AttachContainerOptions::<String> {
                    stdout: Some(true),
                    stderr: Some(true),
                    stdin: Some(true),
                    stream: Some(true),
                    ..Default::default()
                }),
            )
            .await?;

        // set stdout in raw mode so we can do tty stuff
        let stdout = stdout();
        let mut stdout = stdout.lock();

        // pipe docker attach output into stdout
        Ok(while let Some(Ok(output)) = output.next().await {
            stdout.write_all(output.into_bytes().as_ref())?;
            stdout.flush()?;
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("Connecting to Docker daemon");
        let _plugin = Plugin::new(self.plugin_root.clone())?;
        self.build_frontend().await?;

        Ok(())
    }

    pub fn new(plugin_root: PathBuf, output_root: PathBuf) -> Result<Self> {
        let docker =
            Docker::connect_with_local_defaults().context("Could not connect to Docker")?;

        if !output_root.exists() {
            std::fs::create_dir(&output_root)?;
        }

        Ok(Self {
            plugin_root: plugin_root.canonicalize()?,
            output_root: output_root.canonicalize()?,
            docker,
            docker_image: "ghcr.io/steamdeckhomebrew/builder:latest".to_owned(),
        })
    }
}
