//! /api/suppliers — vendors. Open-PO counts are computed on read so the
//! API always reflects the current order book without bookkeeping fields.

use crate::audit::log_in_tx;
use crate::auth::RequireOperator;
use crate::db::RowExt;
use crate::error::{ApiError, ApiResult};
use crate::models::{Supplier, SupplierInput};
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

async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<Supplier>>> {
    let rows = sqlx::query(
        r#"SELECT s.id, s.code, s.name, s.contact, s.lead_time, s.rating, s.total_spend,
                  COALESCE(p.open, 0) AS open_pos
           FROM suppliers s
           LEFT JOIN (
               SELECT supplier_id, COUNT(*) AS open
               FROM purchase_orders
               WHERE status NOT IN ('received', 'cancelled')
               GROUP BY supplier_id
           ) p ON p.supplier_id = s.id
           ORDER BY s.code"#,
    )
    .fetch_all(&state.pool)
    .await?;
    let out = rows.into_iter().map(row_to_supplier).collect();
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Supplier>> {
    let row = sqlx::query(
        r#"SELECT s.id, s.code, s.name, s.contact, s.lead_time, s.rating, s.total_spend,
                  (SELECT COUNT(*) FROM purchase_orders
                   WHERE supplier_id = s.id AND status NOT IN ('received','cancelled')) AS open_pos
           FROM suppliers s WHERE s.id = ?"#,
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(row_to_supplier(row)))
}

async fn create(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Json(input): Json<SupplierInput>,
) -> ApiResult<(StatusCode, Json<Supplier>)> {
    if input.code.trim().is_empty() || input.name.trim().is_empty() {
        return Err(ApiError::BadRequest("code and name are required".into()));
    }
    let id = input.id.unwrap_or_else(|| format!("V-{}", short_id()));
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO suppliers (id, code, name, contact, lead_time, rating, total_spend)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&input.code)
    .bind(&input.name)
    .bind(&input.contact)
    .bind(input.lead_time)
    .bind(input.rating)
    .bind(input.total_spend)
    .execute(&mut *tx)
    .await
    .map_err(map_unique)?;
    log_in_tx(
        &mut tx,
        &auth.username,
        "supplier.create",
        Some(&id),
        &format!("Created supplier {} ({})", input.name, input.code),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Supplier {
            id,
            code: input.code,
            name: input.name,
            contact: input.contact,
            lead_time: input.lead_time,
            rating: input.rating,
            open_pos: 0,
            total_spend: input.total_spend,
        }),
    ))
}

async fn update(
    State(state): State<AppState>,
    RequireOperator(auth): RequireOperator,
    Path(id): Path<String>,
    Json(input): Json<SupplierInput>,
) -> ApiResult<Json<Supplier>> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        r#"UPDATE suppliers SET code = ?, name = ?, contact = ?, lead_time = ?,
                rating = ?, total_spend = ?, updated_at = datetime('now')
           WHERE id = ?"#,
    )
    .bind(&input.code)
    .bind(&input.name)
    .bind(&input.contact)
    .bind(input.lead_time)
    .bind(input.rating)
    .bind(input.total_spend)
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(map_unique)?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    log_in_tx(
        &mut tx,
        &auth.username,
        "supplier.update",
        Some(&id),
        &format!("Updated supplier {}", input.code),
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
    let res = sqlx::query("DELETE FROM suppliers WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    log_in_tx(
        &mut tx,
        &auth.username,
        "supplier.delete",
        Some(&id),
        &format!("Deleted supplier {}", id),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn row_to_supplier(r: sqlx::sqlite::SqliteRow) -> Supplier {
    Supplier {
        id: r.get("id"),
        code: r.get("code"),
        name: r.get("name"),
        contact: r.opt_string("contact"),
        lead_time: r.opt_i64("lead_time"),
        rating: r.opt_f64("rating"),
        open_pos: r.i64_or("open_pos", 0),
        total_spend: r.f64_or("total_spend", 0.0),
    }
}

/// Full v4 UUID (32 hex chars). See locations::short_id for
/// the rationale on dropping the 8-char truncation.
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
