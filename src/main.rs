//! RACKLOG — homelab inventory ops service entry point.

use racklog::{audit, config::Config, db, logging, routes, seed, state::AppState};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Tiny self-check subcommand. Distroless has no shell or curl,
    // so the docker-compose healthcheck shells out to the binary
    // itself: `racklog healthcheck`. Hits /api/healthz on the local
    // bind, exits 0 on 2xx, 1 otherwise.
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return run_healthcheck().await;
    }

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

    // L3 (pen-test): the forwarded-header trust path is binary —
    // when on, *any* peer that can reach the bind socket can claim
    // an admin identity via X-Forwarded-User / X-Forwarded-Groups.
    // The intended deployment is "behind a trusted reverse proxy
    // that strips these headers from upstream requests"; warn at
    // startup if both trust=on and we're listening on a non-loopback
    // address, so the operator gets one chance to spot a misconfig.
    if cfg.trust_forwarded_headers && !cfg.bind.ip().is_loopback() {
        tracing::warn!(
            bind = %cfg.bind,
            "RACKLOG_TRUST_FORWARDED_HEADERS=1 with a non-loopback bind. \
             Ensure ONLY a trusted reverse proxy (Authelia / oauth2-proxy / \
             Authentik / Caddy / nginx) can reach this socket — anyone else \
             can spoof X-Forwarded-User / X-Forwarded-Groups.",
        );
    }

    // into_make_service_with_connect_info gives the middleware access
    // to the peer SocketAddr via ConnectInfo, which the trusted-proxy
    // CIDR check needs to filter spoofed X-Forwarded-User from
    // off-network peers.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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

/// `racklog healthcheck` — used by the docker-compose `healthcheck`
/// stanza to probe the running service from inside the container.
/// Distroless images have no shell or curl, so we can't `wget` like
/// the previous Debian-slim image did. The binary becomes its own
/// healthcheck client.
///
/// Designed to never panic and never emit a backtrace: a probe
/// against a temporarily-down server is the *expected* state during
/// startup, so failure prints one short line on stderr and exits 1.
async fn run_healthcheck() -> anyhow::Result<()> {
    let port = std::env::var("RACKLOG_BIND")
        .ok()
        .and_then(|b| b.rsplit(':').next().map(str::to_string))
        .unwrap_or_else(|| "8080".to_string());
    let url = format!("http://127.0.0.1:{port}/api/healthz");
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("healthcheck: {e}");
            std::process::exit(1);
        }
    };
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => Ok(()),
        Ok(resp) => {
            eprintln!("healthcheck: status {}", resp.status());
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("healthcheck: {e}");
            std::process::exit(1);
        }
    }
}
