//! RACKLOG — homelab inventory ops service entry point.

mod audit;
mod config;
mod db;
mod error;
mod logging;
mod models;

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

    let chain = audit::verify_chain(&pool).await?;
    if chain.valid {
        tracing::info!(entries = chain.entries, "audit chain verified");
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
