use anyhow::{Context, Result};
use colored::Colorize;
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
        // ToDo: 現在は個人プロジェクトのためすべての権限を設定しているが、権限を絞る
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        let honeypot_channel = config.honeypot_channel;

        // AIエージェントの初期化
        let agent_runtime = match AgentRuntime::new(config.clone()) {
            Ok(agent_runtime) => agent_runtime,
            Err(err) => {
                spinner.finish_and_clear();
                eprintln!("  {} Failed to start agent runtime: {}", "✗".red(), err);
                return Err(err);
            }
        };

        // Discordクライアントの作成
        let client = Client::builder(config.discord_token.as_ref(), intents)
            .event_handler(Handler {
                agent_runtime,
                spinner,
                honeypot_channel,
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
