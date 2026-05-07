//! /api/counts — cycle counts and audits.

use crate::audit::log_in_tx;
use crate::auth::RequireOperator;
use crate::db::RowExt;
use crate::error::{ApiError, ApiResult};
use crate::models::{Count, CountInput};
use crate::routes::pagination::PageQuery;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/vision", axum::routing::post(vision_ingest))
        .route("/:id", put(update).delete(delete).get(get_one))
}

#[derive(Debug, Deserialize)]
pub struct VisionObservation {
    /// SKU the model classified. Must already exist in the items
    /// catalog — vision can confirm a count, it can't create new
    /// SKUs (a hostile / mis-trained model otherwise becomes a write
    /// vector into the inventory).
    pub sku: String,
    pub qty: i64,
    /// 0.0 – 1.0 model confidence. Used for display only; the
    /// count is recorded regardless.
    pub confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct VisionCountInput {
    /// Where the camera is pointed.
    pub location_id: String,
    /// Optional bin within the location.
    pub bin: Option<String>,
    /// Identifier of the model run that produced these observations
    /// (e.g. `yolov8n-rack-2026-04-12`). Stored verbatim in the
    /// audit description so future drift is debuggable.
    pub model: Option<String>,
    /// URL to the source frame, if the companion service exposes one.
    pub evidence_url: Option<String>,
    pub observations: Vec<VisionObservation>,
}

#[derive(Debug, Serialize)]
struct VisionCountSummary {
    count_id: String,
    observations: usize,
    matched_skus: i64,
    /// Sum of qty deltas vs current items.qty across observed SKUs.
    /// Useful as a one-shot measure of "is the catalog drifting?".
    total_delta: i64,
}

/// Ingest a vision-derived count. Creates one `counts` row with
/// `source='vision'` and a `count.vision` audit entry summarising
/// the per-SKU deltas against the current `items.qty`. The
/// catalog itself is **not** written through this endpoint —
/// vision corroborates, it does not mutate. An operator can apply
/// adjustments via the existing item update / count workflow.
async fn vision_ingest(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Json(input): Json<VisionCountInput>,
) -> ApiResult<(StatusCode, Json<VisionCountSummary>)> {
    if input.observations.is_empty() {
        return Err(ApiError::BadRequest(
            "observations must not be empty".into(),
        ));
    }
    if input.observations.len() > 1024 {
        return Err(ApiError::BadRequest(
            "≤ 1024 observations per request".into(),
        ));
    }
    if let Some(url) = &input.evidence_url {
        if url.len() > 1024 {
            return Err(ApiError::BadRequest("evidence_url too long".into()));
        }
    }

    // Verify location exists.
    let loc_exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM locations WHERE id = ?")
            .bind(&input.location_id)
            .fetch_optional(&state.pool)
            .await?;
    if loc_exists.is_none() {
        return Err(ApiError::NotFound);
    }

    // Look up current qty for every observed SKU in one query so we
    // can compute deltas without N round trips.
    let placeholders: Vec<&str> = input.observations.iter().map(|_| "?").collect();
    let sql = format!(
        "SELECT sku, qty FROM items WHERE sku IN ({})",
        placeholders.join(",")
    );
    let mut q = sqlx::query(&sql);
    for o in &input.observations {
        q = q.bind(&o.sku);
    }
    let rows = q.fetch_all(&state.pool).await?;
    let mut current_qty: std::collections::HashMap<String, i64> = Default::default();
    for r in rows {
        use sqlx::Row as _;
        current_qty.insert(r.get::<String, _>("sku"), r.try_get("qty").unwrap_or(0));
    }

    let mut matched: i64 = 0;
    let mut total_delta: i64 = 0;
    let mut delta_lines: Vec<String> = Vec::new();
    let avg_confidence = if input.observations.iter().any(|o| o.confidence.is_some()) {
        let mut sum = 0.0;
        let mut n = 0;
        for o in &input.observations {
            if let Some(c) = o.confidence {
                sum += c;
                n += 1;
            }
        }
        if n > 0 {
            Some(sum / n as f64)
        } else {
            None
        }
    } else {
        None
    };
    for o in &input.observations {
        let cur = match current_qty.get(&o.sku) {
            Some(q) => *q,
            None => continue, // unknown SKU; surfaced via matched count
        };
        matched += 1;
        let delta = o.qty - cur;
        total_delta += delta;
        if delta != 0 {
            delta_lines.push(format!("{}: {:+}", o.sku, delta));
        }
    }

    let id = format!("CYC-V-{}", chrono::Utc::now().timestamp_millis());
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO counts (id, location_id, date, status, counted, variance, by_user,
                              source, confidence, evidence_url)
         VALUES (?, ?, date('now'), 'done', ?, ?, ?, 'vision', ?, ?)",
    )
    .bind(&id)
    .bind(&input.location_id)
    .bind(matched)
    .bind(total_delta)
    .bind(&auth.username)
    .bind(avg_confidence)
    .bind(&input.evidence_url)
    .execute(&mut *tx)
    .await?;

    let model = input.model.as_deref().unwrap_or("(unspecified)");
    let summary_desc = if delta_lines.is_empty() {
        format!(
            "Vision count by {model} at {} {} — {matched} SKUs match catalog exactly",
            input.location_id,
            input.bin.as_deref().unwrap_or(""),
        )
    } else {
        // Cap the audit description so a wildly-drifted shelf doesn't
        // blow past audit-row size limits.
        let head: Vec<&String> = delta_lines.iter().take(10).collect();
        let suffix = if delta_lines.len() > 10 {
            format!(" (+{} more)", delta_lines.len() - 10)
        } else {
            String::new()
        };
        format!(
            "Vision count by {model} at {} {} · {matched} SKUs · Δ {}{}",
            input.location_id,
            input.bin.as_deref().unwrap_or(""),
            head.iter().cloned().cloned().collect::<Vec<_>>().join(", "),
            suffix,
        )
    };
    log_in_tx(
        &mut tx,
        &auth.username,
        "count.vision",
        Some(&id),
        &summary_desc,
    )
    .await?;
    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(VisionCountSummary {
            count_id: id,
            observations: input.observations.len(),
            matched_skus: matched,
            total_delta,
        }),
    ))
}

async fn list(
    State(state): State<AppState>,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<Vec<Count>>> {
    let (limit, offset) = p.resolve();
    let rows = sqlx::query(
        "SELECT id, location_id, date, status, counted, variance, by_user \
         FROM counts ORDER BY date DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows.into_iter().map(row_to_count).collect()))
}

async fn get_one(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Count>> {
    let row = sqlx::query(
        "SELECT id, location_id, date, status, counted, variance, by_user \
         FROM counts WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(row_to_count(row)))
}

async fn create(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Json(input): Json<CountInput>,
) -> ApiResult<(StatusCode, Json<Count>)> {
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| format!("CYC-{}", chrono::Utc::now().timestamp()));
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO counts (id, location_id, date, status, counted, variance, by_user) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.loc)
    .bind(&input.date)
    .bind(&input.status)
    .bind(input.counted)
    .bind(input.variance)
    .bind(&input.by)
    .execute(&mut *tx)
    .await?;
    log_in_tx(
        &mut tx,
        &auth.username,
        "count.create",
        Some(&id),
        &format!("Created count {}", id),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Count {
            id,
            loc: input.loc,
            date: input.date,
            status: input.status,
            counted: input.counted,
            variance: input.variance,
            by: input.by,
        }),
    ))
}

async fn update(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
    Json(input): Json<CountInput>,
) -> ApiResult<Json<Count>> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        "UPDATE counts SET location_id = ?, date = ?, status = ?, counted = ?, \
         variance = ?, by_user = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&input.loc)
    .bind(&input.date)
    .bind(&input.status)
    .bind(input.counted)
    .bind(input.variance)
    .bind(&input.by)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    log_in_tx(
        &mut tx,
        &auth.username,
        "count.update",
        Some(&id),
        &format!("Updated count {} (variance {})", id, input.variance),
    )
    .await?;
    tx.commit().await?;
    get_one(State(state), Path(id)).await
}

async fn delete(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query("DELETE FROM counts WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    log_in_tx(
        &mut tx,
        &auth.username,
        "count.delete",
        Some(&id),
        &format!("Deleted count {}", id),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn row_to_count(r: sqlx::sqlite::SqliteRow) -> Count {
    Count {
        id: r.get("id"),
        loc: r.opt_string("location_id"),
        date: r.get("date"),
        status: r.get("status"),
        counted: r.i64_or("counted", 0),
        variance: r.i64_or("variance", 0),
        by: r.opt_string("by_user"),
    }
}
