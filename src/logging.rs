use anyhow::{Context, Result, bail};
use tracing::info;
pub use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::EnvFilter;

use crate::models::secret_key::TruncatingEventFormat;

pub fn init_tracing() -> Result<WorkerGuard> {
    match std::fs::exists("logs") {
        Ok(false) => std::fs::create_dir_all("logs").context("Failed to create logs directory")?,
        Ok(true) => info!("logs directory already exists"),
        _ => bail!("Failed to check logs directory existence"),
    }

    let appender = rolling::daily("logs", "honeypot.log");
    let (non_blocking, guard) = non_blocking(appender);

    dotenvy::dotenv().ok();

    let env_filter = EnvFilter::new(std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into()));

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .with_ansi(false)
        .event_format(TruncatingEventFormat)
        .init();

    Ok(guard)
}
