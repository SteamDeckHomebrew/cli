use std::env::home_dir;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::Result;
use log::info;
use rand::distributions::{Alphanumeric, DistString};

use crate::cli::plugin::build::Builder;
use crate::plugin::DeckFile;
use crate::{cli::FilenameSource, plugin::Plugin};

#[derive(Clone)]
pub struct Deployer {
    builder: Builder,

    pub plugin: Plugin,
    pub plugin_root: PathBuf,
    pub tmp_build_root: PathBuf,
    pub deck_ip: Option<String>,
    pub deck_port: Option<String>,
    pub deck_pass: Option<String>,
    pub deck_key: Option<String>,
    pub deck_dir: Option<String>,
}

impl Deployer {
    pub async fn create_folders(&mut self, deck: DeckFile) -> Result<()> {
        info!("Creating folders");
        Command::new("ssh")
            .args([
                format!("deck@{}", deck.deckip),
                "-p".to_string(),
                format!("{}", deck.deckport),
                format!("{}", if deck.deckkey.contains("-i ") { "-i" } else { "" }),
                format!("{}", if deck.deckkey.contains("-i ") {
                    deck.deckkey
                    .replace("-i ", "")
                    .replace("$HOME", &*home_dir().unwrap().to_string_lossy())
                    .replace("${env:HOME}", &*home_dir().unwrap().to_string_lossy())
                } else {"".to_string()}),
                format!("mkdir -p {deckdir}/homebrew/pluginloader && mkdir -p {deckdir}/homebrew/plugins", deckdir = deck.deckdir),
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .expect("Unable to create folders");
        Ok(())
    }

    pub async fn chmod_folders(&mut self, deck: DeckFile) -> Result<()> {
        info!("Chmod folders");
        Command::new("ssh")
            .args([
                format!("deck@{}", deck.deckip),
                "-p".to_string(),
                format!("{}", deck.deckport),
                format!(
                    "{}",
                    if deck.deckkey.contains("-i ") {
                        "-i"
                    } else {
                        ""
                    }
                ),
                format!(
                    "{}",
                    if deck.deckkey.contains("-i ") {
                        deck.deckkey
                            .replace("-i ", "")
                            .replace("$HOME", &*home_dir().unwrap().to_string_lossy())
                            .replace("${env:HOME}", &*home_dir().unwrap().to_string_lossy())
                    } else {
                        "".to_string()
                    }
                ),
                format!(
                    "echo '{}' | sudo -S chmod -R ug+rw {}/homebrew/",
                    deck.deckpass, deck.deckdir
                ),
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .expect("Unable to chmod folders");
        Ok(())
    }

    pub async fn deploy_plugin(&mut self, deck: DeckFile, filename: String) -> Result<()> {
        info!("Deploying plugin");

        Command::new("rsync")
            .args([
                "-azp".to_string(),
                "--delete".to_string(),
                "--chmod=D0755,F0755".to_string(),
                // format!("--rsh='ssh -p {} {}'", deck.deckport, deck.deckkey.replace("$HOME", &*home_dir().unwrap().to_string_lossy())),
                self.tmp_build_root
                    .join(filename)
                    .to_string_lossy()
                    .to_string(),
                format!("deck@{}:{}/homebrew/plugins", deck.deckip, deck.deckdir),
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .expect("Unable to rsync");

        Ok(())
    }

    pub async fn restart_decky(&mut self, deck: DeckFile) -> Result<()> {
        info!("Restarting decky");
        Command::new("ssh")
            .args([
                format!("deck@{}", deck.deckip),
                "-p".to_string(),
                format!("{}", deck.deckport),
                format!(
                    "{}",
                    if deck.deckkey.contains("-i ") {
                        "-i"
                    } else {
                        ""
                    }
                ),
                format!(
                    "{}",
                    if deck.deckkey.contains("-i ") {
                        deck.deckkey
                            .replace("-i ", "")
                            .replace("$HOME", &*home_dir().unwrap().to_string_lossy())
                            .replace("${env:HOME}", &*home_dir().unwrap().to_string_lossy())
                    } else {
                        "".to_string()
                    }
                ),
                format!(
                    "echo '{}' | sudo -S systemctl restart plugin_loader.service",
                    deck.deckpass
                ),
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .expect("Unable to restart decky");
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut deck: DeckFile;
        if self.deck_ip.is_some()
            && self.deck_port.is_some()
            && self.deck_pass.is_some()
            && self.deck_key.is_some()
            && self.deck_dir.is_some()
        {
            deck = DeckFile {
                deckip: self.deck_ip.clone().unwrap(),
                deckport: self.deck_port.clone().unwrap(),
                deckpass: self.deck_pass.clone().unwrap(),
                deckkey: self.deck_key.clone().unwrap(),
                deckdir: self.deck_dir.clone().unwrap(),
            };
        } else {
            deck = self.plugin.deck.clone();
            if self.deck_ip.is_some() {
                deck.deckip = self.deck_ip.clone().unwrap();
            }
            if self.deck_port.is_some() {
                deck.deckport = self.deck_port.clone().unwrap();
            }
            if self.deck_pass.is_some() {
                deck.deckpass = self.deck_pass.clone().unwrap();
            }
            if self.deck_key.is_some() {
                deck.deckkey = self.deck_key.clone().unwrap();
            }
            if self.deck_dir.is_some() {
                deck.deckdir = self.deck_dir.clone().unwrap();
            }
        }

        self.builder.run().await.unwrap();

        std::fs::remove_dir_all(&self.tmp_build_root).ok();
        std::fs::create_dir_all(&self.tmp_build_root).ok();

        let filename: String = match &self.builder.output_filename_source {
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
            if self.builder.build_with_dev {
                "-dev".to_string()
            } else {
                "".to_string()
            }
        );
        let file = std::fs::File::open(&self.builder.output_root.join(zip_filename))
            .expect("Could not open zip file");
        let mut zip = zip::ZipArchive::new(file).unwrap();
        zip.extract(&self.tmp_build_root).unwrap();

        self.create_folders(deck.clone()).await?;

        self.chmod_folders(deck.clone()).await?;

        self.deploy_plugin(deck.clone(), filename).await?;

        self.chmod_folders(deck.clone()).await?;

        self.restart_decky(deck.clone()).await?;

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
        deck_ip: Option<String>,
        deck_port: Option<String>,
        deck_pass: Option<String>,
        deck_key: Option<String>,
        deck_dir: Option<String>,
    ) -> Result<Self> {
        let output_random_padding: String = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);

        let builder: Builder = Builder::new(
            plugin_root.clone(),
            output_root,
            tmp_build_root.clone(),
            build_as_root,
            build_with_dev,
            follow_symlinks,
            output_filename_source,
        )
        .expect("Could not create builder");

        Ok(Self {
            builder: builder.clone(),
            plugin: builder.plugin.clone(),
            plugin_root,
            tmp_build_root: tmp_build_root.join(output_random_padding),
            deck_ip,
            deck_port,
            deck_pass,
            deck_key,
            deck_dir,
        })
    }
}
