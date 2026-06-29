use colored::Colorize;
use rand::seq::IndexedRandom;
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*,
};
use tracing::{error, info, warn};

use crate::agent::runtime::AgentRuntime;

pub struct Handler {
    pub agent_runtime: AgentRuntime,
    pub spinner: indicatif::ProgressBar,
    pub honeypot_channel: u64,
    pub enable_ai_judgment: bool,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        self.spinner.finish_and_clear();
        info!(user = %data_about_bot.user.name, "discord client is ready");
        println!(
            "  {} Discord client ready! Logged in as {}",
            "✓".green(),
            data_about_bot.user.name
        );
    }

    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        if msg.channel_id.get() != self.honeypot_channel {
            return;
        }

        let ban_reason = if !self.enable_ai_judgment {
            "honeypot: AI judgment disabled, all posts in target channel are banned"
        } else if has_invite_link(&msg.content) {
            "honeypot: discord invite link detected"
        } else if msg.mention_everyone || !msg.mention_roles.is_empty() {
            "honeypot: role/everyone mention detected"
        } else if msg.mentions.len() > 1 {
            "honeypot: mass mention detected"
        } else {
            let is_spam = match self.agent_runtime.judge_spam(&msg.content).await {
                Ok(verdict) => verdict,
                Err(err) => {
                    error!(error = %err, "failed to judge message for spam");
                    return;
                }
            };

            if !is_spam {
                return;
            }

            "honeypot: spam detected by LLM"
        };

        let Some(guild_id) = msg.guild_id else {
            warn!("honeypot message had no guild_id; cannot ban");
            return;
        };

        info!(user = %msg.author.name, user_id = %msg.author.id, reason = ban_reason, "banning spammer detected in honeypot channel");

        let reply = salvation_reply(&msg.author.name);
        if let Err(err) = msg.reply(&ctx.http, reply).await {
            warn!(error = %err, "failed to send salvation reply before ban");
        }

        if let Err(err) = guild_id
            .ban_with_reason(&ctx.http, msg.author.id, 1, ban_reason)
            .await
        {
            error!(error = %err, user_id = %msg.author.id, "failed to ban user - check BAN_MEMBERS permission");
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
        .unwrap()
        .replace("{account_name}", account_name)
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
