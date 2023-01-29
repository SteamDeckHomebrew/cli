use anyhow::{anyhow, Result};
use boolinator::Boolinator;
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
    pub tmp_output_root: PathBuf,
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
                (self.tmp_output_root.to_str().unwrap().into(), "/out".into()),
            ],
        )
        .await
    }

    pub async fn build_backend(&self) -> Result<()> {
        info!("Building backend");
        let mut image_tag = &self.docker_image;

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
            image_tag.into(),
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
                    self.tmp_output_root.join("bin").to_str().unwrap().into(),
                    "/backend/out".into(),
                ),
            ],
        )
        .await
    }

    fn validate_tmp_output_root(tmp_output_root: &PathBuf) -> Result<&PathBuf> {
        Ok(tmp_output_root)
            .and_then(|path| {
                path.is_absolute().as_result(
                    path,
                    anyhow!("For safety reasons, tmp_output_root must be an absolute path"),
                )
            })
            .and_then(|path| {
                path.is_dir()
                    .as_result(path, anyhow!("tmp_output_root must be a directory"))
            })
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Creating temporary build directory");
        std::fs::remove_dir_all(&self.tmp_output_root)?;
        std::fs::create_dir(&self.tmp_output_root)?;

        self.build_backend().await?;
        self.build_frontend().await?;

        Ok(())
    }

    pub fn new(
        plugin_root: PathBuf,
        output_root: PathBuf,
        tmp_output_root: PathBuf,
    ) -> Result<Self> {
        if !output_root.exists() {
            std::fs::create_dir(&output_root)?;
        }

        Builder::validate_tmp_output_root(&tmp_output_root).unwrap();

        Ok(Self {
            plugin: Plugin::new(plugin_root.clone()).expect("Could not create plugin"),
            plugin_root: plugin_root
                .canonicalize()
                .expect("Could not find plugin root"),
            output_root: output_root
                .canonicalize()
                .expect("Could not find output root"),
            tmp_output_root,
            docker_image: "ghcr.io/steamdeckhomebrew/builder:latest".to_owned(),
        })
    }
}
