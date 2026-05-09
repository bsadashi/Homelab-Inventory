//! Aggregate dashboard KPIs returned by `/api/stats`.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct StatusSummary {
    #[serde(rename = "totalSKUs")]
    pub total_skus: i64,
    #[serde(rename = "totalUnits")]
    pub total_units: i64,
    #[serde(rename = "totalValue")]
    pub total_value: f64,
    #[serde(rename = "lowStock")]
    pub low_stock: i64,
    #[serde(rename = "outOfStock")]
    pub out_of_stock: i64,
    #[serde(rename = "serializedUnits")]
    pub serialized_units: i64,
    #[serde(rename = "openPOs")]
    pub open_pos: i64,
    #[serde(rename = "openSOs")]
    pub open_sos: i64,
    #[serde(rename = "pendingTransfers")]
    pub pending_transfers: i64,
    #[serde(rename = "scheduledCounts")]
    pub scheduled_counts: i64,
}
