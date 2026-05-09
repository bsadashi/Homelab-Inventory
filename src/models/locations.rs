//! Locations — physical hierarchy (rack → shelf → bin).

use serde::{Deserialize, Serialize};

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
