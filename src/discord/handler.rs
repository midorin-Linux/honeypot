use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use colored::Colorize;
use rand::seq::IndexedRandom;
use serenity::{
    async_trait,
    model::{
        application::Interaction,
        channel::Message,
        gateway::Ready,
        guild::Guild,
        id::{GuildId, UserId},
    },
    prelude::*,
};
use tracing::{error, info, warn};

use crate::{
    agent::Agent,
    config::Config,
    db::{Sqlite, models::BanTriggerSettings},
    discord::commands,
    moderation::rules::determine_ban_reason,
};

pub struct Handler {
    pub agent: Agent,
    pub config: Config,
    pub spinner: indicatif::ProgressBar,
    pub db: Arc<Sqlite>,
    /// 既にBAN対象として処理したユーザーID。同一スパマーが連投した場合の重複判定・重複BANを防ぐ。
    /// ToDo: 現在はハニーポットが単一チャンネル前提のためプロセス内の単純な集合で足りるが、
    /// 複数チャンネル対応時にはチャンネル単位の管理・レート制限・上限付きキャッシュ等へ変更する。
    pub banned_users: Mutex<HashSet<UserId>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        if !self
            .config
            .app
            .honeypot_channel
            .iter()
            .any(|channel_id| channel_id == &msg.channel_id.get())
        {
            return;
        }

        // 既にBAN対象として処理済みのユーザーは、連投されても再判定・再BANしない。
        if self.already_handled(msg.author.id) {
            return;
        }

        let Some(guild_id) = msg.guild_id else {
            warn!("honeypot message had no guild_id; cannot ban");
            return;
        };

        // ギルド別のBAN判定設定をDBから取得する。行が無ければYAMLデフォルトへフォールバックし、
        // 取得自体に失敗した場合も同様にYAMLデフォルトで判定を継続する(判定不能で処理停止させない)。
        let ban_trigger = match self.db.guild_config().await.get(guild_id.get()).await {
            Ok(Some(settings)) => settings,
            Ok(None) => BanTriggerSettings::from(&self.config.app.ban_trigger),
            Err(err) => {
                warn!(error = %err, guild_id = %guild_id, "failed to load guild ban_trigger settings; falling back to yaml config");
                BanTriggerSettings::from(&self.config.app.ban_trigger)
            }
        };

        let verdict = match determine_ban_reason(&msg, &self.agent, &self.config, &ban_trigger)
            .await
        {
            Ok(verdict) => verdict,
            Err(err) => {
                error!(error = %err, user_id = %msg.author.id, "failed to determine ban verdict; skipping message");
                return;
            }
        };

        // スパム判定されなかった人はここで処理をドロップアウト
        if !verdict.is_spam {
            return;
        };

        // BAN実行前にアトミックに登録する。並行して届いた同一ユーザーのメッセージが
        // 同時にここへ到達しても、実際にBANするのは最初の1件だけになる。
        if !self.mark_handled(msg.author.id) {
            return;
        }

        info!(user = %msg.author.name, user_id = %msg.author.id, reason = verdict.reason, "banning spammer detected in honeypot channel");

        let reply = salvation_reply(&msg.author.name);
        if let Err(err) = msg.reply(&ctx.http, reply).await {
            warn!(error = %err, "failed to send salvation reply before ban");
        }

        if let Err(err) = guild_id
            .ban_with_reason(
                &ctx.http,
                msg.author.id,
                self.config.app.delete_message_days,
                verdict.reason,
            )
            .await
        {
            error!(error = %err, user_id = %msg.author.id, "failed to ban user - check BAN_MEMBERS permission");
        }
    }

    async fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        self.spinner.finish_and_clear();
        info!(user = %data_about_bot.user.name, "discord client is ready");
        println!(
            "  {} Discord client ready! Logged in as {}",
            "✓".green(),
            data_about_bot.user.name
        );
        // スラッシュコマンドの登録は`guild_create`に集約する。`guild_create`は起動時に
        // 参加済みの各ギルドへも発火するため、ここで別途登録すると二重登録になる。
    }

    /// ギルドスコープでスラッシュコマンドを登録する(グローバル登録は反映に最大1時間かかるため不採用)。
    /// `guild_create`は起動時の既参加ギルドと起動後の新規参加ギルドの双方で発火するため、
    /// ここに一本化することで後発の参加でも`/config`が使えるようにする。
    async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: Option<bool>) {
        self.register_guild_commands(&ctx, guild.id).await;
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction
            && command.data.name == "config"
            && let Err(err) = commands::config::handle(&ctx, &command, &self.db, &self.config).await
        {
            error!(error = %err, "failed to handle /config interaction");
        }
    }
}

impl Handler {
    /// 指定ギルドへ`/config`スラッシュコマンドを登録する。失敗しても致命的ではないためログのみ。
    async fn register_guild_commands(&self, ctx: &Context, guild_id: GuildId) {
        match guild_id
            .set_commands(&ctx.http, vec![commands::config::register()])
            .await
        {
            Ok(_) => info!(guild_id = %guild_id, "registered /config command for guild"),
            Err(err) => {
                error!(error = %err, guild_id = %guild_id, "failed to register /config command for guild")
            }
        }
    }

    /// 既にBAN対象として登録済みのユーザーかを確認する（AI判定などの重い処理をスキップするため）。
    fn already_handled(&self, user_id: UserId) -> bool {
        self.banned_users
            .lock()
            .expect("banned_users mutex poisoned")
            .contains(&user_id)
    }

    /// ユーザーをBAN対象として登録する。まだ登録されていなければ`true`（＝このタスクがBANを担当する）、
    /// 既に登録済みなら`false`を返す。挿入と判定をロック内でアトミックに行い、並行処理での二重BANを防ぐ。
    fn mark_handled(&self, user_id: UserId) -> bool {
        self.banned_users
            .lock()
            .expect("banned_users mutex poisoned")
            .insert(user_id)
    }
}

const SALVATION_REPLIES: [&str; 9] = [
    "# 撃ちーかたはじめー！",
    "# やることはシンプルだ！\n# 命令を受け アカウントを消す！",
    "# いいぞ～貴官も救済の一部だ！\n# BANされて来い！ 脱退を許可する！",
    "# 救済だ！",
    "# 貴様に美しさの何が分かる！",
    "# 必要なのだスパムアカウントのBANが！！",
    "# 想像せよ ギルドメンバー諸君！\n# BANで1000万人が救済される！",
    "# 汚しやがって",
    "# 目標はスパムアカウント {account_name}",
];

fn salvation_reply(account_name: &str) -> String {
    SALVATION_REPLIES
        .choose(&mut rand::rng())
        .expect("SALVATION_REPLIES must be non-empty")
        .replace("{account_name}", account_name)
}
