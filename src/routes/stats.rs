//! /api/stats — aggregate KPIs powering the dashboard ribbon.

use crate::error::ApiResult;
use crate::models::StatusSummary;
use crate::state::AppState;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(summary))
}

async fn summary(State(state): State<AppState>) -> ApiResult<Json<StatusSummary>> {
    let total_skus: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
        .fetch_one(&state.pool)
        .await?;
    let total_units: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(qty), 0) FROM items")
        .fetch_one(&state.pool)
        .await?;
    // Force REAL so sqlx never tries to decode INTEGER 0 as f64 on an
    // empty table.
    let total_value: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(qty * cost), 0) AS REAL) FROM items")
            .fetch_one(&state.pool)
            .await?;
    let low_stock: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE qty > 0 AND qty < min_qty")
            .fetch_one(&state.pool)
            .await?;
    let out_of_stock: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE qty = 0")
        .fetch_one(&state.pool)
        .await?;
    let serialized_units: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(json_array_length(serials)), 0) \
         FROM item_stock WHERE serials IS NOT NULL",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    let open_pos: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM purchase_orders WHERE status NOT IN ('received', 'cancelled')",
    )
    .fetch_one(&state.pool)
    .await?;
    let open_sos: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sales_orders WHERE status IN ('open', 'picking')")
            .fetch_one(&state.pool)
            .await?;
    let pending_transfers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM transfers WHERE status = 'pending'")
            .fetch_one(&state.pool)
            .await?;
    let scheduled_counts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM counts WHERE status IN ('scheduled', 'open')")
            .fetch_one(&state.pool)
            .await?;

    Ok(Json(StatusSummary {
        total_skus,
        total_units,
        total_value: total_value.round(),
        low_stock,
        out_of_stock,
        serialized_units,
        open_pos,
        open_sos,
        pending_transfers,
        scheduled_counts,
    }))
}
