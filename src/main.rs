//! RACKLOG — homelab inventory ops service entry point.

use racklog::{audit, config::Config, db, logging, routes, seed, state::AppState};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal;

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

    // Initialise the uptime clock at boot so /api/admin/info and the
    // metrics endpoint report time-since-boot, not time-since-first-call.
    routes::admin::touch_uptime();

    let state = AppState::new(pool.clone(), cfg.clone());
    // routes::serve adds the outer infrastructure layers (tracing,
    // timeout, body limit, compression). Tests use the same helper
    // so a regression in any of those layers is caught in CI.
    let app = routes::serve(state);

    let listener = TcpListener::bind(cfg.bind).await?;
    tracing::info!(addr = %cfg.bind, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

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
        // If signal registration ever fails (rare; e.g. inside a
        // restricted sandbox where SIGTERM is preempted), fall back
        // to ctrl_c-only by parking forever — never panic the
        // shutdown future itself.
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::warn!(%e, "SIGTERM handler unavailable, ctrl-c only");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("ctrl-c received, shutting down"),
        _ = terminate => tracing::info!("SIGTERM received, shutting down"),
    }
}
