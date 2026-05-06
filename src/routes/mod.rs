//! Top-level router. Each domain area gets its own sub-module so the file
//! tree mirrors the URL tree.

pub mod activity;
pub mod counts;
pub mod health;
pub mod items;
pub mod locations;
pub mod purchase_orders;
pub mod sales_orders;
pub mod stats;
pub mod suppliers;
pub mod transfers;

use crate::state::AppState;
use axum::Router;

pub fn build(state: AppState) -> Router {
    Router::new()
        .nest("/api", api_routes())
        .with_state(state)
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .nest("/items",     items::router())
        .nest("/locations", locations::router())
        .nest("/suppliers", suppliers::router())
        .nest("/pos",       purchase_orders::router())
        .nest("/sos",       sales_orders::router())
        .nest("/transfers", transfers::router())
        .nest("/counts",    counts::router())
        .nest("/activity",  activity::router())
        .nest("/stats",     stats::router())
}
