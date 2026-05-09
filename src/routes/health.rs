//! Liveness and readiness endpoints for Kubernetes probes.
//!
//! `/healthz` is cheap and stays green as long as the process is up.
//! `/readyz` runs a short SQL probe so the load balancer only sends
//! traffic once the database is reachable.

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct Status {
    status: &'static str,
    service: &'static str,
    version: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
}

async fn healthz() -> Json<Status> {
    Json(Status {
        status: "ok",
        service: "racklog",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn readyz(State(state): State<AppState>) -> ApiResult<Json<Status>> {
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(Status {
        status: "ready",
        service: "racklog",
        version: env!("CARGO_PKG_VERSION"),
    }))
}
