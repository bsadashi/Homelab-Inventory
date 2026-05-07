//! /api/forecast — reorder-point analysis and one-click draft-PO
//! materialisation.

use crate::audit::log_in_tx;
use crate::auth::{Authed, RequireAdmin};
use crate::error::{ApiError, ApiResult};
use crate::forecasting::{reorder_report, DEFAULT_LOOKBACK_DAYS};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reorder", get(reorder).post(apply_reorder))
}

#[derive(Debug, Deserialize, Default)]
pub struct LookbackQuery {
    pub lookback: Option<i64>,
}

async fn reorder(
    State(state): State<AppState>,
    _: Authed,
    Query(q): Query<LookbackQuery>,
) -> ApiResult<Json<crate::forecasting::ForecastReport>> {
    let report = reorder_report(&state.pool, q.lookback.unwrap_or(DEFAULT_LOOKBACK_DAYS)).await?;
    Ok(Json(report))
}

#[derive(Debug, Deserialize, Default)]
pub struct ApplyQuery {
    pub lookback: Option<i64>,
    /// `?confirm=1` is required to actually create the draft POs.
    pub confirm: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApplyResponse {
    created_pos: Vec<String>,
    items_covered: usize,
}

async fn apply_reorder(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Query(q): Query<ApplyQuery>,
) -> ApiResult<(StatusCode, Json<ApplyResponse>)> {
    if !matches!(q.confirm.as_deref(), Some("1") | Some("true") | Some("yes")) {
        return Err(ApiError::BadRequest(
            "apply is destructive; pass ?confirm=1 to materialise draft POs".into(),
        ));
    }
    let report = reorder_report(&state.pool, q.lookback.unwrap_or(DEFAULT_LOOKBACK_DAYS)).await?;

    let mut created_pos: Vec<String> = Vec::new();
    let mut items_covered = 0usize;

    for draft in &report.draft_pos {
        // Skip drafts with no supplier — operator should fix the
        // item record first; auto-creating an unrouteable PO would
        // just create cleanup work.
        let Some(supplier_id) = draft.supplier_id.as_deref() else {
            continue;
        };
        let po_id = format!("PO-AUTO-{}", chrono::Utc::now().timestamp_millis());
        let mut tx = state.pool.begin().await?;
        sqlx::query(
            "INSERT INTO purchase_orders (id, supplier_id, status, created, total) \
             VALUES (?, ?, 'draft', date('now'), ?)",
        )
        .bind(&po_id)
        .bind(supplier_id)
        .bind(draft.total)
        .execute(&mut *tx)
        .await?;
        for line in &draft.lines {
            sqlx::query(
                "INSERT INTO purchase_order_lines (po_id, sku, qty, cost) VALUES (?, ?, ?, ?)",
            )
            .bind(&po_id)
            .bind(&line.sku)
            .bind(line.qty)
            .bind(line.cost)
            .execute(&mut *tx)
            .await?;
        }
        log_in_tx(
            &mut tx,
            &auth.username,
            "po.auto_draft",
            Some(&po_id),
            &format!(
                "Auto-drafted PO for supplier {} covering {} item(s) (${:.2})",
                supplier_id,
                draft.lines.len(),
                draft.total
            ),
        )
        .await?;
        tx.commit().await?;
        items_covered += draft.item_ids.len();
        created_pos.push(po_id);
    }

    Ok((
        StatusCode::CREATED,
        Json(ApplyResponse {
            created_pos,
            items_covered,
        }),
    ))
}
