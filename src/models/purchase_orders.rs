//! Purchase orders. Lines live in a child table; the DTO folds them
//! back into a single nested shape on read.

use serde::{Deserialize, Serialize};

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
