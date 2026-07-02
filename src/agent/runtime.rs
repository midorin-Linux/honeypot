use anyhow::{Context, Result};
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestMessageContentPartImage,
        ChatCompletionRequestMessageContentPartText, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageArgs,
        ChatCompletionRequestUserMessageContentPart, CreateChatCompletionRequestArgs, ImageUrl,
        ResponseFormat,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;
use tokio_retry::{Retry, strategy::FixedInterval};
use tracing::debug;

use crate::config::Config;

/// An image attachment to be included in the spam-judgment prompt.
/// Images are embedded as base64 data URLs rather than sent as remote URLs,
/// since Discord CDN links can expire or require authentication.
pub struct ImageAttachment {
    pub data: Vec<u8>,
    pub content_type: String,
}

const PROMPT_FILE: &str = "PROMPT.md";

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
    support_image: bool,
    system_prompt: String,
}

impl AgentRuntime {
    pub fn new(config: Config) -> Result<Self> {
        let model = config.ai.model_id.clone();
        let support_image = config.ai.support_image;

        let system_prompt = std::fs::read_to_string(PROMPT_FILE)
            .with_context(|| format!("failed to read system prompt from {PROMPT_FILE}"))?;

        let openai_config = OpenAIConfig::new()
            .with_api_base(config.ai.base_url)
            .with_api_key(config.ai.api_key.expose())
            .with_header("HTTP-Referer", "https://github.com/midorin-Linux/honeypot")?
            .with_header("X-OpenRouter-Title", "Honeypot")?
            .with_header("X-OpenRouter-Categories", "personal-agent")?;

        let client = Client::with_config(openai_config);

        Ok(Self {
            client,
            model,
            support_image,
            system_prompt,
        })
    }

    pub async fn judge_spam(&self, content: &str, images: &[ImageAttachment]) -> Result<bool> {
        let retry_strategy = FixedInterval::from_millis(RETRY_DELAY_MS).take(MAX_RETRIES);

        Retry::start(retry_strategy, || self.judge_spam_once(content, images)).await
    }

    fn build_user_message(
        &self,
        content: &str,
        images: &[ImageAttachment],
    ) -> Result<ChatCompletionRequestMessage> {
        if !self.support_image || images.is_empty() {
            return Ok(ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessage::from(content),
            ));
        }

        let mut parts: Vec<ChatCompletionRequestUserMessageContentPart> =
            vec![ChatCompletionRequestMessageContentPartText::from(content).into()];

        for image in images {
            let data_url = format!(
                "data:{};base64,{}",
                image.content_type,
                BASE64.encode(&image.data)
            );

            parts.push(
                ChatCompletionRequestMessageContentPartImage::from(ImageUrl {
                    url: data_url,
                    detail: None,
                })
                .into(),
            );
        }

        Ok(ChatCompletionRequestUserMessageArgs::default()
            .content(parts)
            .build()
            .context("failed to build user message with images")?
            .into())
    }

    async fn judge_spam_once(&self, content: &str, images: &[ImageAttachment]) -> Result<bool> {
        let user_message = self.build_user_message(content, images)?;

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(vec![
                ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage::from(
                    self.system_prompt.as_str(),
                )),
                user_message,
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
