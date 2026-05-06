//! Domain DTOs. Field names are deliberately picked to match the JSON
//! shapes the RACKLOG frontend already consumes (see `static/data.jsx`),
//! so the API and the prototype agree without a translation layer.

use serde::{Deserialize, Serialize};

// ---- Locations ----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub id: String,
    pub code: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub bins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocationInput {
    pub id: Option<String>,
    pub code: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub parent: Option<String>,
    #[serde(default)]
    pub bins: Vec<String>,
}

// ---- Suppliers ----------------------------------------------------------

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

// ---- Items --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockLine {
    /// Location id.
    pub l: String,
    /// Bin code within the location.
    pub b: String,
    /// On-hand quantity at this bin.
    pub q: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    #[serde(default)]
    pub q: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lot {
    pub lot: String,
    #[serde(default)]
    pub exp: Option<String>,
    #[serde(default)]
    pub q: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub sku: String,
    pub name: String,
    #[serde(rename = "cat")]
    pub category: String,
    pub brand: Option<String>,
    pub supplier: Option<String>,
    pub cost: f64,
    pub price: f64,
    pub unit: String,
    pub min: i64,
    pub max: i64,
    pub qty: i64,
    pub allocated: i64,
    pub barcode: Option<String>,
    #[serde(default)]
    pub loc: Vec<StockLine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<Variant>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lots: Option<Vec<Lot>>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub img: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemInput {
    pub id: Option<String>,
    pub sku: String,
    pub name: String,
    #[serde(rename = "cat")]
    pub category: String,
    pub brand: Option<String>,
    pub supplier: Option<String>,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub price: f64,
    #[serde(default = "default_unit")]
    pub unit: String,
    #[serde(default)]
    pub min: i64,
    #[serde(default)]
    pub max: i64,
    #[serde(default)]
    pub qty: i64,
    #[serde(default)]
    pub allocated: i64,
    pub barcode: Option<String>,
    #[serde(default)]
    pub loc: Vec<StockLine>,
    pub variants: Option<Vec<Variant>>,
    pub lots: Option<Vec<Lot>>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub img: Option<String>,
}

fn default_unit() -> String {
    "ea".to_string()
}

// ---- Purchase orders ----------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseLine {
    pub sku: String,
    pub qty: i64,
    #[serde(default)]
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseOrder {
    pub id: String,
    pub supplier: Option<String>,
    pub status: String,
    pub created: String,
    pub expected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub received: Option<String>,
    pub total: f64,
    pub lines: Vec<PurchaseLine>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PurchaseOrderInput {
    pub id: Option<String>,
    pub supplier: Option<String>,
    pub status: String,
    pub created: String,
    pub expected: Option<String>,
    pub received: Option<String>,
    #[serde(default)]
    pub total: f64,
    pub lines: Vec<PurchaseLine>,
}

// ---- Sales / pick orders -----------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesLine {
    pub sku: String,
    pub qty: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesOrder {
    pub id: String,
    pub proj: Option<String>,
    pub status: String,
    pub created: String,
    pub priority: Option<String>,
    pub lines: Vec<SalesLine>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SalesOrderInput {
    pub id: Option<String>,
    pub proj: Option<String>,
    pub status: String,
    pub created: String,
    pub priority: Option<String>,
    pub lines: Vec<SalesLine>,
}

// ---- Transfers ----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferLine {
    pub sku: String,
    pub qty: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub id: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub date: String,
    pub status: String,
    pub lines: Vec<TransferLine>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransferInput {
    pub id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub date: String,
    pub status: String,
    pub lines: Vec<TransferLine>,
}

// ---- Cycle counts -------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Count {
    pub id: String,
    pub loc: Option<String>,
    pub date: String,
    pub status: String,
    pub counted: i64,
    pub variance: i64,
    pub by: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CountInput {
    pub id: Option<String>,
    pub loc: Option<String>,
    pub date: String,
    pub status: String,
    #[serde(default)]
    pub counted: i64,
    #[serde(default)]
    pub variance: i64,
    pub by: Option<String>,
}

// ---- Activity log -------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub ts: String,
    pub user: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    pub desc: String,
    /// Hex SHA-256 for the chain. Present on reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "prevHash")]
    pub prev_hash: Option<String>,
}

// ---- Aggregate dashboard status ----------------------------------------

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
