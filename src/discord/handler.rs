use std::collections::HashSet;
use std::io::Cursor;
use std::sync::Mutex;

use colored::Colorize;
use rand::seq::IndexedRandom;
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready, id::UserId},
    prelude::*,
};
use tracing::{error, info, warn};

use crate::{
    agent::runtime::{AgentRuntime, ImageAttachment},
    config::Config,
};

/// LLMが生成するBAN理由の最大文字数。監査ログに埋め込む前にこの長さへ切り詰める。
/// serenityは理由が512文字を超えると`ExceededLimit`を返すため、余裕を持たせた上限にする。
const MAX_BAN_REASON_LEN: usize = 100;

/// 画像をそのままAIへ送るサイズ上限（バイト）。これを超えたら縮小・再圧縮する。
const IMAGE_DOWNSCALE_THRESHOLD_BYTES: u64 = 5 * 1024 * 1024;
/// ダウンロード自体を諦めるサイズ上限（バイト）。巨大ファイルによるメモリ枯渇を防ぐ。
const IMAGE_MAX_DOWNLOAD_BYTES: u64 = 25 * 1024 * 1024;
/// 縮小後の画像の最大辺（ピクセル）。スパム判定に必要な解像度は高くないため小さめにする。
const IMAGE_MAX_DIMENSION: u32 = 1024;

pub struct Handler {
    pub agent_runtime: AgentRuntime,
    pub config: Config,
    pub spinner: indicatif::ProgressBar,
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

        if msg.channel_id.get() != self.config.app.honeypot_channel {
            return;
        }

        // 既にBAN対象として処理済みのユーザーは、連投されても再判定・再BANしない。
        if self.already_handled(msg.author.id) {
            return;
        }

        let Some(ban_reason) = self.determine_ban_reason(&msg).await else {
            return;
        };

        let Some(guild_id) = msg.guild_id else {
            warn!("honeypot message had no guild_id; cannot ban");
            return;
        };

        // BAN実行前にアトミックに登録する。並行して届いた同一ユーザーのメッセージが
        // 同時にここへ到達しても、実際にBANするのは最初の1件だけになる。
        if !self.mark_handled(msg.author.id) {
            return;
        }

        info!(user = %msg.author.name, user_id = %msg.author.id, reason = ban_reason, "banning spammer detected in honeypot channel");

        let reply = salvation_reply(&msg.author.name);
        if let Err(err) = msg.reply(&ctx.http, reply).await {
            warn!(error = %err, "failed to send salvation reply before ban");
        }

        if let Err(err) = guild_id
            .ban_with_reason(
                &ctx.http,
                msg.author.id,
                self.config.app.delete_message_days,
                ban_reason,
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
    }
}

impl Handler {
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

    /// メッセージをBANすべきか判定する。BAN対象ならその理由を、対象外なら`None`を返す。
    /// 設定による即時BAN条件を先に評価し、いずれにも該当しない場合のみAI判定へ進む。
    async fn determine_ban_reason(&self, msg: &Message) -> Option<String> {
        if !self.config.app.enable_ai_judgment {
            return Some(
                "honeypot: AI judgment disabled, all posts in target channel are banned"
                    .to_string(),
            );
        }

        if self.config.app.has_invite_link && has_invite_link(&msg.content) {
            return Some("honeypot: discord invite link detected".to_string());
        }

        if self.config.app.has_role_mention
            && (!msg.mention_roles.is_empty() || msg.mention_everyone)
        {
            return Some("honeypot: role/everyone mention detected".to_string());
        }

        let images = if self.config.ai.support_image {
            download_image_attachments(msg).await
        } else {
            Vec::new()
        };

        match self.agent_runtime.judge_spam(&msg.content, &images).await {
            Ok(verdict) => {
                if verdict.is_spam {
                    Some(format!(
                        "honeypot: spam detected by LLM - {}",
                        truncate_reason(&verdict.reason)
                    ))
                } else {
                    None
                }
            }
            Err(err) => {
                error!(error = %err, "failed to judge message for spam");
                None
            }
        }
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

/// BAN理由文字列を`MAX_BAN_REASON_LEN`文字以内に切り詰める（文字境界を尊重）。
/// 切り詰めた場合は末尾を省略記号にする。
fn truncate_reason(reason: &str) -> String {
    let reason = reason.trim();

    if reason.chars().count() <= MAX_BAN_REASON_LEN {
        return reason.to_string();
    }

    let truncated: String = reason.chars().take(MAX_BAN_REASON_LEN - 1).collect();
    format!("{truncated}…")
}

async fn download_image_attachments(msg: &Message) -> Vec<ImageAttachment> {
    let mut images = Vec::new();

    for attachment in &msg.attachments {
        let Some(content_type) = &attachment.content_type else {
            continue;
        };

        if !content_type.starts_with("image/") {
            continue;
        }

        // ハニーポットには悪意ある投稿が集中するため、巨大ファイルはダウンロード自体を行わない。
        if attachment.size as u64 > IMAGE_MAX_DOWNLOAD_BYTES {
            warn!(
                attachment_id = %attachment.id,
                size = attachment.size,
                "skipping oversized image attachment"
            );
            continue;
        }

        let data = match attachment.download().await {
            Ok(data) => data,
            Err(err) => {
                warn!(
                    error = %err,
                    attachment_id = %attachment.id,
                    "failed to download image attachment"
                );
                continue;
            }
        };

        // 一定サイズを超える画像は、メモリ・AI APIコスト削減のため縮小・再圧縮する。
        // 再圧縮に失敗した場合は元データにフォールバックする。
        let (data, content_type) = if attachment.size as u64 > IMAGE_DOWNSCALE_THRESHOLD_BYTES {
            match downscale_image(&data) {
                Some(reduced) => reduced,
                None => {
                    warn!(
                        attachment_id = %attachment.id,
                        "failed to downscale large image; using original"
                    );
                    (data, content_type.clone())
                }
            }
        } else {
            (data, content_type.clone())
        };

        images.push(ImageAttachment { data, content_type });
    }

    images
}

/// 画像をデコードし、最大辺が`IMAGE_MAX_DIMENSION`を超える場合は縮小してJPEGで再エンコードする。
/// 判定に必要十分な解像度へ落としつつ、ペイロードサイズを抑える。デコード/エンコード失敗時は`None`。
fn downscale_image(data: &[u8]) -> Option<(Vec<u8>, String)> {
    let reader = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?;

    let img = reader.decode().ok()?;

    let img = if img.width() > IMAGE_MAX_DIMENSION || img.height() > IMAGE_MAX_DIMENSION {
        img.resize(
            IMAGE_MAX_DIMENSION,
            IMAGE_MAX_DIMENSION,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };

    let mut buf = Cursor::new(Vec::new());
    // JPEGはアルファチャンネルを扱えないためRGB8へ変換してからエンコードする。
    image::DynamicImage::ImageRgb8(img.to_rgb8())
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .ok()?;

    Some((buf.into_inner(), "image/jpeg".to_string()))
}

fn has_invite_link(content: &str) -> bool {
    const INVITE_DOMAINS: [&str; 3] = [
        "discord.gg/",
        "discord.com/invite/",
        "discordapp.com/invite/",
    ];

    let lower = content.to_lowercase();
    INVITE_DOMAINS.iter().any(|domain| lower.contains(domain))
}
