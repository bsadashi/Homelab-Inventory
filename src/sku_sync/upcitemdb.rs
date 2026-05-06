//! UPCitemDB free trial endpoint.
//! Docs: https://www.upcitemdb.com/api/explorer
//!
//! No auth, ~100 requests/day per source IP. Best-effort by design.

use super::{http, validate_barcode, LookupError, LookupResult, Provider};
use serde::Deserialize;

const ENDPOINT: &str = "https://api.upcitemdb.com/prod/trial/lookup";

pub struct UpcItemDb {
    base: String,
}

impl UpcItemDb {
    pub fn new() -> Self {
        Self {
            base: std::env::var("RACKLOG_UPCITEMDB_BASE")
                .unwrap_or_else(|_| ENDPOINT.to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    code: Option<String>,
    message: Option<String>,
    items: Option<Vec<ApiItem>>,
}

#[derive(Debug, Deserialize)]
struct ApiItem {
    title: Option<String>,
    brand: Option<String>,
    category: Option<String>,
    description: Option<String>,
    images: Option<Vec<String>>,
    upc: Option<String>,
}

#[async_trait::async_trait]
impl Provider for UpcItemDb {
    fn name(&self) -> &'static str { "upcitemdb" }

    async fn lookup(&self, barcode: &str) -> Result<Vec<LookupResult>, LookupError> {
        validate_barcode(barcode)?;
        let url = format!("{}?upc={}", self.base, barcode);
        let resp = http().get(&url).send().await?;

        // The API returns 200 even for "INVALID_UPC" — read the body to
        // distinguish a real miss from a transport error.
        let body: ApiResponse = resp.json().await?;
        if let Some(code) = body.code.as_deref() {
            if code != "OK" && code != "FOUND" {
                if let Some(msg) = body.message {
                    tracing::debug!(provider = "upcitemdb", code, msg, "no hit");
                }
                return Ok(Vec::new());
            }
        }
        let items = body.items.unwrap_or_default();
        let out = items
            .into_iter()
            .map(|i| {
                let mut r = LookupResult::new(
                    "upcitemdb",
                    i.upc.unwrap_or_else(|| barcode.to_string()),
                );
                r.name = i.title;
                r.brand = i.brand;
                r.category = i.category;
                r.description = i.description;
                r.image_url = i.images.and_then(|imgs| imgs.into_iter().next());
                r
            })
            .collect();
        Ok(out)
    }
}
