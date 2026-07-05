pub mod commands;
pub mod handler;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use indicatif::ProgressBar;
use serenity::{Client, all::GatewayIntents};
use tracing::info;

use crate::{agent::Agent, config::Config, db::Sqlite, discord::handler::Handler};

pub struct DiscordClient {
    client: Client,
}

impl DiscordClient {
    pub async fn new(config: Config, spinner: ProgressBar, db: Arc<Sqlite>) -> Result<Self> {
        info!("Starting discord client...");

        // AIエージェントの初期化。エラー時のスピナー後処理は呼び出し元(main)に集約する。
        let agent = Agent::new(config.clone()).context("failed to start agent")?;

        // GatewayIntentsの定義
        // ギルドメッセージ・DM・メッセージ本文に加え、`guild_create`受信のためGUILDSを設定する。
        // GUILDSが無いと起動後の新規参加ギルドで`/config`が登録されない。
        let intents = GatewayIntents::GUILDS
            | GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        // Discordクライアントの作成
        let client = Client::builder(config.discord.token.expose(), intents)
            .event_handler(Handler {
                agent,
                config,
                spinner,
                db,
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
