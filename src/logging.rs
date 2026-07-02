use anyhow::{Context, Result, bail};
use config::{Config as ConfigBuilder, File};
use tracing::info;
pub use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::EnvFilter;

use crate::{config::SETTINGS_FILE, models::secret_key::TruncatingEventFormat};

/// `Config::load()`より前に呼ばれるため、settings.ymlから`env.log_level`のみを
/// 軽量に読み取る。settings.yml自体が読めない場合は"info"にフォールバックする。
fn read_log_level() -> String {
    ConfigBuilder::builder()
        .add_source(
            File::from(std::path::PathBuf::from(SETTINGS_FILE))
                .format(config::FileFormat::Yaml)
                .required(false),
        )
        .build()
        .ok()
        .and_then(|config| config.get_string("env.log_level").ok())
        .unwrap_or_else(|| "info".to_string())
}

pub fn init_tracing() -> Result<WorkerGuard> {
    match std::fs::exists("logs") {
        Ok(false) => std::fs::create_dir_all("logs").context("Failed to create logs directory")?,
        Ok(true) => info!("logs directory already exists"),
        _ => bail!("Failed to check logs directory existence"),
    }

    let appender = rolling::daily("logs", "honeypot.log");
    let (non_blocking, guard) = non_blocking(appender);

    let env_filter = EnvFilter::new(read_log_level());

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .with_ansi(false)
        .event_format(TruncatingEventFormat)
        .init();

    Ok(guard)
}
