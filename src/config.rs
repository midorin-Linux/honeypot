use std::{fmt as stdfmt, fmt, path::PathBuf};

use anyhow::{Context, Result};
use chrono::Local;
use config::{Config as ConfigBuilder, File};
use secrecy::{ExposeSecret, SecretString, zeroize::Zeroize};
use serde::Deserialize;
use tracing::{
    Event, Subscriber, debug,
    field::{Field, Visit},
    info,
};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
    registry::LookupSpan,
};

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

    pub honeypot_channel: Vec<u64>,

    #[serde(default = "default_delete_message_days")]
    pub delete_message_days: u8,

    pub ban_trigger: BanTriggerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BanTriggerConfig {
    #[serde(default = "default_has_invite_link")]
    pub has_invite_link: bool,

    #[serde(default = "default_has_role_mention")]
    pub has_role_mention: bool,

    #[serde(default = "default_mention_threshold")]
    pub mention_threshold: u64,
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

fn default_mention_threshold() -> u64 {
    3
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

const MAX_FIELD_VALUE_LEN: usize = 100;

/// Debug出力のマスク文字列。長さ・内容とも秘密から独立した固定値。
const REDACTED: &str = "[REDACTED]";

#[derive(Clone)]
pub struct SecretKey(SecretString);

impl SecretKey {
    /// 渡された`String`は秘密を退避したあとゼロ化する。
    /// （呼び出し元が別に保持しているコピーまでは消せない点に注意。）
    pub fn new(mut value: String) -> Self {
        // `From<&str>`はcapacity==lenの新規バッファへ確保するため、
        // `into_boxed_str()`の再確保で旧バッファが未消去のまま残ることがない。
        let secret = SecretString::from(value.as_str());
        value.zeroize();
        Self(secret)
    }

    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl<'de> Deserialize<'de> for SecretKey {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::new(s))
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        // 秘密には一切触れず、内容・長さから独立した固定文字列のみを出力する。
        f.debug_tuple("SecretKey").field(&REDACTED).finish()
    }
}

/// tracingのイベントをフォーマットする際、各フィールド値を`MAX_FIELD_VALUE_LEN`文字で切り詰める。
/// これは長すぎるフィールドがログを圧迫するのを防ぐための上限であって、秘密情報のマスクではない。
/// 秘密情報の保護は`SecretKey`の`Debug`実装（全文マスク）と、平文を露出する`expose()`の
/// 戻り値をログに渡さない運用で担保する。対になる防御としてここに置く。
pub struct TruncatingEventFormat;

impl<S, N> FormatEvent<S, N> for TruncatingEventFormat
where
    S: Subscriber + for<'span> LookupSpan<'span>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> stdfmt::Result {
        write!(
            writer,
            "{} {:<5} {}",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            event.metadata().level(),
            event.metadata().target(),
        )?;

        let mut visitor = TruncatingVisitor::default();
        event.record(&mut visitor);

        for (field, value) in visitor.fields {
            write!(writer, " {}={}", field, value)?;
        }

        writeln!(writer)
    }
}

#[derive(Default)]
struct TruncatingVisitor {
    fields: Vec<(String, String)>,
}

impl Visit for TruncatingVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .push((field.name().to_string(), truncate_value(value)));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn stdfmt::Debug) {
        self.fields.push((
            field.name().to_string(),
            truncate_value(&format!("{value:?}")),
        ));
    }
}

fn truncate_value(value: &str) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(MAX_FIELD_VALUE_LEN).collect();

    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_fully_redacts_any_secret() {
        // 長さにかかわらず、一部の文字も含めて一切露出しない。
        for secret in ["", "a", "abcd", "abcdefghijk", "0123456789ABCDEF"] {
            let debug = format!("{:?}", SecretKey::new(secret.to_string()));
            assert_eq!(
                debug,
                format!("SecretKey({REDACTED:?})"),
                "secret {secret:?} must be fully redacted"
            );
        }
    }

    #[test]
    fn debug_does_not_leak_secret_length() {
        let short = format!("{:?}", SecretKey::new("0123456789ABCDEF".to_string()));
        let long = format!(
            "{:?}",
            SecretKey::new("0123456789ABCDEF0123456789ABCDEF".to_string())
        );
        // どちらも出力は固定文字列なので、表示から元の長さは推測できない。
        assert_eq!(short, long);
    }

    #[test]
    fn expose_returns_plaintext() {
        let key = SecretKey::new("plaintext-token".to_string());
        assert_eq!(key.expose(), "plaintext-token");
    }
}
