//! /api/locations — physical hierarchy (rack → shelf → bin).

use crate::auth::RequireOperator;
use crate::audit::log_in_tx;
use crate::error::{ApiError, ApiResult};
use crate::models::{Location, LocationInput};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use sqlx::Row;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", put(update).delete(delete).get(get_one))
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<Location>>> {
    let rows = sqlx::query(
        "SELECT id, code, name, type, parent, bins FROM locations ORDER BY code",
    )
    .fetch_all(&state.pool)
    .await?;
    let out = rows
        .into_iter()
        .map(|r| Location {
            id: r.get("id"),
            code: r.get("code"),
            name: r.get("name"),
            kind: r.get("type"),
            parent: r.try_get("parent").ok().flatten(),
            bins: serde_json::from_str(&r.get::<String, _>("bins")).unwrap_or_default(),
        })
        .collect();
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Location>> {
    let row = sqlx::query(
        "SELECT id, code, name, type, parent, bins FROM locations WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(Location {
        id: row.get("id"),
        code: row.get("code"),
        name: row.get("name"),
        kind: row.get("type"),
        parent: row.try_get("parent").ok().flatten(),
        bins: serde_json::from_str(&row.get::<String, _>("bins")).unwrap_or_default(),
    }))
}

async fn create(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Json(input): Json<LocationInput>,
) -> ApiResult<(StatusCode, Json<Location>)> {
    if input.code.trim().is_empty() || input.name.trim().is_empty() {
        return Err(ApiError::BadRequest("code and name are required".into()));
    }
    let id = input
        .id
        .unwrap_or_else(|| format!("L-{}", short_id()));
    let bins_json = serde_json::to_string(&input.bins)?;

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO locations (id, code, name, type, parent, bins) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.code)
    .bind(&input.name)
    .bind(&input.kind)
    .bind(&input.parent)
    .bind(&bins_json)
    .execute(&mut *tx)
    .await
    .map_err(map_unique)?;
    log_in_tx(&mut tx, &auth.username, "location.create", Some(&id), &format!("Created location {} ({})", input.name, input.code)).await?;
    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(Location {
            id,
            code: input.code,
            name: input.name,
            kind: input.kind,
            parent: input.parent,
            bins: input.bins,
        }),
    ))
}

async fn update(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
    Json(input): Json<LocationInput>,
) -> ApiResult<Json<Location>> {
    let bins_json = serde_json::to_string(&input.bins)?;
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        r#"UPDATE locations SET code = ?, name = ?, type = ?, parent = ?, bins = ?,
           updated_at = datetime('now') WHERE id = ?"#,
    )
    .bind(&input.code)
    .bind(&input.name)
    .bind(&input.kind)
    .bind(&input.parent)
    .bind(&bins_json)
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(map_unique)?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    log_in_tx(&mut tx, &auth.username, "location.update", Some(&id), &format!("Updated location {}", input.code)).await?;
    tx.commit().await?;
    Ok(Json(Location {
        id,
        code: input.code,
        name: input.name,
        kind: input.kind,
        parent: input.parent,
        bins: input.bins,
    }))
}

async fn delete(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query("DELETE FROM locations WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|e| match e {
            // FK violation when stock still references the location.
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => {
                ApiError::Conflict("location still has stock or transfers attached".into())
            }
            other => ApiError::Database(other),
        })?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    log_in_tx(&mut tx, &auth.username, "location.delete", Some(&id), &format!("Deleted location {}", id)).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Full v4 UUID (32 hex chars). Previously truncated to 8 chars,
/// giving only 32 bits of entropy — birthday collisions begin around
/// 64 K rows. The full UUID has 122 bits.
fn short_id() -> String {
    Uuid::new_v4().simple().to_string()
}

fn map_unique(e: sqlx::Error) -> ApiError {
    match e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ApiError::Conflict("code already exists".into())
        }
        other => ApiError::Database(other),
    }
}
