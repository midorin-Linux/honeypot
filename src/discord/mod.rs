pub mod handler;

use anyhow::{Context, Result};
use indicatif::ProgressBar;
use serenity::{Client, all::GatewayIntents};
use tracing::info;

use crate::{config::Config, discord::handler::Handler};

pub struct DiscordClient {
    client: Client,
}

impl DiscordClient {
    pub async fn new(config: Config, spinner: ProgressBar) -> Result<Self> {
        info!("Starting discord client...");

        // GatewayIntentsの定義
        // ギルドメッセージ・DM・メッセージ本文の3つに絞って設定している。
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        // Discordクライアントの作成
        let client = Client::builder(config.discord.token.expose(), intents)
            .event_handler(Handler { config, spinner })
            .await
            .context("failed to create discord client")?;

        Ok(Self { client })
    }

    pub async fn run(mut self) -> Result<()> {
        info!("starting discord event loop");
        self.client
            .start()
            .await
            .context("Failed to start Discord client")?;

        info!("discord event loop finished");

        Ok(())
    }
}
