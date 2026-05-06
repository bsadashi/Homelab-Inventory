//! Shared, cheaply-cloneable application state. Axum extracts this from
//! every handler so we never need globals.

use crate::config::Config;
use crate::db::Db;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: Db,
    pub cfg: Arc<Config>,
}

impl AppState {
    pub fn new(pool: Db, cfg: Config) -> Self {
        Self {
            pool,
            cfg: Arc::new(cfg),
        }
    }
}
