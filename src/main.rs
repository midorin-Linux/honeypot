pub mod agent;
pub mod config;
pub mod discord;
pub mod logging;
pub mod models;

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::time::sleep;
use tracing::info;

use crate::{discord::client::DiscordClient, logging::init_tracing};

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
    let _guard = match init_tracing() {
        Ok(guard) => guard,
        Err(err) => {
            spinner.finish_and_clear();
            eprintln!("  {} Failed to initialize tracing: {}", "✗".red(), err);
            return Err(err);
        }
    };
    info!("Tracing initialized successfully");

    // Configの読み込み
    let config = match config::Config::load() {
        Ok(config) => config,
        Err(err) => {
            spinner.finish_and_clear();
            eprintln!("  {} Failed to load configuration: {}", "✗".red(), err);
            return Err(err);
        }
    };
    info!("Configuration loaded successfully");

    // Discordクライアントの起動
    let discord_client = match DiscordClient::new(config, spinner).await {
        Ok(discord_client) => discord_client,
        Err(err) => {
            eprintln!("  {} Failed to start discord client: {}", "✗".red(), err);
            return Err(err);
        }
    };
    discord_client.run().await?;

    Ok(())
}
