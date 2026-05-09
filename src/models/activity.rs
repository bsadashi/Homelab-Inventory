//! Activity log entries (read-only DTO; writes go through `audit::`).

use serde::{Deserialize, Serialize};

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
