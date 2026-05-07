//! Reorder-point forecasting and draft-PO generation.
//!
//! The simplest useful forecasting for a homelab catalog: walk the
//! items table, find anything below its configured `min` threshold,
//! and propose a draft purchase order grouped by supplier with
//! enough quantity to refill to `max`. Optionally weights by recent
//! consumption to bump urgency for fast movers.
//!
//! No ML, no external services. The activity log gives us enough
//! signal to compute days-to-stockout via simple linear consumption
//! over a configurable trailing window.

use crate::models::{Item, PurchaseLine};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Trailing days used to estimate consumption rate. Configurable per
/// caller via `?lookback=`. Default leans long because homelab
/// volumes are bursty.
pub const DEFAULT_LOOKBACK_DAYS: i64 = 90;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderSuggestion {
    pub item_id: String,
    pub sku: String,
    pub name: String,
    pub supplier: Option<String>,
    pub qty_on_hand: i64,
    pub min: i64,
    pub max: i64,
    pub reorder_qty: i64,
    /// Estimated daily consumption from the last `lookback` days of
    /// pick / shipment activity, or 0.0 if we have no signal.
    pub avg_daily_consumption: f64,
    /// Estimated days until `qty_on_hand` reaches zero at the
    /// observed rate. `None` when consumption is zero.
    pub days_to_stockout: Option<f64>,
    /// `urgent` if already below min AND projected stockout within
    /// the supplier's lead time; `low` otherwise.
    pub urgency: Urgency,
    pub estimated_unit_cost: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Urgent,
    Low,
}

#[derive(Debug, Clone, Serialize)]
pub struct DraftPo {
    /// Supplier id (or None for items with no supplier; those get
    /// grouped under a single anonymous draft).
    pub supplier_id: Option<String>,
    pub total: f64,
    pub lines: Vec<PurchaseLine>,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForecastReport {
    pub lookback_days: i64,
    pub items_below_min: usize,
    pub urgent: usize,
    pub suggestions: Vec<ReorderSuggestion>,
    pub draft_pos: Vec<DraftPo>,
}

pub async fn reorder_report(pool: &SqlitePool, lookback_days: i64) -> sqlx::Result<ForecastReport> {
    let lookback_days = lookback_days.clamp(7, 365);

    // Items needing attention: at-or-below min OR out of stock.
    // Pull every column we need to build the suggestion plus the
    // supplier's lead_time to decide urgency.
    let rows: Vec<(
        String,         // id
        String,         // sku
        String,         // name
        Option<String>, // supplier
        i64,            // min_qty
        i64,            // max_qty
        i64,            // qty
        f64,            // cost
        Option<i64>,    // supplier lead_time
    )> = sqlx::query_as(
        r#"SELECT i.id, i.sku, i.name, i.supplier_id, i.min_qty, i.max_qty, i.qty,
                  i.cost, s.lead_time
           FROM items i
           LEFT JOIN suppliers s ON s.id = i.supplier_id
           WHERE i.qty < i.min_qty OR i.qty = 0
           ORDER BY i.qty - i.min_qty ASC"#,
    )
    .fetch_all(pool)
    .await?;

    // One pass over recent pick / shipment activity to build a
    // sku → consumption map. Activity descriptions are free text
    // (e.g. "Picked 2× Cat6A 7ft Blue …") but the audit log carries
    // the SO id in `ref`, so we join through sales_order_lines for
    // a ground-truth count.
    let consumption_rows: Vec<(String, i64)> = sqlx::query_as(
        r#"SELECT sol.sku, COALESCE(SUM(sol.qty), 0) AS total_picked
           FROM sales_order_lines sol
           JOIN sales_orders so ON so.id = sol.so_id
           WHERE so.status IN ('shipped', 'picking')
             AND so.created >= date('now', ? || ' days')
           GROUP BY sol.sku"#,
    )
    .bind(format!("-{}", lookback_days))
    .fetch_all(pool)
    .await?;

    let mut consumption: std::collections::HashMap<String, f64> = Default::default();
    for (sku, total) in consumption_rows {
        let per_day = total as f64 / lookback_days as f64;
        consumption.insert(sku, per_day);
    }

    let mut suggestions: Vec<ReorderSuggestion> = Vec::with_capacity(rows.len());
    for (id, sku, name, supplier, min, max, qty, cost, lead_time) in rows {
        let reorder_qty = (max - qty).max(min - qty).max(1);
        let rate = *consumption.get(&sku).unwrap_or(&0.0);
        let days_to_stockout = if rate > f64::EPSILON {
            Some((qty.max(0) as f64) / rate)
        } else {
            None
        };
        // Urgent if already at zero, OR projected stockout falls
        // within the supplier's lead time. Default lead time of
        // 7 days when the supplier is unknown.
        let lead_time = lead_time.unwrap_or(7) as f64;
        let urgency = if qty == 0 {
            Urgency::Urgent
        } else if days_to_stockout.map(|d| d <= lead_time).unwrap_or(false) {
            Urgency::Urgent
        } else {
            Urgency::Low
        };
        suggestions.push(ReorderSuggestion {
            item_id: id,
            sku,
            name,
            supplier,
            qty_on_hand: qty,
            min,
            max,
            reorder_qty,
            avg_daily_consumption: rate,
            days_to_stockout,
            urgency,
            estimated_unit_cost: cost,
        });
    }

    let urgent = suggestions
        .iter()
        .filter(|s| s.urgency == Urgency::Urgent)
        .count();

    // Group by supplier into draft POs.
    let mut by_supplier: std::collections::HashMap<Option<String>, DraftPo> = Default::default();
    for s in &suggestions {
        let entry = by_supplier
            .entry(s.supplier.clone())
            .or_insert(DraftPo {
                supplier_id: s.supplier.clone(),
                total: 0.0,
                lines: Vec::new(),
                item_ids: Vec::new(),
            });
        let line_total = (s.reorder_qty as f64) * s.estimated_unit_cost;
        entry.total += line_total;
        entry.lines.push(PurchaseLine {
            sku: s.sku.clone(),
            qty: s.reorder_qty,
            cost: s.estimated_unit_cost,
        });
        entry.item_ids.push(s.item_id.clone());
    }
    let mut draft_pos: Vec<DraftPo> = by_supplier.into_values().collect();
    // Stable order: known suppliers first by id, anonymous last.
    draft_pos.sort_by(|a, b| match (&a.supplier_id, &b.supplier_id) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    Ok(ForecastReport {
        lookback_days,
        items_below_min: suggestions.len(),
        urgent,
        suggestions,
        draft_pos,
    })
}

/// For a fresh `Item`, compute the projected days to stockout on the
/// fly. Used inline by the dashboard rather than going through the
/// full report.
#[allow(dead_code)]
pub fn quick_days_to_stockout(item: &Item, recent_consumption_per_day: f64) -> Option<f64> {
    if recent_consumption_per_day <= f64::EPSILON {
        return None;
    }
    Some((item.qty.max(0) as f64) / recent_consumption_per_day)
}
