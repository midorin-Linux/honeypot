use std::io::Cursor;

use anyhow::Result;
use serenity::all::Message;
use tracing::{error, warn};

use crate::{
    agent::{Agent, ImageAttachment},
    config::AiConfig,
    moderation::verdict::Verdict,
};

/// LLMが生成するBAN理由の最大文字数。監査ログに埋め込む前にこの長さへ切り詰める。
/// serenityは理由が512文字を超えると`ExceededLimit`を返すため、余裕を持たせた上限にする。
pub const MAX_BAN_REASON_LEN: usize = 100;

/// 画像をそのままAIへ送るサイズ上限（バイト）。これを超えたら縮小・再圧縮する。
const IMAGE_DOWNSCALE_THRESHOLD_BYTES: u64 = 5 * 1024 * 1024;
/// ダウンロード自体を諦めるサイズ上限（バイト）。巨大ファイルによるメモリ枯渇を防ぐ。
const IMAGE_MAX_DOWNLOAD_BYTES: u64 = 25 * 1024 * 1024;
/// 縮小後の画像の最大辺（ピクセル）。スパム判定に必要な解像度は高くないため小さめにする。
const IMAGE_MAX_DIMENSION: u32 = 1024;

pub async fn fallback_agent(msg: &Message, ai_config: &AiConfig, agent: &Agent) -> Result<Verdict> {
    let images = if ai_config.support_image {
        download_image_attachments(msg).await
    } else {
        Vec::new()
    };

    match agent.judge_spam(&msg.content, &images).await {
        Ok(verdict) => {
            if verdict.is_spam {
                let format_reason = format!(
                    "honeypot: spam detected by LLM - {}",
                    truncate_reason(&verdict.reason)
                );

                Ok(Verdict {
                    is_spam: true,
                    reason: format_reason,
                })
            } else {
                let format_reason = format!(
                    "honeypot: spam doesn't detect by LLM - {}",
                    truncate_reason(&verdict.reason)
                );

                Ok(Verdict {
                    is_spam: false,
                    reason: format_reason,
                })
            }
        }
        Err(err) => {
            error!(error = %err, "failed to judge message for spam");
            Err(err)
        }
    }
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
