#[derive(Clone)]
pub struct GuildConfig {
    pub sqlite_pool: sqlx::Pool<sqlx::Sqlite>,
}

impl GuildConfig {
    pub async fn new(sqlite_pool: sqlx::Pool<sqlx::Sqlite>) -> Self {
        Self { sqlite_pool }
    }
}
