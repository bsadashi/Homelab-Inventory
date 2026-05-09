//! Tracing setup. Picks a JSON or pretty subscriber based on config so the
//! same binary runs cleanly under `kubectl logs` and during local dev.

use crate::config::LogFormat;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(format: LogFormat) {
    // RACKLOG_LOG (preferred) → RUST_LOG → safe default. Keep dependency
    // chatter to warn so the structured access log stays readable.
    let filter = std::env::var("RACKLOG_LOG")
        .ok()
        .or_else(|| std::env::var("RUST_LOG").ok())
        .unwrap_or_else(|| "info,sqlx=warn,tower_http=info,hyper=warn".into());
    let env_filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));

    let registry = tracing_subscriber::registry().with(env_filter);
    match format {
        LogFormat::Json => {
            registry
                .with(
                    fmt::layer()
                        .json()
                        .with_current_span(false)
                        .with_span_list(false),
                )
                .init();
        }
        LogFormat::Pretty => {
            registry.with(fmt::layer().compact()).init();
        }
    }
}
