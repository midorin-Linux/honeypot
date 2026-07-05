use anyhow::{Context, Result};

use crate::db::models::BanTriggerSettings;

#[derive(Clone)]
pub struct GuildConfig {
    pub sqlite_pool: sqlx::Pool<sqlx::Sqlite>,
}

impl GuildConfig {
    pub async fn new(sqlite_pool: sqlx::Pool<sqlx::Sqlite>) -> Self {
        Self { sqlite_pool }
    }

    /// ギルド別のBAN判定設定を取得する。行が存在しない場合は`None`(呼び出し元でYAMLデフォルトへフォールバックする)。
    pub async fn get(&self, guild_id: u64) -> Result<Option<BanTriggerSettings>> {
        let row =
            sqlx::query_as::<_, (String,)>("SELECT settings FROM guild_configs WHERE guild_id = ?")
                .bind(guild_id as i64)
                .fetch_optional(&self.sqlite_pool)
                .await
                .context("failed to query guild_configs")?;

        let Some((settings,)) = row else {
            return Ok(None);
        };

        let settings: BanTriggerSettings =
            serde_json::from_str(&settings).context("failed to deserialize guild settings")?;

        Ok(Some(settings))
    }

    /// ギルド別のBAN判定設定を作成・更新する。
    pub async fn upsert(&self, guild_id: u64, settings: &BanTriggerSettings) -> Result<()> {
        let settings_json =
            serde_json::to_string(settings).context("failed to serialize guild settings")?;
        let last_update = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO guild_configs (guild_id, last_update, settings) VALUES (?, ?, ?)
             ON CONFLICT(guild_id) DO UPDATE SET last_update = excluded.last_update, settings = excluded.settings",
        )
        .bind(guild_id as i64)
        .bind(last_update)
        .bind(settings_json)
        .execute(&self.sqlite_pool)
        .await
        .context("failed to upsert guild_configs")?;

        Ok(())
    }

    /// ギルド別のBAN判定設定を削除する。行が無い状態が「デフォルト設定を使用中」を意味する。
    pub async fn reset(&self, guild_id: u64) -> Result<()> {
        sqlx::query("DELETE FROM guild_configs WHERE guild_id = ?")
            .bind(guild_id as i64)
            .execute(&self.sqlite_pool)
            .await
            .context("failed to delete guild_configs row")?;

        Ok(())
    }
}
