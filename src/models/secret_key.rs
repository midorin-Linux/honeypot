use std::{fmt as stdfmt, fmt};

use chrono::Local;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
    registry::LookupSpan,
};

const MAX_FIELD_VALUE_LEN: usize = 100;

/// Debug出力で末尾の一部を可視化する文字数。
const VISIBLE_SUFFIX_LEN: usize = 4;
/// 末尾の一部を可視化してよい最小の長さ。これ未満の秘密は全マスクする。
const MIN_LEN_FOR_SUFFIX: usize = 12;
/// マスク文字列の表示幅。元の長さを推測されないよう固定にする。
const MASK_WIDTH: usize = 20;

#[derive(Clone)]
pub struct SecretKey(SecretString);

impl SecretKey {
    pub fn new(value: String) -> Self {
        Self(SecretString::new(value.into()))
    }

    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl<'de> Deserialize<'de> for SecretKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::new(s))
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let inner = self.0.expose_secret();
        let length = inner.chars().count();

        // 十分に長い秘密のときだけ末尾数文字を残し、短い・空の秘密は全マスクする。
        let masked = if length >= MIN_LEN_FOR_SUFFIX {
            let suffix: String = inner.chars().skip(length - VISIBLE_SUFFIX_LEN).collect();
            format!("{suffix:*>MASK_WIDTH$}")
        } else {
            "*".repeat(MASK_WIDTH)
        };

        f.debug_tuple("SecretKey").field(&masked).finish()
    }
}

/// tracingのイベントをフォーマットする際、各フィールド値を`MAX_FIELD_VALUE_LEN`文字で切り詰める。
/// これは長すぎるフィールドがログを圧迫するのを防ぐための上限であって、秘密情報のマスクではない。
/// 秘密情報の保護は`SecretKey`の`Debug`実装（末尾以外をマスク）と、平文を露出する`expose()`の
/// 戻り値をログに渡さない運用で担保する。対になる防御としてここに置く。
pub struct TruncatingEventFormat;

impl<S, N> FormatEvent<S, N> for TruncatingEventFormat
where
    S: Subscriber + for<'span> LookupSpan<'span>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> stdfmt::Result {
        write!(
            writer,
            "{} {:<5} {}",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            event.metadata().level(),
            event.metadata().target(),
        )?;

        let mut visitor = TruncatingVisitor::default();
        event.record(&mut visitor);

        for (field, value) in visitor.fields {
            write!(writer, " {}={}", field, value)?;
        }

        writeln!(writer)
    }
}

#[derive(Default)]
struct TruncatingVisitor {
    fields: Vec<(String, String)>,
}

impl Visit for TruncatingVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .push((field.name().to_string(), truncate_value(value)));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn stdfmt::Debug) {
        self.fields.push((
            field.name().to_string(),
            truncate_value(&format!("{value:?}")),
        ));
    }
}

fn truncate_value(value: &str) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(MAX_FIELD_VALUE_LEN).collect();

    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_masks_short_secret_entirely() {
        // MIN_LEN_FOR_SUFFIX未満は末尾も含めて一切露出しない。
        for secret in ["", "a", "abcd", "abcdefghijk"] {
            let debug = format!("{:?}", SecretKey::new(secret.to_string()));
            let expected = format!("SecretKey(\"{}\")", "*".repeat(MASK_WIDTH));
            assert_eq!(debug, expected, "secret {secret:?} must be fully masked");
        }
    }

    #[test]
    fn debug_reveals_only_suffix_for_long_secret() {
        let secret = "0123456789ABCDEF"; // 16文字
        let debug = format!("{:?}", SecretKey::new(secret.to_string()));
        // 末尾4文字("CDEF")のみ残し、幅20まで`*`で左詰めする。
        assert_eq!(debug, "SecretKey(\"****************CDEF\")");
        // 末尾以外は含まれない。
        assert!(!debug.contains("0123456789AB"));
    }

    #[test]
    fn debug_does_not_leak_secret_length() {
        let short = format!("{:?}", SecretKey::new("0123456789ABCDEF".to_string()));
        let long = format!(
            "{:?}",
            SecretKey::new("0123456789ABCDEF0123456789ABCDEF".to_string())
        );
        // どちらも表示幅は固定なので、`*`の数から元の長さは推測できない。
        assert_eq!(short.len(), long.len());
    }

    #[test]
    fn expose_returns_plaintext() {
        let key = SecretKey::new("plaintext-token".to_string());
        assert_eq!(key.expose(), "plaintext-token");
    }
}
