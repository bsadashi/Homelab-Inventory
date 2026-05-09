//! First-boot seed loader.
//!
//! Ports the homelab dataset shipped with the original RACKLOG prototype
//! (see `static/data.jsx`) into the relational schema. Runs only when the
//! `items` table is empty, so re-deploying against an existing volume is
//! a no-op. Keep this file focused on data plumbing — the canonical
//! dataset lives in `static/seed/seed.json`, which is built into the
//! binary via include_str! so containers boot without external state.

use crate::audit::{record_in_tx, AuditEvent};
use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;

const EMBEDDED_SEED: &str = include_str!("../static/seed/seed.json");

#[derive(Debug, Deserialize)]
struct Seed {
    locations: Vec<Value>,
    suppliers: Vec<Value>,
    items: Vec<Value>,
    purchase_orders: Vec<Value>,
    sales_orders: Vec<Value>,
    transfers: Vec<Value>,
    counts: Vec<Value>,
    activity: Vec<Value>,
}

pub async fn seed_if_empty(pool: &SqlitePool) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(false);
    }

    let seed: Seed = serde_json::from_str(EMBEDDED_SEED)?;
    let mut tx = pool.begin().await?;

    for loc in &seed.locations {
        let bins = loc
            .get("bins")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "[]".to_string());
        sqlx::query(
            "INSERT INTO locations (id, code, name, type, parent, bins) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(s(loc, "id"))
        .bind(s(loc, "code"))
        .bind(s(loc, "name"))
        .bind(s(loc, "type"))
        .bind(opt_s(loc, "parent"))
        .bind(bins)
        .execute(&mut *tx)
        .await?;
    }

    for v in &seed.suppliers {
        sqlx::query(
            r#"INSERT INTO suppliers
               (id, code, name, contact, lead_time, rating, total_spend)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(s(v, "id"))
        .bind(s(v, "code"))
        .bind(s(v, "name"))
        .bind(opt_s(v, "contact"))
        .bind(opt_i(v, "leadTime"))
        .bind(opt_f(v, "rating"))
        .bind(opt_f(v, "totalSpend").unwrap_or(0.0))
        .execute(&mut *tx)
        .await?;
    }

    for it in &seed.items {
        sqlx::query(
            r#"INSERT INTO items
               (id, sku, name, category, brand, supplier_id,
                cost, price, unit, min_qty, max_qty, qty, allocated,
                barcode, variants, lots, tags, img, updated)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(s(it, "id"))
        .bind(s(it, "sku"))
        .bind(s(it, "name"))
        .bind(s(it, "cat"))
        .bind(opt_s(it, "brand"))
        .bind(opt_s(it, "supplier"))
        .bind(opt_f(it, "cost").unwrap_or(0.0))
        .bind(opt_f(it, "price").unwrap_or(0.0))
        .bind(opt_s(it, "unit").unwrap_or_else(|| "ea".into()))
        .bind(opt_i(it, "min").unwrap_or(0))
        .bind(opt_i(it, "max").unwrap_or(0))
        .bind(opt_i(it, "qty").unwrap_or(0))
        .bind(opt_i(it, "allocated").unwrap_or(0))
        .bind(opt_s(it, "barcode"))
        .bind(
            it.get("variants")
                .filter(|v| !v.is_null())
                .map(|v| v.to_string()),
        )
        .bind(
            it.get("lots")
                .filter(|v| !v.is_null())
                .map(|v| v.to_string()),
        )
        .bind(
            it.get("tags")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "[]".into()),
        )
        .bind(opt_s(it, "img"))
        .bind(opt_s(it, "updated"))
        .execute(&mut *tx)
        .await?;

        if let Some(loc_arr) = it.get("loc").and_then(|v| v.as_array()) {
            for line in loc_arr {
                let serials = line
                    .get("serial")
                    .filter(|v| !v.is_null())
                    .map(|v| v.to_string());
                sqlx::query(
                    "INSERT INTO item_stock (item_id, location_id, bin, qty, serials)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(s(it, "id"))
                .bind(s(line, "l"))
                .bind(opt_s(line, "b"))
                .bind(opt_i(line, "q").unwrap_or(0))
                .bind(serials)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    for po in &seed.purchase_orders {
        sqlx::query(
            r#"INSERT INTO purchase_orders
               (id, supplier_id, status, created, expected, received, total)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(s(po, "id"))
        .bind(opt_s(po, "supplier"))
        .bind(s(po, "status"))
        .bind(s(po, "created"))
        .bind(opt_s(po, "expected"))
        .bind(opt_s(po, "received"))
        .bind(opt_f(po, "total").unwrap_or(0.0))
        .execute(&mut *tx)
        .await?;
        if let Some(lines) = po.get("lines").and_then(|v| v.as_array()) {
            for line in lines {
                sqlx::query(
                    "INSERT INTO purchase_order_lines (po_id, sku, qty, cost) VALUES (?, ?, ?, ?)",
                )
                .bind(s(po, "id"))
                .bind(s(line, "sku"))
                .bind(opt_i(line, "qty").unwrap_or(0))
                .bind(opt_f(line, "cost").unwrap_or(0.0))
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    for so in &seed.sales_orders {
        sqlx::query(
            r#"INSERT INTO sales_orders (id, project, status, priority, created)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(s(so, "id"))
        .bind(opt_s(so, "proj"))
        .bind(s(so, "status"))
        .bind(opt_s(so, "priority"))
        .bind(s(so, "created"))
        .execute(&mut *tx)
        .await?;
        if let Some(lines) = so.get("lines").and_then(|v| v.as_array()) {
            for line in lines {
                sqlx::query("INSERT INTO sales_order_lines (so_id, sku, qty) VALUES (?, ?, ?)")
                    .bind(s(so, "id"))
                    .bind(s(line, "sku"))
                    .bind(opt_i(line, "qty").unwrap_or(0))
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }

    for tr in &seed.transfers {
        sqlx::query(
            r#"INSERT INTO transfers (id, from_loc, to_loc, date, status)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(s(tr, "id"))
        .bind(opt_s(tr, "from"))
        .bind(opt_s(tr, "to"))
        .bind(s(tr, "date"))
        .bind(s(tr, "status"))
        .execute(&mut *tx)
        .await?;
        if let Some(lines) = tr.get("lines").and_then(|v| v.as_array()) {
            for line in lines {
                sqlx::query("INSERT INTO transfer_lines (transfer_id, sku, qty) VALUES (?, ?, ?)")
                    .bind(s(tr, "id"))
                    .bind(s(line, "sku"))
                    .bind(opt_i(line, "qty").unwrap_or(0))
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }

    for c in &seed.counts {
        sqlx::query(
            r#"INSERT INTO counts (id, location_id, date, status, counted, variance, by_user)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(s(c, "id"))
        .bind(opt_s(c, "loc"))
        .bind(s(c, "date"))
        .bind(s(c, "status"))
        .bind(opt_i(c, "counted").unwrap_or(0))
        .bind(opt_i(c, "variance").unwrap_or(0))
        .bind(opt_s(c, "by"))
        .execute(&mut *tx)
        .await?;
    }

    // Replay activity events through the audit chain so the seeded log is
    // already cryptographically anchored.
    for a in &seed.activity {
        let user = opt_s(a, "user").unwrap_or_else(|| "system".into());
        let kind = opt_s(a, "type").unwrap_or_else(|| "seed".into());
        let r#ref = opt_s(a, "ref");
        let desc = opt_s(a, "desc").unwrap_or_default();
        record_in_tx(
            &mut tx,
            AuditEvent {
                user: &user,
                kind: &kind,
                r#ref: r#ref.as_deref(),
                description: &desc,
                payload: None,
            },
        )
        .await?;
    }

    record_in_tx(
        &mut tx,
        AuditEvent {
            user: "system",
            kind: "seed",
            r#ref: None,
            description: "Initial dataset seeded into empty database",
            payload: None,
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}
fn opt_s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}
fn opt_i(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| x.as_i64())
}
fn opt_f(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(|x| x.as_f64())
}
