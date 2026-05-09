//! /api/activity — read-only window onto the tamper-evident log.
//!
//! Activity entries are append-only by design (writes flow through
//! `audit::record_in_tx` from inside other handlers' transactions). This
//! module exposes the read views and the chain-verification probe.

use crate::audit::{recent, verify_chain, ChainStatus};
use crate::error::ApiResult;
use crate::models::ActivityEntry;
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/verify", get(verify))
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> ApiResult<Json<Vec<ActivityEntry>>> {
    let limit = q.limit.unwrap_or(200).clamp(1, 5000);
    let entries = recent(&state.pool, limit).await?;
    Ok(Json(entries))
}

async fn verify(State(state): State<AppState>) -> ApiResult<Json<ChainStatus>> {
    Ok(Json(verify_chain(&state.pool).await?))
}
