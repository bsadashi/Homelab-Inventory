//! Top-level router. Each domain area gets its own sub-module so the file
//! tree mirrors the URL tree.

pub mod health;
pub mod items;
pub mod locations;
pub mod suppliers;

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
}
