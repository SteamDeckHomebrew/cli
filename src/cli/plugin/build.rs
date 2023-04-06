use anyhow::{anyhow, Context, Result};
use boolinator::Boolinator;
use glob::glob;
use itertools::Itertools;
use log::info;
use rand::distributions::{Alphanumeric, DistString};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
use zip::{write::FileOptions, ZipWriter};

use crate::{
    cli::FilenameSource,
    docker,
    plugin::{CustomBackend, Plugin},
};

pub struct Builder {
    docker_image: String,

    pub plugin: Plugin,
    pub plugin_root: PathBuf,
    pub output_root: PathBuf,
    pub tmp_build_root: PathBuf,
    pub build_as_root: bool,
    pub output_filename_source: FilenameSource,
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
                (self.tmp_build_root.to_str().unwrap().into(), "/out".into()),
            ],
            self.build_as_root,
        )
        .await
    }

    pub async fn build_backend(&self) -> Result<()> {
        if !&self.plugin_root.join("backend").exists() {
            info!("Plugin does not have a custom backend");
            return Ok(());
        }

        info!("Building backend");
        let mut image_tag: String = self.docker_image.clone();

        match self.plugin.custom_backend {
            CustomBackend::Dockerfile => {
                image_tag = docker::build_image(
                    self.plugin_root.join("backend").join("Dockerfile"),
                    self.plugin.meta.name.to_ascii_lowercase().replace(" ", "-"),
                )
                .await?
                .clone();
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
                    self.tmp_build_root.join("bin").to_str().unwrap().into(),
                    "/backend/out".into(),
                ),
            ],
            self.build_as_root,
        )
        .await
    }

    fn zip_path(&self, filename: &str, path: PathBuf, zip: &mut ZipWriter<File>) -> Result<()> {
        let name = path
            .strip_prefix(&self.tmp_build_root)
            .map(|name| name.to_path_buf())
            .and_then(|name| {
                name.strip_prefix("defaults")
                    .map(|path| path.to_path_buf())
                    .or(Ok(name))
            })
            .map(|name| Path::new(filename).join(name))?;

        info!("Zipping {:?}", name);

        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();

            zip.start_file(name.to_str().unwrap(), FileOptions::default())?;

            zip.write_all(&bytes)?;
        } else if !name.as_os_str().is_empty() {
            zip.add_directory(name.to_str().unwrap(), FileOptions::default())?;
        }

        Ok(())
    }

    pub fn zip_plugin(&self) -> Result<()> {
        info!("Zipping plugin");
        let filename: String = match &self.output_filename_source {
            FilenameSource::PluginName => self.plugin.meta.name.clone(),
            FilenameSource::Directory => self
                .plugin_root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        };
        let zip_filename = format!("{}.zip", &filename);
        let file = std::fs::File::create(&self.output_root.join(zip_filename))
            .expect("Could not create zip file");
        let mut zip = zip::ZipWriter::new(file);

        let directories = vec![("dist", true), ("bin", false), ("defaults", false)];
        let expected_files = vec![
            "LICENSE",
            "main.py",
            "package.json",
            "plugin.json",
            "README.md",
        ]
        .into_iter()
        .map(|f| f.to_string());

        let python_files = glob(&format!("{}/*.py", self.tmp_build_root.to_string_lossy()))
            .unwrap()
            .map(|f| {
                f.unwrap()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .into_iter();

        let files = expected_files.chain(python_files).unique();

        for file in files {
            let full_path = self.tmp_build_root.join(&file);
            self.zip_path(&filename, full_path, &mut zip)?;
        }

        for directory in directories {
            let full_path = self.tmp_build_root.join(&directory.0);

            if directory.1 == false && !full_path.exists() {
                info!("Optional directory {} not found. Continuing", &directory.0);
                continue;
            }

            let dir_entries = WalkDir::new(full_path);

            for entry in dir_entries {
                let file = entry?;
                self.zip_path(&filename, file.path().to_path_buf(), &mut zip)?;
            }
        }

        zip.finish()?;

        Ok(())
    }

    fn validate_tmp_build_root(tmp_build_root: &PathBuf) -> Result<&PathBuf> {
        Ok(tmp_build_root).and_then(|path| {
            path.is_absolute().as_result(
                path,
                anyhow!("For safety reasons, tmp_build_root must be an absolute path"),
            )
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Creating temporary build directory");
        std::fs::remove_dir_all(&self.tmp_build_root).ok();
        std::fs::create_dir_all(&self.tmp_build_root)
            .context("Temporary build directory already exists")?;

        info!("Building plugin");
        self.build_backend().await.context(
            "Failed to build backend. There might be more information in the output above.",
        )?;
        self.build_frontend().await.context(
            "Failed to build frontend. There might be more information in the output above.",
        )?;
        self.zip_plugin().context("Failed to zip plugin.")?;

        Ok(())
    }

    pub fn new(
        plugin_root: PathBuf,
        output_root: PathBuf,
        tmp_build_root: PathBuf,
        build_as_root: bool,
        output_filename_source: FilenameSource,
    ) -> Result<Self> {
        if !output_root.exists() {
            std::fs::create_dir(&output_root)?;
        }

        Builder::validate_tmp_build_root(&tmp_build_root).unwrap();

        let output_random_padding: String = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);

        Ok(Self {
            plugin: Plugin::new(plugin_root.clone()).expect("Could not create plugin"),
            plugin_root: plugin_root
                .canonicalize()
                .expect("Could not find plugin root"),
            output_root: output_root
                .canonicalize()
                .expect("Could not find output root"),
            tmp_build_root: tmp_build_root.join(output_random_padding),
            docker_image: "ghcr.io/steamdeckhomebrew/builder:latest".to_owned(),
            build_as_root,
            output_filename_source,
        })
    }
}
