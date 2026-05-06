//! RACKLOG — homelab inventory ops service entry point.

mod config;
mod db;
mod error;
mod logging;

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

    // HTTP server lands in the next commit. Closing the pool cleanly on
    // exit keeps the journal in good shape for the next boot.
    pool.close().await;
    Ok(())
}
