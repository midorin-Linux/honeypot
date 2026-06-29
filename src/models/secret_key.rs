use std::{fmt as stdfmt, fmt};

use chrono::Local;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
    registry::LookupSpan,
};
use zeroize::Zeroizing;

const MAX_FIELD_VALUE_LEN: usize = 100;

#[derive(Clone)]
pub struct SecretKey(Zeroizing<SecretString>);

impl SecretKey {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(SecretString::new(value.into())))
    }

    pub fn expose(&self) -> &str {
        (*self.0).expose_secret()
    }
}

impl AsRef<str> for SecretKey {
    fn as_ref(&self) -> &str {
        self.expose()
    }
}

impl Serialize for SecretKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.expose())
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
        let visible_length = 4;
        let masked = {
            let inner = (*self.0).expose_secret();
            let length = inner.chars().count();
            let start = length.saturating_sub(visible_length);
            let extracted: String = inner.chars().skip(start).collect();
            format!("{:*>20}", &extracted)
        };
        f.debug_tuple("SecretKey").field(&masked).finish()
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {}
}

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
