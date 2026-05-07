//! /api/bootstrap — single round-trip dashboard hydration payload.
//!
//! The frontend (originally a static prototype with seeded JS data) calls
//! this once on load, then mirrors the response onto window globals so the
//! existing views keep rendering unchanged. Putting everything in one
//! response keeps cold-start fast and avoids ten parallel fetches.

use crate::db::RowExt;
use crate::error::ApiResult;
use crate::models::*;
use crate::state::AppState;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(bootstrap))
}

#[derive(Serialize)]
pub struct Bootstrap {
    pub locations: Vec<Location>,
    pub suppliers: Vec<Supplier>,
    pub items: Vec<Item>,
    #[serde(rename = "purchaseOrders")]
    pub purchase_orders: Vec<PurchaseOrder>,
    #[serde(rename = "salesOrders")]
    pub sales_orders: Vec<SalesOrder>,
    pub transfers: Vec<Transfer>,
    pub counts: Vec<Count>,
    pub activity: Vec<ActivityEntry>,
    pub status: StatusSummary,
}

async fn bootstrap(State(state): State<AppState>) -> ApiResult<Json<Bootstrap>> {
    Ok(Json(snapshot(&state.pool).await?))
}

/// Build the dashboard hydration snapshot. Exported so the web shell can
/// inject it into the HTML at render time, avoiding a second round trip.
pub async fn snapshot(pool: &sqlx::SqlitePool) -> ApiResult<Bootstrap> {
    let locations = read_locations(pool).await?;
    let suppliers = read_suppliers(pool).await?;
    let items = read_items(pool).await?;
    let purchase_orders = read_pos(pool).await?;
    let sales_orders = read_sos(pool).await?;
    let transfers = read_transfers(pool).await?;
    let counts = read_counts(pool).await?;
    let activity = crate::audit::recent(pool, 200).await?;
    let status = read_status(pool).await?;

    Ok(Bootstrap {
        locations,
        suppliers,
        items,
        purchase_orders,
        sales_orders,
        transfers,
        counts,
        activity,
        status,
    })
}

async fn read_locations(pool: &sqlx::SqlitePool) -> ApiResult<Vec<Location>> {
    let rows =
        sqlx::query("SELECT id, code, name, type, parent, bins FROM locations ORDER BY code")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|r| Location {
            id: r.get("id"),
            code: r.get("code"),
            name: r.get("name"),
            kind: r.get("type"),
            parent: r.opt_string("parent"),
            bins: serde_json::from_str(&r.get::<String, _>("bins")).unwrap_or_default(),
        })
        .collect())
}

async fn read_suppliers(pool: &sqlx::SqlitePool) -> ApiResult<Vec<Supplier>> {
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
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Supplier {
            id: r.get("id"),
            code: r.get("code"),
            name: r.get("name"),
            contact: r.opt_string("contact"),
            lead_time: r.opt_i64("lead_time"),
            rating: r.opt_f64("rating"),
            open_pos: r.i64_or("open_pos", 0),
            total_spend: r.f64_or("total_spend", 0.0),
        })
        .collect())
}

async fn read_items(pool: &sqlx::SqlitePool) -> ApiResult<Vec<Item>> {
    let rows = sqlx::query(&format!(
        "SELECT {} FROM items ORDER BY sku",
        crate::routes::items::ITEM_COLUMNS,
    ))
    .fetch_all(pool)
    .await?;
    // Reuse the canonical row → Item mapper so a column rename has
    // exactly one site to follow.
    let mut items: Vec<Item> = rows
        .into_iter()
        .map(crate::routes::items::row_to_item)
        .collect();

    let stock_rows = sqlx::query("SELECT item_id, location_id, bin, qty, serials FROM item_stock")
        .fetch_all(pool)
        .await?;
    let mut stock: std::collections::HashMap<String, Vec<StockLine>> = Default::default();
    for r in stock_rows {
        let item_id: String = r.get("item_id");
        stock.entry(item_id).or_default().push(StockLine {
            l: r.get("location_id"),
            b: r.string_or_default("bin"),
            q: r.i64_or("qty", 0),
            serial: r
                .try_get::<Option<String>, _>("serials")
                .ok()
                .flatten()
                .and_then(|s| serde_json::from_str(&s).ok()),
        });
    }
    for it in items.iter_mut() {
        it.loc = stock.remove(&it.id).unwrap_or_default();
    }
    Ok(items)
}

async fn read_pos(pool: &sqlx::SqlitePool) -> ApiResult<Vec<PurchaseOrder>> {
    let rows = sqlx::query(
        "SELECT id, supplier_id, status, created, expected, received, total \
         FROM purchase_orders ORDER BY created DESC",
    )
    .fetch_all(pool)
    .await?;
    let mut out: Vec<PurchaseOrder> = rows
        .into_iter()
        .map(|r| PurchaseOrder {
            id: r.get("id"),
            supplier: r.opt_string("supplier_id"),
            status: r.get("status"),
            created: r.get("created"),
            expected: r.opt_string("expected"),
            received: r.opt_string("received"),
            total: r.f64_or("total", 0.0),
            lines: Vec::new(),
        })
        .collect();
    let lines = sqlx::query("SELECT po_id, sku, qty, cost FROM purchase_order_lines")
        .fetch_all(pool)
        .await?;
    let mut by: std::collections::HashMap<String, Vec<PurchaseLine>> = Default::default();
    for r in lines {
        let id: String = r.get("po_id");
        by.entry(id).or_default().push(PurchaseLine {
            sku: r.get("sku"),
            qty: r.i64_or("qty", 0),
            cost: r.f64_or("cost", 0.0),
        });
    }
    for po in out.iter_mut() {
        po.lines = by.remove(&po.id).unwrap_or_default();
    }
    Ok(out)
}

async fn read_sos(pool: &sqlx::SqlitePool) -> ApiResult<Vec<SalesOrder>> {
    let rows = sqlx::query(
        "SELECT id, project, status, priority, created FROM sales_orders ORDER BY created DESC",
    )
    .fetch_all(pool)
    .await?;
    let mut out: Vec<SalesOrder> = rows
        .into_iter()
        .map(|r| SalesOrder {
            id: r.get("id"),
            proj: r.opt_string("project"),
            status: r.get("status"),
            created: r.get("created"),
            priority: r.opt_string("priority"),
            lines: Vec::new(),
        })
        .collect();
    let lines = sqlx::query("SELECT so_id, sku, qty FROM sales_order_lines")
        .fetch_all(pool)
        .await?;
    let mut by: std::collections::HashMap<String, Vec<SalesLine>> = Default::default();
    for r in lines {
        let id: String = r.get("so_id");
        by.entry(id).or_default().push(SalesLine {
            sku: r.get("sku"),
            qty: r.i64_or("qty", 0),
        });
    }
    for so in out.iter_mut() {
        so.lines = by.remove(&so.id).unwrap_or_default();
    }
    Ok(out)
}

async fn read_transfers(pool: &sqlx::SqlitePool) -> ApiResult<Vec<Transfer>> {
    let rows =
        sqlx::query("SELECT id, from_loc, to_loc, date, status FROM transfers ORDER BY date DESC")
            .fetch_all(pool)
            .await?;
    let mut out: Vec<Transfer> = rows
        .into_iter()
        .map(|r| Transfer {
            id: r.get("id"),
            from: r.opt_string("from_loc"),
            to: r.opt_string("to_loc"),
            date: r.get("date"),
            status: r.get("status"),
            lines: Vec::new(),
        })
        .collect();
    let lines = sqlx::query("SELECT transfer_id, sku, qty FROM transfer_lines")
        .fetch_all(pool)
        .await?;
    let mut by: std::collections::HashMap<String, Vec<TransferLine>> = Default::default();
    for r in lines {
        let id: String = r.get("transfer_id");
        by.entry(id).or_default().push(TransferLine {
            sku: r.get("sku"),
            qty: r.i64_or("qty", 0),
        });
    }
    for tr in out.iter_mut() {
        tr.lines = by.remove(&tr.id).unwrap_or_default();
    }
    Ok(out)
}

async fn read_counts(pool: &sqlx::SqlitePool) -> ApiResult<Vec<Count>> {
    let rows = sqlx::query(
        "SELECT id, location_id, date, status, counted, variance, by_user \
         FROM counts ORDER BY date DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Count {
            id: r.get("id"),
            loc: r.opt_string("location_id"),
            date: r.get("date"),
            status: r.get("status"),
            counted: r.i64_or("counted", 0),
            variance: r.i64_or("variance", 0),
            by: r.opt_string("by_user"),
        })
        .collect())
}

async fn read_status(pool: &sqlx::SqlitePool) -> ApiResult<StatusSummary> {
    let total_skus: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
        .fetch_one(pool)
        .await?;
    let total_units: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(qty), 0) FROM items")
        .fetch_one(pool)
        .await?;
    let total_value: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(qty * cost), 0) AS REAL) FROM items")
            .fetch_one(pool)
            .await?;
    let low_stock: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE qty > 0 AND qty < min_qty")
            .fetch_one(pool)
            .await?;
    let out_of_stock: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE qty = 0")
        .fetch_one(pool)
        .await?;
    let serialized_units: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(json_array_length(serials)), 0) FROM item_stock WHERE serials IS NOT NULL",
    ).fetch_one(pool).await.unwrap_or(0);
    let open_pos: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM purchase_orders WHERE status NOT IN ('received', 'cancelled')",
    )
    .fetch_one(pool)
    .await?;
    let open_sos: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sales_orders WHERE status IN ('open', 'picking')")
            .fetch_one(pool)
            .await?;
    let pending_transfers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM transfers WHERE status = 'pending'")
            .fetch_one(pool)
            .await?;
    let scheduled_counts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM counts WHERE status IN ('scheduled', 'open')")
            .fetch_one(pool)
            .await?;
    Ok(StatusSummary {
        total_skus,
        total_units,
        total_value: total_value.round(),
        low_stock,
        out_of_stock,
        serialized_units,
        open_pos,
        open_sos,
        pending_transfers,
        scheduled_counts,
    })
}
