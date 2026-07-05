pub mod guild_config;
pub mod models;

use std::{sync::Arc, time::Duration};

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

        let guild_config = Arc::new(GuildConfig::new(sqlite_pool.clone()).await);

        Ok(Self { guild_config })
    }

    pub async fn guild_config(&self) -> &Arc<GuildConfig> {
        &self.guild_config
    }
}
