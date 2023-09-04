use anyhow::{anyhow, Context, Result};
use boolinator::Boolinator;
use glob::glob;
use itertools::Itertools;
use log::{error, info};
use rand::distributions::{Alphanumeric, DistString};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
use std::io::Read;
use sha2::{Sha256, Digest};
use serde_json::Value;
use walkdir::WalkDir;
use zip::{write::FileOptions, ZipWriter};

use crate::{
    cli::FilenameSource,
    docker,
    plugin::{CustomBackend, Plugin},
};

#[derive(Clone)]
pub struct Builder {
    docker_image: String,

    pub plugin: Plugin,
    pub plugin_root: PathBuf,
    pub output_root: PathBuf,
    pub tmp_build_root: PathBuf,
    pub build_as_root: bool,
    pub build_with_dev: bool,
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
            self.build_as_root.clone(),
            self.build_with_dev.clone(),
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
            self.build_as_root.clone(),
            self.build_with_dev.clone(),
        )
        .await
    }

    pub async fn copy_remote_binaries(&self) -> Result<()> {
        let package_json_file = std::fs::read_to_string(self.plugin_root.join("package.json")).expect("Failed to read package.json");
        let json: Value = serde_json::from_str(&package_json_file).expect("Failed to parse package.json");
        let bin_dir = self.tmp_build_root.join("bin");

        let mut any_binaries: bool = false;
        if let Some(remote_binary) = json["remote_binary"].as_array() {
            if !remote_binary.is_empty() {
                any_binaries = true;
                for binary in remote_binary {
                    let url = binary["url"].as_str().expect("Failed to get URL from remote_binary config");
                    let expected_checksum = binary["sha256hash"].as_str().expect("Failed to get sha256hash from remote_binary config");
                    let dest_filename = binary["name"].as_str().expect("Failed to get name from remote_binary config");

                    let response = reqwest::get(url).await.expect("Failed to download remote_binary");
                    let buffer = response.bytes().await.expect("Failed to read remote_binary GET response");

                    let mut hasher = Sha256::new();
                    hasher.update(&buffer);
                    let result = hasher.finalize();
                    let checksum = format!("{:x}", result);

                    if checksum == expected_checksum {
                        info!("Checksums match for file at URL: {}", url);

                        std::fs::create_dir_all(&bin_dir).expect("Failed to create directory");
                        let filepath = bin_dir.join(dest_filename);
                        std::fs::write(&filepath, &buffer).expect("Failed to write file");
                        info!("File saved to: {:?}", filepath);
                    } else {
                        error!("Checksums do not match for file at URL: {}", url);
                        panic!("Bad checksum for file defined in remote_binary")
                    }
                }
            }
        }

        if !any_binaries {
            info!("Plugin does not require any remote binaries");
        }

        Ok(())
    }

    fn zip_path(
        &self,
        filename: &str,
        path: PathBuf,
        zip: &mut ZipWriter<File>,
        perms: FileOptions,
    ) -> Result<()> {
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

            zip.start_file(name.to_str().unwrap(), perms)?;

            zip.write_all(&bytes)?;
        } else if !name.as_os_str().is_empty() {
            zip.add_directory(name.to_str().unwrap(), perms)?;
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
        let zip_filename = format!("{}{}.zip", &filename, if self.build_with_dev { "-dev".to_string() } else { "".to_string() });
        let file = std::fs::File::create(&self.output_root.join(zip_filename))
            .expect("Could not create zip file");
        let mut zip = zip::ZipWriter::new(file);

        /// Directory that needs to be zipped
        struct DirDirective<'a> {
            path: &'a str,
            mandatory: bool,
            permissions: FileOptions,
        }

        let directories = vec![
            DirDirective {
                path: "dist",
                mandatory: true,
                permissions: FileOptions::default(),
            },
            DirDirective {
                path: "bin",
                mandatory: false,
                permissions: FileOptions::default().unix_permissions(0o755),
            },
            DirDirective {
                path: "defaults",
                mandatory: false,
                permissions: FileOptions::default(),
            },
        ];

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
            self.zip_path(&filename, full_path, &mut zip, Default::default())?;
        }

        for directory in directories {
            let full_path = self.tmp_build_root.join(&directory.path);

            if directory.mandatory == false && !full_path.exists() {
                info!(
                    "Optional directory {} not found. Continuing",
                    &directory.path
                );
                continue;
            }

            let dir_entries = WalkDir::new(full_path);

            for entry in dir_entries {
                let file = entry?;
                self.zip_path(
                    &filename,
                    file.path().to_path_buf(),
                    &mut zip,
                    directory.permissions,
                )?;
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
        self.copy_remote_binaries().await.context(
            "Failed to copy remote binaries. There might be more information in the output above."
        )?;
        self.zip_plugin().context("Failed to zip plugin.")?;

        Ok(())
    }

    pub fn new(
        plugin_root: PathBuf,
        output_root: PathBuf,
        tmp_build_root: PathBuf,
        build_as_root: bool,
        build_with_dev: bool,
        output_filename_source: FilenameSource,
    ) -> Result<Self> {
        if !output_root.exists() {
            std::fs::create_dir(&output_root)?;
        }

        docker::ensure_availability()?;

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
            build_with_dev,
            output_filename_source,
        })
    }
}
