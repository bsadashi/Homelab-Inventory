//! RACKLOG — homelab inventory ops service entry point.

mod audit;
mod config;
mod db;
mod error;
mod logging;
mod models;
mod seed;

use crate::config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = Config::from_env()?;
    logging::init(cfg.log_format);

    tracing::info!(
        bind = %cfg.bind,
        data_dir = %cfg.data_dir.display(),
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

    pool.close().await;
    Ok(())
}
