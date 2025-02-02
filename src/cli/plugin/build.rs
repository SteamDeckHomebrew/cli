use anyhow::{anyhow, Context, Result};
use boolinator::Boolinator;
use glob::glob;
use itertools::Itertools;
use log::{error, info};
use rand::distributions::{Alphanumeric, DistString};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    fs::File,
    io::Write,
    os,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
use zip::{write::FileOptions, CompressionMethod, ZipWriter};

use crate::{
    cli::{CompressMethod, ContainerEngine, FilenameSource},
    container_engine,
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
    pub follow_symlinks: bool,
    pub output_filename_source: FilenameSource,
    pub container_engine: ContainerEngine,
    pub compression_method: CompressMethod,
    pub compression_level: Option<i32>,
}

impl Builder {
    pub async fn build_frontend(&self) -> Result<()> {
        info!("Building frontend");

        container_engine::run_image(
            &self.container_engine,
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
                image_tag = container_engine::build_image(
                    &self.container_engine,
                    self.plugin_root.join("backend").join("Dockerfile"),
                    self.plugin.meta.name.to_ascii_lowercase().replace(" ", "-"),
                )
                .await?
                .clone();
            }
            CustomBackend::None => {}
        }

        container_engine::run_image(
            &self.container_engine,
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
                (
                    self.plugin_root.canonicalize()?.to_str().unwrap().into(),
                    "/plugin".into(),
                ),
            ],
            self.build_as_root.clone(),
            self.build_with_dev.clone(),
        )
        .await
    }

    pub async fn copy_remote_binaries(&self) -> Result<()> {
        let package_json_file = std::fs::read_to_string(self.plugin_root.join("package.json"))
            .expect("Failed to read package.json");
        let json: Value =
            serde_json::from_str(&package_json_file).expect("Failed to parse package.json");
        let bin_dir = self.tmp_build_root.join("bin");

        let mut any_binaries: bool = false;
        let mut remote_binary_bundling: bool = false;
        if json["remote_binary_bundling"].as_bool().is_some() {
            remote_binary_bundling = true;
        }
        if let Some(remote_binary) = json["remote_binary"].as_array() {
            if !remote_binary.is_empty() {
                any_binaries = true;
                if remote_binary_bundling {
                    for binary in remote_binary {
                        let url = binary["url"]
                            .as_str()
                            .expect("Failed to get URL from remote_binary config");
                        let expected_checksum = binary["sha256hash"]
                            .as_str()
                            .expect("Failed to get sha256hash from remote_binary config");
                        let dest_filename = binary["name"]
                            .as_str()
                            .expect("Failed to get name from remote_binary config");

                        let response = reqwest::get(url)
                            .await
                            .expect("Failed to download remote_binary");
                        let buffer = response
                            .bytes()
                            .await
                            .expect("Failed to read remote_binary GET response");

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
        }

        if !remote_binary_bundling {
            info!("Plugin does not want to bundle binaries during build");
        }

        if !any_binaries {
            info!("Plugin does not require any remote binaries");
        }

        Ok(())
    }

    pub async fn build_py_modules(&self) -> Result<()> {
        let source_py_modules_dir = self.plugin_root.join("py_modules");
        let tmp_py_modules_dir = self.tmp_build_root.join("py_modules");

        if !&source_py_modules_dir.exists() {
            info!("Plugin does not have a py_modules");
            return Ok(());
        }

        info!("Building py_modules");

        self.copy_py_modules(source_py_modules_dir, tmp_py_modules_dir)?;

        Ok(())
    }

    fn copy_py_modules(&self, src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
        fs::create_dir_all(&dst)?;

        let src = src.as_ref();
        let dst = dst.as_ref();

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let file_type = entry.file_type()?;

            let to = dst.join(entry.file_name());

            if file_type.is_symlink() && self.follow_symlinks {
                let original = src.join(fs::read_link(entry.path())?);
                let original_fullpath = original.canonicalize()?;

                os::unix::fs::symlink(original_fullpath, to)?;
            } else if file_type.is_dir() {
                if entry.file_name() == "__pycache__" {
                    continue;
                }

                self.copy_py_modules(entry.path(), to)?;
            } else if file_type.is_file() {
                fs::copy(entry.path(), to)?;
            }
        }

        Ok(())
    }

    fn zip_path(
        &self,
        filename: &str,
        path: PathBuf,
        zip: &mut ZipWriter<File>,
        opts: FileOptions,
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

            let method = match self.compression_method {
                CompressMethod::Deflate => CompressionMethod::Deflated,
                CompressMethod::Store => CompressionMethod::Stored,
            };

            let mut opts = opts.compression_method(method);

            if method == CompressionMethod::Deflated {
                opts = match self.compression_level {
                    Some(level) => opts.compression_level(Some(level)),
                    None => opts.compression_level(Some(9))
                }
            }

            zip.start_file(name.to_str().unwrap(), opts)?;

            zip.write_all(&bytes)?;
        } else if !name.as_os_str().is_empty() {
            zip.add_directory(name.to_str().unwrap(), opts)?;
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
        let zip_filename = format!(
            "{}{}.zip",
            &filename,
            if self.build_with_dev {
                "-dev".to_string()
            } else {
                "".to_string()
            }
        );
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
            DirDirective {
                path: "py_modules",
                mandatory: false,
                permissions: FileOptions::default().unix_permissions(0o755),
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

            let dir_entries = WalkDir::new(full_path).follow_links(self.follow_symlinks);
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
            "Failed to copy remote binaries. There might be more information in the output above.",
        )?;
        self.build_py_modules().await.context(
            "Failed to build py_modules. There might be more information in the output above.",
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
        follow_symlinks: bool,
        output_filename_source: FilenameSource,
        container_engine: ContainerEngine,
        compression_method: CompressMethod,
        compression_level: Option<i32>,
    ) -> Result<Self> {
        if !output_root.exists() {
            std::fs::create_dir(&output_root)?;
        }

        container_engine::ensure_availability(&container_engine)?;

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
            follow_symlinks,
            output_filename_source,
            container_engine,
            compression_method,
            compression_level,
        })
    }
}
