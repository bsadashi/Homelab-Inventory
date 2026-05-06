//! /api/sos — sales / pick orders. In a homelab context these represent
//! parts allocated to a project (e.g. "NAS rebuild"); the schema is
//! deliberately the same as a real SO so a small business deployment
//! can use the same endpoints unchanged.

use crate::audit::{record_in_tx, AuditEvent};
use crate::error::{ApiError, ApiResult};
use crate::models::{SalesLine, SalesOrder, SalesOrderInput};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", put(update).delete(delete).get(get_one))
}

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<Vec<SalesOrder>>> {
    let limit = p.limit.unwrap_or(200).clamp(1, 2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let rows = sqlx::query(
        "SELECT id, project, status, priority, created FROM sales_orders \
         ORDER BY created DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let mut out: Vec<SalesOrder> = rows
        .into_iter()
        .map(|r| SalesOrder {
            id: r.get("id"),
            proj: r.try_get("project").ok().flatten(),
            status: r.get("status"),
            created: r.get("created"),
            priority: r.try_get("priority").ok().flatten(),
            lines: Vec::new(),
        })
        .collect();
    let lines = sqlx::query("SELECT so_id, sku, qty FROM sales_order_lines")
        .fetch_all(&state.pool)
        .await?;
    let mut by: std::collections::HashMap<String, Vec<SalesLine>> = Default::default();
    for r in lines {
        let so_id: String = r.get("so_id");
        by.entry(so_id).or_default().push(SalesLine {
            sku: r.get("sku"),
            qty: r.try_get("qty").unwrap_or(0),
        });
    }
    for so in out.iter_mut() {
        so.lines = by.remove(&so.id).unwrap_or_default();
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<SalesOrder>> {
    let row = sqlx::query(
        "SELECT id, project, status, priority, created FROM sales_orders WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let lines = sqlx::query("SELECT sku, qty FROM sales_order_lines WHERE so_id = ?")
        .bind(&id)
        .fetch_all(&state.pool)
        .await?
        .into_iter()
        .map(|r| SalesLine {
            sku: r.get("sku"),
            qty: r.try_get("qty").unwrap_or(0),
        })
        .collect();
    Ok(Json(SalesOrder {
        id: row.get("id"),
        proj: row.try_get("project").ok().flatten(),
        status: row.get("status"),
        created: row.get("created"),
        priority: row.try_get("priority").ok().flatten(),
        lines,
    }))
}

async fn create(
    State(state): State<AppState>,
    Json(input): Json<SalesOrderInput>,
) -> ApiResult<(StatusCode, Json<SalesOrder>)> {
    let id = input.id.clone().unwrap_or_else(|| {
        format!("SO-{}", chrono::Utc::now().timestamp())
    });
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO sales_orders (id, project, status, priority, created) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.proj)
    .bind(&input.status)
    .bind(&input.priority)
    .bind(&input.created)
    .execute(&mut *tx)
    .await?;
    for line in &input.lines {
        sqlx::query("INSERT INTO sales_order_lines (so_id, sku, qty) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(&line.sku)
            .bind(line.qty)
            .execute(&mut *tx)
            .await?;
    }
    record_in_tx(
        &mut tx,
        AuditEvent {
            user: "api",
            kind: "so.create",
            r#ref: Some(&id),
            description: &format!("Created pick order {}", id),
            payload: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(SalesOrder {
            id,
            proj: input.proj,
            status: input.status,
            created: input.created,
            priority: input.priority,
            lines: input.lines,
        }),
    ))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<SalesOrderInput>,
) -> ApiResult<Json<SalesOrder>> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        "UPDATE sales_orders SET project = ?, status = ?, priority = ?, created = ?, \
         updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&input.proj)
    .bind(&input.status)
    .bind(&input.priority)
    .bind(&input.created)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    sqlx::query("DELETE FROM sales_order_lines WHERE so_id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    for line in &input.lines {
        sqlx::query("INSERT INTO sales_order_lines (so_id, sku, qty) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(&line.sku)
            .bind(line.qty)
            .execute(&mut *tx)
            .await?;
    }
    record_in_tx(
        &mut tx,
        AuditEvent {
            user: "api",
            kind: "so.update",
            r#ref: Some(&id),
            description: &format!("Updated pick order {} → {}", id, input.status),
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
    let res = sqlx::query("DELETE FROM sales_orders WHERE id = ?")
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
            kind: "so.delete",
            r#ref: Some(&id),
            description: &format!("Deleted pick order {}", id),
            payload: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
