//! Stock transfers — bin-to-bin movement records.

use serde::{Deserialize, Serialize};

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
