//! Suppliers — vendors. `open_pos` is computed at read-time from
//! purchase_orders; the input shape doesn't carry it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Supplier {
    pub id: String,
    pub code: String,
    pub name: String,
    pub contact: Option<String>,
    #[serde(rename = "leadTime")]
    pub lead_time: Option<i64>,
    pub rating: Option<f64>,
    /// Computed at read-time from purchase_orders.
    #[serde(rename = "openPOs", default)]
    pub open_pos: i64,
    #[serde(rename = "totalSpend", default)]
    pub total_spend: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SupplierInput {
    pub id: Option<String>,
    pub code: String,
    pub name: String,
    pub contact: Option<String>,
    #[serde(rename = "leadTime")]
    pub lead_time: Option<i64>,
    pub rating: Option<f64>,
    #[serde(rename = "totalSpend", default)]
    pub total_spend: f64,
}
