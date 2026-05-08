//! DigiKey product information API.
//!
//! Auth flow: client_credentials OAuth2 → short-lived (≤10 min) bearer
//! token cached in-process. The token is refreshed lazily on the first
//! 401 (or when the cached value is past its expiry minus 30 s of
//! safety margin).
//!
//! Endpoints used:
//!   POST  /v1/oauth2/token              — token mint
//!   GET   /products/v4/search/barcode/  — barcode → part match
//!
//! Reference: https://developer.digikey.com/products/barcode/v4

use super::{http, validate_barcode, LookupError, LookupResult, Provider};
use serde::Deserialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct DigiKey {
    client_id: String,
    client_secret: super::Redacted,
    site: String,
    language: String,
    currency: String,
    base_url: String,
    token_url: String,
    token: Mutex<Option<CachedToken>>,
}

#[derive(Clone)]
struct CachedToken {
    value: super::Redacted,
    expires_at: Instant,
}

/// Intentionally NOT `Debug` — we never want a typo of
/// `tracing::debug!("{:?}", parsed)` to print the OAuth bearer
/// token. Field access goes through explicit moves at the call
/// site, so removing Debug breaks nothing.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BarcodeResponse {
    #[serde(default)]
    product: Option<Product>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Product {
    description: Option<Description>,
    manufacturer_product_number: Option<String>,
    manufacturer: Option<NamedRef>,
    category: Option<NamedRef>,
    photo_url: Option<String>,
    product_url: Option<String>,
    unit_price: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Description {
    product_description: Option<String>,
    detailed_description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NamedRef {
    name: Option<String>,
}

impl DigiKey {
    pub fn new(
        client_id: String,
        client_secret: String,
        site: String,
        language: String,
        currency: String,
    ) -> Self {
        let base_url =
            std::env::var("DIGIKEY_BASE_URL").unwrap_or_else(|_| "https://api.digikey.com".into());
        let token_url = format!("{}/v1/oauth2/token", base_url);
        Self {
            client_id,
            client_secret: super::Redacted::new(client_secret),
            site,
            language,
            currency,
            base_url,
            token_url,
            token: Mutex::new(None),
        }
    }

    async fn token(&self) -> Result<String, LookupError> {
        {
            let guard = self.token.lock().unwrap();
            if let Some(c) = &*guard {
                if c.expires_at > Instant::now() {
                    return Ok(c.value.expose().to_string());
                }
            }
        }
        let resp = http()?
            .post(&self.token_url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", &self.client_id),
                ("client_secret", self.client_secret.expose()),
            ])
            .send()
            .await?;
        if !resp.status().is_success() {
            let s = resp.status();
            // Don't include the body — DigiKey error responses can
            // echo back form fields including client_id / client_secret
            // depending on their misconfiguration mode. Status alone
            // is enough to diagnose the common cases.
            let _body = resp.text().await.unwrap_or_default();
            return Err(LookupError::Provider(format!(
                "digikey token mint failed: status {s}"
            )));
        }
        let parsed: TokenResponse = resp.json().await?;
        let lifetime = Duration::from_secs(parsed.expires_in.unwrap_or(540));
        let access = parsed.access_token;
        let cached = CachedToken {
            value: super::Redacted::new(access.clone()),
            expires_at: Instant::now() + lifetime.saturating_sub(Duration::from_secs(30)),
        };
        *self.token.lock().unwrap() = Some(cached);
        Ok(access)
    }
}

#[async_trait::async_trait]
impl Provider for DigiKey {
    fn name(&self) -> &'static str {
        "digikey"
    }

    async fn lookup(&self, barcode: &str) -> Result<Vec<LookupResult>, LookupError> {
        let barcode = validate_barcode(barcode)?;
        // First attempt with the cached token. On 401 we drop the
        // cache and retry exactly once with a freshly-minted token —
        // this hides routine token expiry from the caller. A second
        // 401 surfaces as a real error.
        let mut attempt = 0u8;
        let parsed: BarcodeResponse = loop {
            attempt += 1;
            let token = self.token().await?;
            let url = format!("{}/products/v4/search/barcode/{}", self.base_url, barcode);
            let resp = http()?
                .get(&url)
                .bearer_auth(&token)
                .header("X-DIGIKEY-Client-Id", &self.client_id)
                .header("X-DIGIKEY-Locale-Site", &self.site)
                .header("X-DIGIKEY-Locale-Language", &self.language)
                .header("X-DIGIKEY-Locale-Currency", &self.currency)
                .send()
                .await?;

            match resp.status().as_u16() {
                200 => break resp.json().await?,
                404 => return Ok(Vec::new()),
                401 if attempt < 2 => {
                    tracing::debug!("digikey 401, retrying with fresh token");
                    *self.token.lock().unwrap() = None;
                    continue;
                }
                401 => {
                    *self.token.lock().unwrap() = None;
                    return Err(LookupError::Provider(
                        "digikey: unauthorised (token rotation failed)".into(),
                    ));
                }
                s => {
                    // Truncate body in the error message so we don't
                    // log unbounded provider responses, and never
                    // surface the body to the caller — only to logs.
                    let body = resp.text().await.unwrap_or_default();
                    let preview: String = body.chars().take(200).collect();
                    tracing::warn!(status = s, body = %preview, "digikey error response");
                    return Err(LookupError::Provider(format!("digikey: status {}", s)));
                }
            }
        };
        let Some(p) = parsed.product else {
            return Ok(Vec::new());
        };

        let mut r = LookupResult::new("digikey", barcode.to_string());
        r.mpn = p.manufacturer_product_number;
        r.brand = p.manufacturer.and_then(|m| m.name);
        r.category = p.category.and_then(|c| c.name);
        if let Some(d) = p.description {
            r.name = d.product_description.clone();
            r.description = d.detailed_description.or(d.product_description);
        }
        r.image_url = p.photo_url;
        r.product_url = p.product_url;
        r.price = p.unit_price;
        r.currency = Some(self.currency.clone());
        Ok(vec![r])
    }
}
