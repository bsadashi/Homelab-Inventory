//! /api/pos — purchase orders. Lines are stored in a child table and
//! folded back into the JSON shape the frontend expects on read.

use crate::auth::RequireOperator;
use crate::routes::pagination::PageQuery;
use crate::audit::log_in_tx;
use crate::error::{ApiError, ApiResult};
use crate::models::{PurchaseLine, PurchaseOrder, PurchaseOrderInput};
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
) -> ApiResult<Json<Vec<PurchaseOrder>>> {
    let (limit, offset) = p.resolve();
    let rows = sqlx::query(
        "SELECT id, supplier_id, status, created, expected, received, total \
         FROM purchase_orders ORDER BY created DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let mut out: Vec<PurchaseOrder> = rows
        .into_iter()
        .map(|r| PurchaseOrder {
            id: r.get("id"),
            supplier: r.try_get("supplier_id").ok().flatten(),
            status: r.get("status"),
            created: r.get("created"),
            expected: r.try_get("expected").ok().flatten(),
            received: r.try_get("received").ok().flatten(),
            total: r.try_get("total").unwrap_or(0.0),
            lines: Vec::new(),
        })
        .collect();

    let lines = sqlx::query("SELECT po_id, sku, qty, cost FROM purchase_order_lines")
        .fetch_all(&state.pool)
        .await?;
    let mut by_po: std::collections::HashMap<String, Vec<PurchaseLine>> = Default::default();
    for r in lines {
        let po_id: String = r.get("po_id");
        by_po.entry(po_id).or_default().push(PurchaseLine {
            sku: r.get("sku"),
            qty: r.try_get("qty").unwrap_or(0),
            cost: r.try_get("cost").unwrap_or(0.0),
        });
    }
    for po in out.iter_mut() {
        po.lines = by_po.remove(&po.id).unwrap_or_default();
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<PurchaseOrder>> {
    let row = sqlx::query(
        "SELECT id, supplier_id, status, created, expected, received, total \
         FROM purchase_orders WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let lines = sqlx::query(
        "SELECT sku, qty, cost FROM purchase_order_lines WHERE po_id = ?",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|r| PurchaseLine {
        sku: r.get("sku"),
        qty: r.try_get("qty").unwrap_or(0),
        cost: r.try_get("cost").unwrap_or(0.0),
    })
    .collect();

    Ok(Json(PurchaseOrder {
        id: row.get("id"),
        supplier: row.try_get("supplier_id").ok().flatten(),
        status: row.get("status"),
        created: row.get("created"),
        expected: row.try_get("expected").ok().flatten(),
        received: row.try_get("received").ok().flatten(),
        total: row.try_get("total").unwrap_or(0.0),
        lines,
    }))
}

async fn create(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Json(input): Json<PurchaseOrderInput>,
) -> ApiResult<(StatusCode, Json<PurchaseOrder>)> {
    let id = input.id.clone().unwrap_or_else(next_po_id);
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO purchase_orders (id, supplier_id, status, created, expected, received, total) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.supplier)
    .bind(&input.status)
    .bind(&input.created)
    .bind(&input.expected)
    .bind(&input.received)
    .bind(input.total)
    .execute(&mut *tx)
    .await?;
    for line in &input.lines {
        sqlx::query(
            "INSERT INTO purchase_order_lines (po_id, sku, qty, cost) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&line.sku)
        .bind(line.qty)
        .bind(line.cost)
        .execute(&mut *tx)
        .await?;
    }
    log_in_tx(&mut tx, &auth.username, "po.create", Some(&id), &format!("Created purchase order {}", id)).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(PurchaseOrder {
            id,
            supplier: input.supplier,
            status: input.status,
            created: input.created,
            expected: input.expected,
            received: input.received,
            total: input.total,
            lines: input.lines,
        }),
    ))
}

async fn update(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
    Json(input): Json<PurchaseOrderInput>,
) -> ApiResult<Json<PurchaseOrder>> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        "UPDATE purchase_orders SET supplier_id = ?, status = ?, created = ?, expected = ?, \
         received = ?, total = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&input.supplier)
    .bind(&input.status)
    .bind(&input.created)
    .bind(&input.expected)
    .bind(&input.received)
    .bind(input.total)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    sqlx::query("DELETE FROM purchase_order_lines WHERE po_id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    for line in &input.lines {
        sqlx::query(
            "INSERT INTO purchase_order_lines (po_id, sku, qty, cost) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&line.sku)
        .bind(line.qty)
        .bind(line.cost)
        .execute(&mut *tx)
        .await?;
    }
    log_in_tx(&mut tx, &auth.username, "po.update", Some(&id), &format!("Updated purchase order {} → {}", id, input.status)).await?;
    tx.commit().await?;
    get_one(State(state), Path(id)).await
}

async fn delete(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query("DELETE FROM purchase_orders WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    log_in_tx(&mut tx, &auth.username, "po.delete", Some(&id), &format!("Deleted purchase order {}", id)).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn next_po_id() -> String {
    format!("PO-{}", chrono::Utc::now().timestamp())
}
