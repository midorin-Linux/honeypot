use super::Message;

pub fn has_invite_link(msg: &Message) -> bool {
    const INVITE_DOMAINS: [&str; 3] = [
        "discord.gg/",
        "discord.com/invite/",
        "discordapp.com/invite/",
    ];

    let lower = msg.content.to_lowercase();
    INVITE_DOMAINS.iter().any(|domain| lower.contains(domain))
}
