use anyhow::{Context, Result};
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestUserMessage, CreateChatCompletionRequestArgs, ResponseFormat,
    },
};
use serde::Deserialize;
use tokio_retry::{Retry, strategy::FixedInterval};
use tracing::debug;

use crate::config::Config;

const SYSTEM_PROMPT: &str = r#"
    You are a spam classifier for a honeypot Discord channel that exists only to attract scammers and spammers. Classify the following user message as spam or not.

    Analyze the incoming message based on the following characteristics and trends:

    ### 1. Spam Characteristics
    - **Mass Mentioning (Ghost Mentions):** Messages containing an unusually high number of user or role tags without context.
    - **Repetitive Copy-Paste:** Sending the exact same message or block of text multiple times across channels in a short timeframe.
    - **Unsolicited Self-Promotion / Links:** Messages promoting external links, sketchy websites, or alternative Discord server invites.
    - **Phishing & Scams:** Promises of free Discord Nitro, cryptocurrency schemes, fake giveaways, or urgent requests to click a link to "verify" an account.

    ### 2. Trolling (Raiding / Harassment) Characteristics
    - **Deliberate Provocation (Flamebait):** Messages clearly intended to anger, upset, or provoke emotional reactions.
    - **Character/Text Flooding:** Using walls of text, excessive line breaks, or repeating large blocks of emojis/symbols to disrupt the chat flow.
    - **Bypassing Filters (Leetspeak/Obfuscation):** Attempting to bypass word filters by using intentional misspellings, symbols, spaces, or numbers (e.g., "sc@m", "b4n").
    - **Coordinated Attack Behavior:** Synchronized, off-topic, or hostile messages sent by newly joined accounts (Raiding).

    ### Output Format
    Respond with a JSON object of exactly this shape:

    {"is_spam": boolean, "reason": string}

    Do not include any other text. Only output the JSON object.
    "#;

const MAX_RETRIES: usize = 2;
const RETRY_DELAY_MS: u64 = 1000;

#[derive(Debug, Deserialize)]
struct SpamVerdict {
    is_spam: bool,
    reason: String,
}

pub struct AgentRuntime {
    client: Client<OpenAIConfig>,
    model: String,
}

impl AgentRuntime {
    pub fn new(config: Config) -> Result<Self> {
        let model = config.llm_model.clone();

        let openai_config = OpenAIConfig::new()
            .with_api_base(config.api_base_url)
            .with_api_key(config.api_key.as_ref())
            .with_header("HTTP-Referer", "https://github.com/midorin-Linux/honeypot")?
            .with_header("X-OpenRouter-Title", "Honeypot")?
            .with_header("X-OpenRouter-Categories", "personal-agent")?;

        let client = Client::with_config(openai_config);

        Ok(Self { client, model })
    }

    pub async fn judge_spam(&self, content: &str) -> Result<bool> {
        let retry_strategy = FixedInterval::from_millis(RETRY_DELAY_MS).take(MAX_RETRIES);

        Retry::start(retry_strategy, || self.judge_spam_once(content)).await
    }

    async fn judge_spam_once(&self, content: &str) -> Result<bool> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(vec![
                ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage::from(
                    SYSTEM_PROMPT,
                )),
                ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage::from(content)),
            ])
            .response_format(ResponseFormat::JsonObject)
            .build()
            .context("failed to build chat completion request")?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .context("chat completion request failed")?;

        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .context("chat completion response had no content")?;

        let verdict: SpamVerdict =
            serde_json::from_str(content).context("failed to parse spam verdict json")?;

        debug!(reason = %verdict.reason, "spam verdict reason");

        Ok(verdict.is_spam)
    }
}
