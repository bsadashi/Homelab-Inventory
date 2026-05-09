//! Shared, cheaply-cloneable application state. Axum extracts this from
//! every handler so we never need globals.

use crate::config::Config;
use crate::db::Db;
use crate::sku_sync::Provider;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: Db,
    pub cfg: Arc<Config>,
    /// SKU sync providers, in fallback order. Empty when no provider is
    /// configured — the lookup route returns gracefully in that case.
    pub providers: Arc<Vec<Box<dyn Provider>>>,
}

impl AppState {
    pub fn new(pool: Db, cfg: Config) -> Self {
        let providers = crate::sku_sync::providers_from_env();
        Self::with_providers(pool, cfg, providers)
    }

    pub fn with_providers(pool: Db, cfg: Config, providers: Vec<Box<dyn Provider>>) -> Self {
        Self {
            pool,
            cfg: Arc::new(cfg),
            providers: Arc::new(providers),
        }
    }
}
