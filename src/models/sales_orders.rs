//! Sales / pick orders. In a homelab context these represent parts
//! allocated to a project; the schema is the same as a real SO so a
//! small-business deployment can use it unchanged.

use serde::{Deserialize, Serialize};

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
