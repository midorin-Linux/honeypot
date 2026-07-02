use std::path::PathBuf;

use anyhow::{Context, Result, bail};
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
    pub ai: AiConfig,
    pub app: AppConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnvConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordConfig {
    pub token: SecretKey,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiConfig {
    pub api_key: SecretKey,
    pub base_url: String,
    pub model_id: String,

    #[serde(default = "default_support_image")]
    pub support_image: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_enable_ai_judgment")]
    pub enable_ai_judgment: bool,

    #[serde(default = "default_has_invite_link")]
    pub has_invite_link: bool,

    #[serde(default = "default_has_role_mention")]
    pub has_role_mention: bool,

    pub honeypot_channel: u64,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_support_image() -> bool {
    false
}

fn default_enable_ai_judgment() -> bool {
    true
}

fn default_has_invite_link() -> bool {
    true
}

fn default_has_role_mention() -> bool {
    true
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
        if self.discord.token.expose().trim().is_empty() {
            bail!("discord.token must not be empty");
        }

        if self.app.honeypot_channel == 0 {
            bail!("app.honeypot_channel must not be empty");
        }

        if self.app.enable_ai_judgment {
            if self.ai.base_url.trim().is_empty() {
                bail!("ai.base_url must not be empty");
            }

            if self.ai.api_key.expose().trim().is_empty() {
                bail!("ai.api_key must not be empty");
            }

            if self.ai.model_id.trim().is_empty() {
                bail!("ai.model_id must not be empty");
            }
        }

        Ok(())
    }
}
