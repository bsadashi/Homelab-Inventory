//! RACKLOG — homelab inventory ops service entry point.

mod config;
mod error;
mod logging;

use crate::config::Config;

fn main() -> anyhow::Result<()> {
    let cfg = Config::from_env()?;
    logging::init(cfg.log_format);
    tracing::info!(
        bind = %cfg.bind,
        data_dir = %cfg.data_dir.display(),
        "racklog starting (scaffold)"
    );

    // The HTTP server lands in the next commit. Returning success keeps the
    // binary trivially runnable from CI and container builds during the
    // incremental rollout.
    Ok(())
}
