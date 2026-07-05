use anyhow::{Context, Result};

const PROMPT_FILE: &str = "PROMPT.md";

pub fn load_system_prompt() -> Result<String> {
    std::fs::read_to_string(PROMPT_FILE)
        .with_context(|| format!("failed to read system prompt from {PROMPT_FILE}"))
}
