pub mod config;
pub mod db;
pub mod models;
pub mod telemetry;

use anyhow::{Error, Result};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::time::sleep;
use tracing::info;

use crate::{config::Config, db::Sqlite, telemetry::init_tracing};

/// 起動時エラーの共通処理。スピナーを片付け、統一フォーマットで標準エラーへ出力し、
/// `?`で伝播できるよう元のエラーをそのまま返す。
fn startup_error(spinner: &ProgressBar, context: &str, err: Error) -> Error {
    spinner.finish_and_clear();
    eprintln!("  {} {}: {}", "✗".red(), context, err);
    err
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("  Honeypot Ver. {}", env!("CARGO_PKG_VERSION"));

    sleep(std::time::Duration::from_secs(1)).await;
    println!();

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("  {spinner} Starting honeypot...")?,
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    // Tracingの初期化
    let _guard = init_tracing()
        .map_err(|err| startup_error(&spinner, "Failed to initialize tracing", err))?;
    info!("Tracing initialized successfully");

    // Configの読み込み
    let config = Config::load()
        .map_err(|err| startup_error(&spinner, "Failed to load configuration", err))?;
    info!("Configuration loaded successfully");

    // SqlitePoolの初期化
    let _sqlite_pool = Sqlite::new(config.env.database_url.clone().as_str())
        .await
        .map_err(|err| startup_error(&spinner, "Failed to initialize sqlite pool", err))?;
    info!("Sqlite pool initialized successfully");

    Ok(())
}
