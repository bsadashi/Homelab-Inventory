//! RACKLOG — homelab inventory ops service entry point.

mod audit;
mod config;
mod db;
mod error;
mod logging;
mod models;
mod routes;
mod seed;
mod state;

use crate::config::Config;
use crate::state::AppState;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = Config::from_env()?;
    logging::init(cfg.log_format);

    tracing::info!(
        bind = %cfg.bind,
        data_dir = %cfg.data_dir.display(),
        version = env!("CARGO_PKG_VERSION"),
        "racklog starting"
    );

    let pool = db::connect(&cfg.database_url).await?;
    db::migrate(&pool).await?;
    tracing::info!("database ready, schema migrated");

    if cfg.seed_on_empty {
        match seed::seed_if_empty(&pool).await {
            Ok(true) => tracing::info!("seeded initial homelab dataset"),
            Ok(false) => tracing::info!("existing data found, skipping seed"),
            Err(e) => tracing::error!(%e, "seed failed"),
        }
    }

    let chain = audit::verify_chain(&pool).await?;
    if chain.valid {
        tracing::info!(
            entries = chain.entries,
            head = ?chain.head,
            "audit chain verified"
        );
    } else {
        tracing::warn!(
            entries = chain.entries,
            broken_at = ?chain.broken_at,
            "audit chain verification failed"
        );
    }

    let state = AppState::new(pool.clone(), cfg.clone());
    let app = routes::build(state)
        .layer(TraceLayer::new_for_http())
        .layer(tower_http::timeout::TimeoutLayer::new(cfg.request_timeout))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(cfg.max_body_bytes))
        .layer(tower_http::compression::CompressionLayer::new());

    let listener = TcpListener::bind(cfg.bind).await?;
    tracing::info!(addr = %cfg.bind, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Give in-flight queries a moment to finish, then close the pool cleanly
    // so SQLite checkpoints the WAL on exit.
    tokio::time::timeout(Duration::from_secs(5), pool.close())
        .await
        .ok();
    tracing::info!("shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .ok()
            .map(|mut s| async move { s.recv().await })
            .unwrap()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("ctrl-c received, shutting down"),
        _ = terminate => tracing::info!("SIGTERM received, shutting down"),
    }
}
