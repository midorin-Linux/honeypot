use std::time::Duration;

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
use backoff::{ExponentialBackoffBuilder, future::retry};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;
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

/// リトライの初期待機時間（ミリ秒）。以降は指数的に増加する。
const RETRY_INITIAL_DELAY_MS: u64 = 1000;
/// リトライを打ち切るまでの総経過時間（秒）。これを超えると最後のエラーを返す。
const RETRY_MAX_ELAPSED_SECS: u64 = 15;

#[derive(Debug, Deserialize)]
pub struct SpamVerdict {
    pub is_spam: bool,
    pub reason: String,
}

pub struct AgentRuntime {
    client: Client<OpenAIConfig>,
    model: String,
    support_image: bool,
    system_prompt: String,
    request_timeout: Duration,
}

impl AgentRuntime {
    pub fn new(config: Config) -> Result<Self> {
        let model = config.ai.model_id.clone();
        let support_image = config.ai.support_image;
        let request_timeout = Duration::from_secs(config.ai.request_timeout_secs);

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
            request_timeout,
        })
    }

    pub async fn judge_spam(
        &self,
        content: &str,
        images: &[ImageAttachment],
    ) -> Result<SpamVerdict> {
        let backoff = ExponentialBackoffBuilder::new()
            .with_initial_interval(Duration::from_millis(RETRY_INITIAL_DELAY_MS))
            .with_max_elapsed_time(Some(Duration::from_secs(RETRY_MAX_ELAPSED_SECS)))
            .build();

        retry(backoff, || async {
            self.judge_spam_once(content, images)
                .await
                .map_err(backoff::Error::transient)
        })
        .await
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

    async fn judge_spam_once(
        &self,
        content: &str,
        images: &[ImageAttachment],
    ) -> Result<SpamVerdict> {
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

        let response =
            tokio::time::timeout(self.request_timeout, self.client.chat().create(request))
                .await
                .context("chat completion request timed out")?
                .context("chat completion request failed")?;

        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .context("chat completion response had no content")?;

        let verdict: SpamVerdict =
            serde_json::from_str(extract_json(content)).with_context(|| {
                format!("failed to parse spam verdict json from response: {content}")
            })?;

        debug!(reason = %verdict.reason, "spam verdict reason");

        Ok(verdict)
    }
}

/// LLM応答からJSONオブジェクト部分を抽出する。
/// `response_format`にJSONモードを指定していても、指示追従性の低いモデルは
/// Markdownコードフェンス(```json ... ```)や前置き・後置きテキストを付けることがある。
/// 最初の`{`から最後の`}`までを切り出すことで、そうした装飾を許容する。
fn extract_json(content: &str) -> &str {
    let trimmed = content.trim();

    match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if end > start => &trimmed[start..=end],
        _ => trimmed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_passes_through_plain_object() {
        let input = r#"{"is_spam": true, "reason": "x"}"#;
        assert_eq!(extract_json(input), input);
    }

    #[test]
    fn extract_json_strips_code_fence_and_prose() {
        let input =
            "Sure, here is the result:\n```json\n{\"is_spam\": false, \"reason\": \"ok\"}\n```";
        let extracted = extract_json(input);
        let verdict: SpamVerdict = serde_json::from_str(extracted).expect("should parse");
        assert!(!verdict.is_spam);
        assert_eq!(verdict.reason, "ok");
    }

    #[test]
    fn extract_json_returns_input_when_no_braces() {
        assert_eq!(extract_json("not json"), "not json");
    }
}
