//! /api/counts — cycle counts and audits.

use crate::audit::{record_in_tx, AuditEvent};
use crate::error::{ApiError, ApiResult};
use crate::models::{Count, CountInput};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", put(update).delete(delete).get(get_one))
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<Count>>> {
    let rows = sqlx::query(
        "SELECT id, location_id, date, status, counted, variance, by_user \
         FROM counts ORDER BY date DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows.into_iter().map(row_to_count).collect()))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Count>> {
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
    Json(input): Json<CountInput>,
) -> ApiResult<(StatusCode, Json<Count>)> {
    let id = input.id.clone().unwrap_or_else(|| {
        format!("CYC-{}", chrono::Utc::now().timestamp())
    });
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
    record_in_tx(
        &mut tx,
        AuditEvent {
            user: "api",
            kind: "count.create",
            r#ref: Some(&id),
            description: &format!("Created count {}", id),
            payload: None,
        },
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
    record_in_tx(
        &mut tx,
        AuditEvent {
            user: "api",
            kind: "count.update",
            r#ref: Some(&id),
            description: &format!(
                "Updated count {} (variance {})",
                id, input.variance
            ),
            payload: None,
        },
    )
    .await?;
    tx.commit().await?;
    get_one(State(state), Path(id)).await
}

async fn delete(
    State(state): State<AppState>,
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
    record_in_tx(
        &mut tx,
        AuditEvent {
            user: "api",
            kind: "count.delete",
            r#ref: Some(&id),
            description: &format!("Deleted count {}", id),
            payload: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn row_to_count(r: sqlx::sqlite::SqliteRow) -> Count {
    Count {
        id: r.get("id"),
        loc: r.try_get("location_id").ok().flatten(),
        date: r.get("date"),
        status: r.get("status"),
        counted: r.try_get("counted").unwrap_or(0),
        variance: r.try_get("variance").unwrap_or(0),
        by: r.try_get("by_user").ok().flatten(),
    }
}
