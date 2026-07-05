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
    pub env: EnvConfig,
    pub discord: DiscordConfig,
    pub ai: AiConfig,
    pub app: AppConfig,
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

#[derive(Debug, Clone, Deserialize)]
pub struct AiConfig {
    pub api_key: SecretKey,
    pub base_url: String,
    pub model_id: String,

    #[serde(default = "default_support_image")]
    pub support_image: bool,

    /// AIプロバイダへのリクエストタイムアウト（秒）。ハング時に判定が無期限ブロックするのを防ぐ。
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_enable_ai_judgment")]
    pub enable_ai_judgment: bool,

    #[serde(default = "default_has_invite_link")]
    pub has_invite_link: bool,

    #[serde(default = "default_has_role_mention")]
    pub has_role_mention: bool,

    /// BAN時にさかのぼって削除するメッセージの日数（Discord仕様で0〜7）。
    #[serde(default = "default_delete_message_days")]
    pub delete_message_days: u8,

    pub honeypot_channel: Vec<u64>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_database_url() -> String {
    "honeypot.db".to_string()
}

fn default_support_image() -> bool {
    false
}

fn default_request_timeout_secs() -> u64 {
    300
}

fn default_delete_message_days() -> u8 {
    1
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
        Ok(())
    }
}
