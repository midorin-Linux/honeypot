use std::path::PathBuf;

use anyhow::{Context, Result};
use config::{Config as ConfigBuilder, File};
use serde::Deserialize;
use tracing::{debug, info};

use crate::models::secret_key::SecretKey;

/// 設定ファイルのパス。`Config`と`logging`の軽量読み取りで共有する。
pub const SETTINGS_FILE: &str = "settings.yml";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub env: EnvConfig,
    pub discord: DiscordConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnvConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default = "default_database_url")]
    pub database_url: String,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            database_url: default_database_url(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordConfig {
    pub token: SecretKey,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_database_url() -> String {
    "honeypot.db".to_string()
}

impl Config {
    pub fn load() -> Result<Self> {
        info!("loading configuration file");

        let settings_path = PathBuf::from(SETTINGS_FILE);

        let config = ConfigBuilder::builder()
            .add_source(
                File::from(settings_path)
                    .format(config::FileFormat::Yaml)
                    .required(true),
            )
            .build()
            .context("failed to build config")?;

        debug!("configuration source parsed");

        let parsed: Self = config.try_deserialize()?;

        parsed.validate()?;

        info!("configuration deserialized successfully");

        Ok(parsed)
    }

    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}
