//! /api/transfers — bin-to-bin stock movement records.

use crate::auth::RequireOperator;
use crate::routes::pagination::PageQuery;
use crate::audit::{record_in_tx, AuditEvent};
use crate::error::{ApiError, ApiResult};
use crate::models::{Transfer, TransferInput, TransferLine};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", put(update).delete(delete).get(get_one))
}


async fn list(
    State(state): State<AppState>,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<Vec<Transfer>>> {
    let (limit, offset) = p.resolve();
    let rows = sqlx::query(
        "SELECT id, from_loc, to_loc, date, status FROM transfers \
         ORDER BY date DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let mut out: Vec<Transfer> = rows
        .into_iter()
        .map(|r| Transfer {
            id: r.get("id"),
            from: r.try_get("from_loc").ok().flatten(),
            to: r.try_get("to_loc").ok().flatten(),
            date: r.get("date"),
            status: r.get("status"),
            lines: Vec::new(),
        })
        .collect();
    let lines = sqlx::query("SELECT transfer_id, sku, qty FROM transfer_lines")
        .fetch_all(&state.pool)
        .await?;
    let mut by: std::collections::HashMap<String, Vec<TransferLine>> = Default::default();
    for r in lines {
        let tid: String = r.get("transfer_id");
        by.entry(tid).or_default().push(TransferLine {
            sku: r.get("sku"),
            qty: r.try_get("qty").unwrap_or(0),
        });
    }
    for tr in out.iter_mut() {
        tr.lines = by.remove(&tr.id).unwrap_or_default();
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Transfer>> {
    let row = sqlx::query(
        "SELECT id, from_loc, to_loc, date, status FROM transfers WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let lines = sqlx::query("SELECT sku, qty FROM transfer_lines WHERE transfer_id = ?")
        .bind(&id)
        .fetch_all(&state.pool)
        .await?
        .into_iter()
        .map(|r| TransferLine {
            sku: r.get("sku"),
            qty: r.try_get("qty").unwrap_or(0),
        })
        .collect();
    Ok(Json(Transfer {
        id: row.get("id"),
        from: row.try_get("from_loc").ok().flatten(),
        to: row.try_get("to_loc").ok().flatten(),
        date: row.get("date"),
        status: row.get("status"),
        lines,
    }))
}

async fn create(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Json(input): Json<TransferInput>,
) -> ApiResult<(StatusCode, Json<Transfer>)> {
    let id = input.id.clone().unwrap_or_else(|| {
        format!("TR-{}", chrono::Utc::now().timestamp())
    });
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO transfers (id, from_loc, to_loc, date, status) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.from)
    .bind(&input.to)
    .bind(&input.date)
    .bind(&input.status)
    .execute(&mut *tx)
    .await?;
    for line in &input.lines {
        sqlx::query("INSERT INTO transfer_lines (transfer_id, sku, qty) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(&line.sku)
            .bind(line.qty)
            .execute(&mut *tx)
            .await?;
    }
    record_in_tx(
        &mut tx,
        AuditEvent {
            user: &auth.username,
            kind: "transfer.create",
            r#ref: Some(&id),
            description: &format!(
                "Created transfer {} ({} → {})",
                id,
                input.from.as_deref().unwrap_or("?"),
                input.to.as_deref().unwrap_or("?")
            ),
            payload: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Transfer {
            id,
            from: input.from,
            to: input.to,
            date: input.date,
            status: input.status,
            lines: input.lines,
        }),
    ))
}

async fn update(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
    Json(input): Json<TransferInput>,
) -> ApiResult<Json<Transfer>> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        "UPDATE transfers SET from_loc = ?, to_loc = ?, date = ?, status = ?, \
         updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&input.from)
    .bind(&input.to)
    .bind(&input.date)
    .bind(&input.status)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    sqlx::query("DELETE FROM transfer_lines WHERE transfer_id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    for line in &input.lines {
        sqlx::query("INSERT INTO transfer_lines (transfer_id, sku, qty) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(&line.sku)
            .bind(line.qty)
            .execute(&mut *tx)
            .await?;
    }
    record_in_tx(
        &mut tx,
        AuditEvent {
            user: &auth.username,
            kind: "transfer.update",
            r#ref: Some(&id),
            description: &format!("Updated transfer {} → {}", id, input.status),
            payload: None,
        },
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
    let res = sqlx::query("DELETE FROM transfers WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    record_in_tx(
        &mut tx,
        AuditEvent {
            user: &auth.username,
            kind: "transfer.delete",
            r#ref: Some(&id),
            description: &format!("Deleted transfer {}", id),
            payload: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
