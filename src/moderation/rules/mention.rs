use super::Message;

pub fn has_role_mention(msg: &Message) -> bool {
    !msg.mention_roles.is_empty() || msg.mention_everyone
}

pub fn has_many_mention(msg: &Message, value: u64) -> bool {
    msg.mentions.len() as u64 >= value
}
