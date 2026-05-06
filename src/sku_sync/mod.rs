//! External SKU lookup. Provides a normalised `LookupResult` over a
//! handful of free product-data providers. Providers are configured
//! via env vars and only enabled when their credentials are present —
//! a vanilla deployment runs with zero external calls.
//!
//! Privacy note: nothing in this module fires unless the operator
//! explicitly sets a provider's env vars. The frontend's `Lookup`
//! button is the only path that calls into here, and the request body
//! contains only the user-provided barcode/SKU. We do not forward
//! cookies, tokens, or any other identity material.

pub mod digikey;
pub mod upcitemdb;

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Normalised lookup result. Providers populate what they can; missing
/// fields stay None so the frontend can decide what to surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResult {
    pub source: &'static str,
    pub barcode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mpn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

impl LookupResult {
    pub fn new(source: &'static str, barcode: impl Into<String>) -> Self {
        Self {
            source,
            barcode: barcode.into(),
            mpn: None,
            name: None,
            brand: None,
            category: None,
            description: None,
            image_url: None,
            product_url: None,
            price: None,
            currency: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("no provider configured")]
    NoProvider,
    #[error("invalid barcode")]
    InvalidBarcode,
    #[error("provider error: {0}")]
    Provider(String),
    #[error(transparent)]
    Network(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Provider trait. Implementations are stateless except for cached
/// auth tokens, which they manage internally.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn lookup(&self, barcode: &str) -> Result<Vec<LookupResult>, LookupError>;
}

/// Builds the active provider chain from env vars. Order matters:
/// the first provider with results wins so we can put the cheapest /
/// fastest source first and only fall back when it has nothing.
pub fn providers_from_env() -> Vec<Box<dyn Provider>> {
    let mut out: Vec<Box<dyn Provider>> = Vec::new();

    // UPCitemDB — free trial endpoint, 100 req/day, no auth. Always on
    // unless explicitly disabled, because it costs the operator nothing.
    if std::env::var("RACKLOG_UPCITEMDB_DISABLED").ok().as_deref() != Some("1") {
        out.push(Box::new(upcitemdb::UpcItemDb::new()));
    }

    // DigiKey — opt-in via client credentials.
    if let (Ok(id), Ok(secret)) = (
        std::env::var("DIGIKEY_CLIENT_ID"),
        std::env::var("DIGIKEY_CLIENT_SECRET"),
    ) {
        let site = std::env::var("DIGIKEY_LOCALE_SITE")
            .unwrap_or_else(|_| "US".to_string());
        let language = std::env::var("DIGIKEY_LOCALE_LANGUAGE")
            .unwrap_or_else(|_| "en".to_string());
        let currency = std::env::var("DIGIKEY_LOCALE_CURRENCY")
            .unwrap_or_else(|_| "USD".to_string());
        out.push(Box::new(digikey::DigiKey::new(
            id, secret, site, language, currency,
        )));
    }

    out
}

/// Run every configured provider sequentially until one returns hits.
/// Stops on first non-empty result so we don't spam multiple APIs for
/// the common case where the cheapest provider has the answer.
pub async fn lookup_chain(
    providers: &[Box<dyn Provider>],
    barcode: &str,
) -> Result<Vec<LookupResult>, LookupError> {
    if providers.is_empty() {
        return Err(LookupError::NoProvider);
    }
    let mut last_err = None;
    for p in providers {
        match p.lookup(barcode).await {
            Ok(hits) if !hits.is_empty() => return Ok(hits),
            Ok(_) => continue,
            Err(e) => {
                tracing::warn!(provider = p.name(), %e, "provider failed");
                last_err = Some(e);
            }
        }
    }
    if let Some(e) = last_err {
        return Err(e);
    }
    Ok(Vec::new())
}

/// Single shared HTTP client. Built lazily and reused so we keep
/// connection pools warm. 10 s timeout is conservative — UPCitemDB and
/// DigiKey both respond in <1 s under normal conditions.
fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("racklog/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("reqwest client")
}

/// Reject obvious junk before we make any network call. Keeps log
/// noise low and stops a hostile barcode from probing the URL space.
pub fn validate_barcode(s: &str) -> Result<(), LookupError> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.len() > 32 {
        return Err(LookupError::InvalidBarcode);
    }
    // Allow A-Z, 0-9, '-' (DigiKey part numbers), '_' (rare).
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(LookupError::InvalidBarcode);
    }
    Ok(())
}
