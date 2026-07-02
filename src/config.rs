use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use config::{Config as ConfigBuilder, File};
use serde::Deserialize;
use tracing::{debug, info};

use crate::models::secret_key::SecretKey;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    // .env（機密情報）
    pub discord_token: SecretKey,
    pub api_key: SecretKey,
    // settings.yml（アプリケーション設定）
    pub honeypot_channel: u64,
    pub api_base_url: String,
    pub llm_model: String,
    #[serde(default = "default_enable_ai_judgment")]
    pub enable_ai_judgment: bool,
}

fn default_enable_ai_judgment() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            discord_token: SecretKey::new("".to_string()),
            api_key: SecretKey::new("".to_string()),
            honeypot_channel: 0,
            api_base_url: "".to_string(),
            llm_model: "".to_string(),
            enable_ai_judgment: true,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        info!("loading configuration files");

        let settings_path = PathBuf::from("settings.yml");
        let env_path = PathBuf::from(".env");

        let config = ConfigBuilder::builder()
            .add_source(
                File::from(settings_path)
                    .format(config::FileFormat::Yaml)
                    .required(true),
            )
            .add_source(
                File::from(env_path)
                    .format(config::FileFormat::Ini)
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
        if self.discord_token.expose().trim().is_empty() {
            bail!("discord_token must not be empty");
        }

        if self.honeypot_channel == 0 {
            bail!("honeypot_channel must not be empty");
        }

        if self.enable_ai_judgment {
            if self.api_base_url.trim().is_empty() {
                bail!("api_base_url must not be empty");
            }

            if self.api_key.expose().trim().is_empty() {
                bail!("api_key must not be empty");
            }

            if self.llm_model.trim().is_empty() {
                bail!("llm_model must not be empty");
            }
        }

        Ok(())
    }
}
