//! Cycle counts and audits.

use serde::{Deserialize, Serialize};

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
