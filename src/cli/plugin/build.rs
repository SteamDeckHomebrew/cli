use anyhow::Result;
use log::info;
use std::path::PathBuf;

use crate::{
    docker,
    plugin::{CustomBackend, Plugin},
};

pub struct Builder {
    docker_image: String,

    pub plugin: Plugin,
    pub plugin_root: PathBuf,
    pub output_root: PathBuf,
}

impl Builder {
    pub async fn build_frontend(&self) -> Result<()> {
        info!("Building frontend");

        docker::run_image(
            self.docker_image.clone(),
            vec![
                (
                    self.plugin_root.canonicalize()?.to_str().unwrap().into(),
                    "/plugin".into(),
                ),
                (self.output_root.to_str().unwrap().into(), "/out".into()),
            ],
        )
        .await
    }

    pub async fn build_backend(&self) -> Result<()> {
        info!("Building backend");
        let mut image_tag: String = "".into();

        match self.plugin.custom_backend {
            CustomBackend::Dockerfile => {
                image_tag = docker::build_image(
                    self.plugin_root.join("backend").join("Dockerfile"),
                    self.plugin.meta.name.to_lowercase().clone(),
                )
                .await?;
            }
            CustomBackend::None => {}
        }

        docker::run_image(
            image_tag,
            vec![
                (
                    self.plugin_root
                        .join("backend")
                        .canonicalize()?
                        .to_str()
                        .unwrap()
                        .into(),
                    "/backend".into(),
                ),
                (
                    self.output_root.join("bin").to_str().unwrap().into(),
                    "/backend/out".into(),
                ),
            ],
        )
        .await
    }

    pub async fn run(&self) -> Result<()> {
        info!("Connecting to Docker daemon");
        self.build_backend().await?;
        self.build_frontend().await?;

        Ok(())
    }

    pub fn new(plugin_root: PathBuf, output_root: PathBuf) -> Result<Self> {
        if !output_root.exists() {
            std::fs::create_dir(&output_root)?;
        }

        Ok(Self {
            plugin: Plugin::new(plugin_root.clone())?,
            plugin_root: plugin_root.canonicalize()?,
            output_root: output_root.canonicalize()?,
            docker_image: "ghcr.io/steamdeckhomebrew/builder:latest".to_owned(),
        })
    }
}
