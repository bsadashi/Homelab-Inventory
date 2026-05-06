//! /api/items — catalog with per-bin stock, variants, lots and serials.
//!
//! Items are denormalised at write-time: variants/lots/tags/serials live
//! inside JSON-typed columns to keep reads cheap and the wire format
//! identical to the prototype's data shape.

use crate::audit::{record_in_tx, AuditEvent};
use crate::error::{ApiError, ApiResult};
use crate::models::{Item, ItemInput, StockLine};
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
pub struct ListFilter {
    pub category: Option<String>,
    pub supplier: Option<String>,
    pub barcode: Option<String>,
    pub q: Option<String>,
    /// Optional pagination — defaults are generous since the typical
    /// homelab catalog is well under the cap.
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    Query(filter): Query<ListFilter>,
) -> ApiResult<Json<Vec<Item>>> {
    let limit = filter.limit.unwrap_or(1000).clamp(1, 5000);
    let offset = filter.offset.unwrap_or(0).max(0);

    let mut sql = String::from(
        "SELECT id, sku, name, category, brand, supplier_id, cost, price, unit, \
         min_qty, max_qty, qty, allocated, barcode, variants, lots, tags, img, updated \
         FROM items WHERE 1=1",
    );
    if filter.category.is_some() { sql.push_str(" AND category = ?"); }
    if filter.supplier.is_some() { sql.push_str(" AND supplier_id = ?"); }
    if filter.barcode.is_some()  { sql.push_str(" AND barcode = ?"); }
    if filter.q.is_some()        { sql.push_str(" AND (sku LIKE ? OR name LIKE ?)"); }
    sql.push_str(" ORDER BY sku LIMIT ? OFFSET ?");

    let mut q = sqlx::query(&sql);
    if let Some(v) = &filter.category { q = q.bind(v); }
    if let Some(v) = &filter.supplier { q = q.bind(v); }
    if let Some(v) = &filter.barcode  { q = q.bind(v); }
    if let Some(v) = &filter.q {
        let pat = format!("%{}%", v);
        q = q.bind(pat.clone()).bind(pat);
    }
    q = q.bind(limit).bind(offset);
    let rows = q.fetch_all(&state.pool).await?;

    let mut items: Vec<Item> = rows.into_iter().map(row_to_item).collect();
    let stock = load_all_stock(&state.pool, items.iter().map(|i| i.id.as_str())).await?;
    for it in items.iter_mut() {
        it.loc = stock.get(&it.id).cloned().unwrap_or_default();
    }
    Ok(Json(items))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Item>> {
    let row = sqlx::query(
        "SELECT id, sku, name, category, brand, supplier_id, cost, price, unit, \
         min_qty, max_qty, qty, allocated, barcode, variants, lots, tags, img, updated \
         FROM items WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let mut item = row_to_item(row);
    item.loc = load_stock_for(&state.pool, &id).await?;
    Ok(Json(item))
}

async fn create(
    State(state): State<AppState>,
    Json(input): Json<ItemInput>,
) -> ApiResult<(StatusCode, Json<Item>)> {
    validate(&input)?;
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| format!("I-{}", uuid::Uuid::new_v4().simple().to_string()[..8].to_string()));

    let mut tx = state.pool.begin().await?;
    insert_item(&mut tx, &id, &input).await?;
    write_stock(&mut tx, &id, &input.loc).await?;
    record_in_tx(
        &mut tx,
        AuditEvent {
            user: "api",
            kind: "item.create",
            r#ref: Some(&id),
            description: &format!("Created item {} ({})", input.name, input.sku),
            payload: None,
        },
    )
    .await?;
    tx.commit().await?;

    let mut item = input_to_item(&id, &input);
    item.loc = input.loc;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<ItemInput>,
) -> ApiResult<Json<Item>> {
    validate(&input)?;
    let mut tx = state.pool.begin().await?;

    let res = sqlx::query(
        r#"UPDATE items SET sku = ?, name = ?, category = ?, brand = ?, supplier_id = ?,
                cost = ?, price = ?, unit = ?, min_qty = ?, max_qty = ?, qty = ?,
                allocated = ?, barcode = ?, variants = ?, lots = ?, tags = ?,
                img = ?, updated = ?, updated_at = datetime('now')
           WHERE id = ?"#,
    )
    .bind(&input.sku)
    .bind(&input.name)
    .bind(&input.category)
    .bind(&input.brand)
    .bind(&input.supplier)
    .bind(input.cost)
    .bind(input.price)
    .bind(&input.unit)
    .bind(input.min)
    .bind(input.max)
    .bind(input.qty)
    .bind(input.allocated)
    .bind(&input.barcode)
    .bind(input.variants.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default()))
    .bind(input.lots.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default()))
    .bind(serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".into()))
    .bind(&input.img)
    .bind(chrono::Utc::now().date_naive().to_string())
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(map_unique)?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    sqlx::query("DELETE FROM item_stock WHERE item_id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    write_stock(&mut tx, &id, &input.loc).await?;

    record_in_tx(
        &mut tx,
        AuditEvent {
            user: "api",
            kind: "item.update",
            r#ref: Some(&id),
            description: &format!("Updated item {}", input.sku),
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
    let res = sqlx::query("DELETE FROM items WHERE id = ?")
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
            kind: "item.delete",
            r#ref: Some(&id),
            description: &format!("Deleted item {}", id),
            payload: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- helpers ------------------------------------------------------------

fn validate(input: &ItemInput) -> ApiResult<()> {
    if input.sku.trim().is_empty() || input.name.trim().is_empty() {
        return Err(ApiError::BadRequest("sku and name are required".into()));
    }
    if input.qty < 0 || input.allocated < 0 {
        return Err(ApiError::BadRequest("qty and allocated must be ≥ 0".into()));
    }
    Ok(())
}

async fn insert_item<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Sqlite>,
    id: &str,
    input: &ItemInput,
) -> ApiResult<()> {
    sqlx::query(
        r#"INSERT INTO items
           (id, sku, name, category, brand, supplier_id, cost, price, unit,
            min_qty, max_qty, qty, allocated, barcode, variants, lots, tags, img, updated)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(id)
    .bind(&input.sku)
    .bind(&input.name)
    .bind(&input.category)
    .bind(&input.brand)
    .bind(&input.supplier)
    .bind(input.cost)
    .bind(input.price)
    .bind(&input.unit)
    .bind(input.min)
    .bind(input.max)
    .bind(input.qty)
    .bind(input.allocated)
    .bind(&input.barcode)
    .bind(input.variants.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default()))
    .bind(input.lots.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default()))
    .bind(serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".into()))
    .bind(&input.img)
    .bind(chrono::Utc::now().date_naive().to_string())
    .execute(&mut **tx)
    .await
    .map_err(map_unique)?;
    Ok(())
}

async fn write_stock<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Sqlite>,
    item_id: &str,
    lines: &[StockLine],
) -> ApiResult<()> {
    for line in lines {
        let serials = line
            .serial
            .as_ref()
            .map(|s| serde_json::to_string(s).unwrap_or_default());
        sqlx::query(
            "INSERT INTO item_stock (item_id, location_id, bin, qty, serials)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(item_id)
        .bind(&line.l)
        .bind(&line.b)
        .bind(line.q)
        .bind(serials)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn load_stock_for(pool: &sqlx::SqlitePool, id: &str) -> ApiResult<Vec<StockLine>> {
    let rows = sqlx::query(
        "SELECT location_id, bin, qty, serials FROM item_stock WHERE item_id = ?",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| StockLine {
            l: r.get("location_id"),
            b: r.try_get::<Option<String>, _>("bin").ok().flatten().unwrap_or_default(),
            q: r.try_get("qty").unwrap_or(0),
            serial: r
                .try_get::<Option<String>, _>("serials")
                .ok()
                .flatten()
                .and_then(|s| serde_json::from_str(&s).ok()),
        })
        .collect())
}

async fn load_all_stock<'a>(
    pool: &sqlx::SqlitePool,
    ids: impl Iterator<Item = &'a str>,
) -> ApiResult<std::collections::HashMap<String, Vec<StockLine>>> {
    let id_list: Vec<String> = ids.map(|s| s.to_string()).collect();
    if id_list.is_empty() {
        return Ok(Default::default());
    }
    // Build placeholders for an IN clause; SQLite doesn't take arrays directly.
    let placeholders = std::iter::repeat("?")
        .take(id_list.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT item_id, location_id, bin, qty, serials FROM item_stock WHERE item_id IN ({})",
        placeholders
    );
    let mut q = sqlx::query(&sql);
    for id in &id_list {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;
    let mut out: std::collections::HashMap<String, Vec<StockLine>> = std::collections::HashMap::new();
    for r in rows {
        let item_id: String = r.get("item_id");
        out.entry(item_id).or_default().push(StockLine {
            l: r.get("location_id"),
            b: r.try_get::<Option<String>, _>("bin").ok().flatten().unwrap_or_default(),
            q: r.try_get("qty").unwrap_or(0),
            serial: r
                .try_get::<Option<String>, _>("serials")
                .ok()
                .flatten()
                .and_then(|s| serde_json::from_str(&s).ok()),
        });
    }
    Ok(out)
}

fn row_to_item(r: sqlx::sqlite::SqliteRow) -> Item {
    Item {
        id: r.get("id"),
        sku: r.get("sku"),
        name: r.get("name"),
        category: r.get("category"),
        brand: r.try_get("brand").ok().flatten(),
        supplier: r.try_get("supplier_id").ok().flatten(),
        cost: r.try_get("cost").unwrap_or(0.0),
        price: r.try_get("price").unwrap_or(0.0),
        unit: r.try_get("unit").unwrap_or_else(|_| "ea".into()),
        min: r.try_get("min_qty").unwrap_or(0),
        max: r.try_get("max_qty").unwrap_or(0),
        qty: r.try_get("qty").unwrap_or(0),
        allocated: r.try_get("allocated").unwrap_or(0),
        barcode: r.try_get("barcode").ok().flatten(),
        loc: Vec::new(),
        variants: r
            .try_get::<Option<String>, _>("variants")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok()),
        lots: r
            .try_get::<Option<String>, _>("lots")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok()),
        tags: r
            .try_get::<String, _>("tags")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        updated: r.try_get("updated").ok().flatten(),
        img: r.try_get("img").ok().flatten(),
    }
}

fn input_to_item(id: &str, input: &ItemInput) -> Item {
    Item {
        id: id.to_string(),
        sku: input.sku.clone(),
        name: input.name.clone(),
        category: input.category.clone(),
        brand: input.brand.clone(),
        supplier: input.supplier.clone(),
        cost: input.cost,
        price: input.price,
        unit: input.unit.clone(),
        min: input.min,
        max: input.max,
        qty: input.qty,
        allocated: input.allocated,
        barcode: input.barcode.clone(),
        loc: Vec::new(),
        variants: input.variants.clone(),
        lots: input.lots.clone(),
        tags: input.tags.clone(),
        updated: Some(chrono::Utc::now().date_naive().to_string()),
        img: input.img.clone(),
    }
}

fn map_unique(e: sqlx::Error) -> ApiError {
    match e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ApiError::Conflict("sku already exists".into())
        }
        other => ApiError::Database(other),
    }
}
