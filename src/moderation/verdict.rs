use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Verdict {
    pub is_spam: bool,
    pub reason: String,
}
