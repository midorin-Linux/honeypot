pub mod guild_config;
pub mod models;

use std::{sync::Arc, time::Duration};

use anyhow::Context;
use sqlx::sqlite::SqlitePoolOptions;

use crate::db::guild_config::GuildConfig;

pub struct Sqlite {
    guild_config: Arc<GuildConfig>,
}

impl Sqlite {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let sqlite_pool = SqlitePoolOptions::new()
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await?;

        // マイグレーションはここでのみ実行する。以後の起動でも冪等に適用される。
        sqlx::migrate!("./migrations")
            .run(&sqlite_pool)
            .await
            .context("failed to run database migrations")?;

        let guild_config = Arc::new(GuildConfig::new(sqlite_pool.clone()).await);

        Ok(Self { guild_config })
    }

    pub async fn guild_config(&self) -> &Arc<GuildConfig> {
        &self.guild_config
    }
}
