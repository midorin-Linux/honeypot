use std::collections::HashSet;
use std::sync::Mutex;

use anyhow::{Context, Result};
use indicatif::ProgressBar;
use serenity::{Client, all::GatewayIntents};
use tracing::info;

use crate::{agent::runtime::AgentRuntime, config::Config, discord::handler::Handler};

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

        // AIエージェントの初期化。エラー時のスピナー後処理は呼び出し元(main)に集約する。
        let agent_runtime =
            AgentRuntime::new(config.clone()).context("failed to start agent runtime")?;

        // Discordクライアントの作成
        let client = Client::builder(config.discord.token.expose(), intents)
            .event_handler(Handler {
                agent_runtime,
                config,
                spinner,
                banned_users: Mutex::new(HashSet::new()),
            })
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
