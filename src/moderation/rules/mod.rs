pub mod invite_link;
pub mod mention;

use anyhow::Result;
pub use serenity::all::Message;

use crate::{
    agent::Agent,
    config::Config,
    db::models::BanTriggerSettings,
    moderation::{
        pipeline::fallback_agent,
        rules::{
            invite_link::has_invite_link,
            mention::{has_many_mention, has_role_mention},
        },
        verdict::Verdict,
    },
};

pub async fn determine_ban_reason(
    msg: &Message,
    agent: &Agent,
    config: &Config,
    ban_trigger: &BanTriggerSettings,
) -> Result<Verdict> {
    if config.app.unconditional_ban {
        Ok(Verdict {
            is_spam: true,
            reason: "unconditional ban enabled".to_string(),
        })
    } else if ban_trigger.has_invite_link && has_invite_link(msg) {
        Ok(Verdict {
            is_spam: true,
            reason: "discord invite link detected".to_string(),
        })
    } else if ban_trigger.has_role_mention && has_role_mention(msg) {
        Ok(Verdict {
            is_spam: true,
            reason: "role/everyone mention detected".to_string(),
        })
    } else if ban_trigger.mention_threshold != 0
        && has_many_mention(msg, ban_trigger.mention_threshold)
    {
        Ok(Verdict {
            is_spam: true,
            reason: "many mentions detected".to_string(),
        })
    } else if config.app.enable_ai_judgment {
        fallback_agent(msg, &config.ai, agent).await
    } else {
        Ok(Verdict {
            is_spam: false,
            reason: "No spam detected.".to_string(),
        })
    }
}
