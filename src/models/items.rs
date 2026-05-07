//! Items — the canonical catalog DTO plus its child shapes
//! (per-bin stock lines, variants, lots).

use serde::{Deserialize, Serialize};

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
