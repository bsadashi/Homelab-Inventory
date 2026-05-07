//! /api/trackers — bind / list / unbind physical asset trackers
//! (AirTag, Galaxy SmartTag, Tile, generic BLE beacon, …) to
//! inventory items, plus a sync endpoint where a companion service
//! pushes the latest known location.
//!
//! The actual provider integrations (talking to OpenHaystack /
//! Apple Find My, Samsung's SmartThings Find, the Tile API, a local
//! BLE scanner) live outside this binary — they POST to
//! `/api/trackers/{id}/sync` with the position + battery they
//! observed. RACKLOG just records the binding and the latest
//! report.

use crate::audit::log_in_tx;
use crate::auth::{Authed, RequireOperator};
use crate::db::RowExt;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", get(get_one).delete(delete))
        .route("/:id/sync", post(sync))
}

#[derive(Debug, Clone, Serialize)]
pub struct Tracker {
    pub id: String,
    pub item_id: String,
    pub provider: String,
    pub provider_id: String,
    pub label: Option<String>,
    pub last_seen_lat: Option<f64>,
    pub last_seen_lng: Option<f64>,
    pub last_seen_at: Option<String>,
    pub last_seen_label: Option<String>,
    pub battery_pct: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

fn row_to_tracker(r: sqlx::sqlite::SqliteRow) -> Tracker {
    Tracker {
        id: r.string_or_default("id"),
        item_id: r.string_or_default("item_id"),
        provider: r.string_or_default("provider"),
        provider_id: r.string_or_default("provider_id"),
        label: r.opt_string("label"),
        last_seen_lat: r.opt_f64("last_seen_lat"),
        last_seen_lng: r.opt_f64("last_seen_lng"),
        last_seen_at: r.opt_string("last_seen_at"),
        last_seen_label: r.opt_string("last_seen_label"),
        battery_pct: r.opt_i64("battery_pct"),
        created_at: r.string_or_default("created_at"),
        updated_at: r.string_or_default("updated_at"),
    }
}

async fn list(State(state): State<AppState>, _: Authed) -> ApiResult<Json<Vec<Tracker>>> {
    let rows = sqlx::query(
        "SELECT id, item_id, provider, provider_id, label,
                last_seen_lat, last_seen_lng, last_seen_at,
                last_seen_label, battery_pct, created_at, updated_at
         FROM trackers ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows.into_iter().map(row_to_tracker).collect()))
}

async fn get_one(
    State(state): State<AppState>,
    _: Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<Tracker>> {
    let row = sqlx::query(
        "SELECT id, item_id, provider, provider_id, label,
                last_seen_lat, last_seen_lng, last_seen_at,
                last_seen_label, battery_pct, created_at, updated_at
         FROM trackers WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(row_to_tracker(row)))
}

#[derive(Debug, Deserialize)]
pub struct CreateTrackerInput {
    pub item_id: String,
    pub provider: String,
    pub provider_id: String,
    pub label: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Json(input): Json<CreateTrackerInput>,
) -> ApiResult<(StatusCode, Json<Tracker>)> {
    // Tiny validation. provider/provider_id are operator-supplied
    // strings; we don't talk to the upstream service from the
    // create path because most provider integrations need network
    // round trips that don't belong in a synchronous handler.
    if input.provider.trim().is_empty() || input.provider_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "provider and provider_id are required".into(),
        ));
    }
    if !valid_provider(&input.provider) {
        return Err(ApiError::BadRequest(
            "provider must be one of: airtag, smarttag, tile, ble-beacon, ruuvi, custom".into(),
        ));
    }

    // Confirm the item exists so a typo doesn't leave an orphan row
    // (the FK would catch it but the error would be ugly).
    let item_exists: Option<String> = sqlx::query_scalar("SELECT id FROM items WHERE id = ?")
        .bind(&input.item_id)
        .fetch_optional(&state.pool)
        .await?;
    if item_exists.is_none() {
        return Err(ApiError::NotFound);
    }

    let id = format!("tk-{}", Uuid::new_v4().simple());
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO trackers (id, item_id, provider, provider_id, label) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.item_id)
    .bind(&input.provider)
    .bind(&input.provider_id)
    .bind(&input.label)
    .execute(&mut *tx)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ApiError::Conflict("tracker already bound".into())
        }
        other => ApiError::Database(other),
    })?;
    log_in_tx(
        &mut tx,
        &auth.username,
        "tracker.bind",
        Some(&id),
        &format!(
            "Bound {} tracker {} to item {}",
            input.provider, input.provider_id, input.item_id
        ),
    )
    .await?;
    tx.commit().await?;
    let row = sqlx::query(
        "SELECT id, item_id, provider, provider_id, label,
                last_seen_lat, last_seen_lng, last_seen_at,
                last_seen_label, battery_pct, created_at, updated_at
         FROM trackers WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(row_to_tracker(row))))
}

async fn delete(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query("DELETE FROM trackers WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    log_in_tx(
        &mut tx,
        &auth.username,
        "tracker.unbind",
        Some(&id),
        &format!("Unbound tracker {}", id),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SyncInput {
    pub last_seen_lat: Option<f64>,
    pub last_seen_lng: Option<f64>,
    pub last_seen_label: Option<String>,
    pub battery_pct: Option<i64>,
}

/// Push a fresh location report from a companion sync worker.
/// Authenticated identity is recorded in the audit log so the
/// operator can tell which integration last touched a tracker.
/// Requires operator role — sync mutates `trackers` rows, so it
/// follows the same write-path policy as bind/unbind. Issue a
/// per-companion API key under the operator role rather than
/// granting humans this privilege.
async fn sync(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
    Json(input): Json<SyncInput>,
) -> ApiResult<Json<Tracker>> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        "UPDATE trackers SET
              last_seen_lat = COALESCE(?, last_seen_lat),
              last_seen_lng = COALESCE(?, last_seen_lng),
              last_seen_label = COALESCE(?, last_seen_label),
              battery_pct = COALESCE(?, battery_pct),
              last_seen_at = datetime('now'),
              updated_at = datetime('now')
           WHERE id = ?",
    )
    .bind(input.last_seen_lat)
    .bind(input.last_seen_lng)
    .bind(&input.last_seen_label)
    .bind(input.battery_pct)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    // Quiet audit row — sync events fire often, so don't include
    // the lat/lng in the description (still recorded in the row).
    log_in_tx(
        &mut tx,
        &auth.username,
        "tracker.sync",
        Some(&id),
        &format!(
            "Sync · {} · {}",
            input.last_seen_label.as_deref().unwrap_or("(no label)"),
            input
                .battery_pct
                .map(|b| format!("{b}%"))
                .unwrap_or_else(|| "?%".into())
        ),
    )
    .await?;
    tx.commit().await?;
    let row = sqlx::query(
        "SELECT id, item_id, provider, provider_id, label,
                last_seen_lat, last_seen_lng, last_seen_at,
                last_seen_label, battery_pct, created_at, updated_at
         FROM trackers WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(row_to_tracker(row)))
}

fn valid_provider(p: &str) -> bool {
    matches!(
        p,
        "airtag" | "smarttag" | "tile" | "ble-beacon" | "ruuvi" | "custom"
    )
}
