//! Tracing/logging initialization

use anyhow::Result;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    prelude::*,
    EnvFilter,
};

use crate::config::LoggingConfig;

/// Initialize the tracing subscriber based on configuration
pub fn init_tracing(config: &LoggingConfig) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    let subscriber = tracing_subscriber::registry().with(filter);

    match config.format.as_str() {
        "json" => {
            let fmt_layer = fmt::layer()
                .json()
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_span_events(FmtSpan::CLOSE);
            subscriber.with(fmt_layer).init();
        }
        _ => {
            let fmt_layer = fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_span_events(FmtSpan::CLOSE);
            subscriber.with(fmt_layer).init();
        }
    }

    Ok(())
}

