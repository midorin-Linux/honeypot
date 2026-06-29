use std::fmt as stdfmt;

use anyhow::{Context, Result, bail};
use chrono::Local;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    info,
};
pub use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{
    EnvFilter,
    fmt::{
        FmtContext,
        format::{FormatEvent, FormatFields, Writer},
    },
    registry::LookupSpan,
};

const MAX_FIELD_VALUE_LEN: usize = 100;

pub fn init_tracing() -> Result<WorkerGuard> {
    match std::fs::exists("logs") {
        Ok(false) => std::fs::create_dir_all("logs").context("Failed to create logs directory")?,
        Ok(true) => info!("logs directory already exists"),
        _ => bail!("Failed to check logs directory existence"),
    }

    let appender = rolling::daily("logs", "honeypot.log");
    let (non_blocking, guard) = non_blocking(appender);

    dotenvy::dotenv().ok();

    let env_filter = EnvFilter::new(std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into()));

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .with_ansi(false)
        .event_format(TruncatingEventFormat)
        .init();

    Ok(guard)
}

struct TruncatingEventFormat;

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
