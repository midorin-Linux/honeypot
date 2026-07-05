use serde::{Deserialize, Serialize};

/// ギルド別のBAN判定設定。DB層(SQLiteのJSONカラム)専用のモデルであり、
/// YAML設定の`BanTriggerConfig`とは分離する(Serialize追加・Default実装の都合上、
/// `config.rs`の型をそのまま再利用しない)。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BanTriggerSettings {
    pub has_invite_link: bool,
    pub has_role_mention: bool,
    pub mention_threshold: u64,
}

impl Default for BanTriggerSettings {
    fn default() -> Self {
        Self {
            has_invite_link: true,
            has_role_mention: true,
            mention_threshold: 3,
        }
    }
}

impl From<&crate::config::BanTriggerConfig> for BanTriggerSettings {
    fn from(config: &crate::config::BanTriggerConfig) -> Self {
        Self {
            has_invite_link: config.has_invite_link,
            has_role_mention: config.has_role_mention,
            mention_threshold: config.mention_threshold,
        }
    }
}
