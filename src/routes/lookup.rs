//! /api/lookup — barcode → product info via configured external
//! providers, plus a local fallback that returns the matching item if
//! the barcode is already in our catalog.

use crate::error::{ApiError, ApiResult};
use crate::sku_sync::{lookup_chain, validate_barcode, LookupError, LookupResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sqlx::Row;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/providers", get(providers))
        .route("/:barcode", get(lookup))
}

#[derive(Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<&'static str>,
}

async fn providers(State(state): State<AppState>) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        providers: state.providers.iter().map(|p| p.name()).collect(),
    })
}

#[derive(Serialize)]
pub struct LookupResponse {
    pub barcode: String,
    /// Matching local item id, if any. Lets the UI jump straight to the
    /// existing record instead of offering an import.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_item_id: Option<String>,
    pub external: Vec<LookupResult>,
    pub providers_tried: Vec<&'static str>,
}

async fn lookup(
    State(state): State<AppState>,
    Path(barcode): Path<String>,
) -> ApiResult<Json<LookupResponse>> {
    // Cheap process-wide rate limit. /api/lookup is the only endpoint
    // that can spend the operator's external API quota (UPCitemDB:
    // ~100/day; DigiKey: per-token), so a misbehaving client shouldn't
    // be able to burn it from a tight loop. The default 30 req/min
    // is generous for hand-scanning and easily raised via
    // RACKLOG_LOOKUP_RATE_PER_MIN.
    if !lookup_rate_limit_allow() {
        return Err(ApiError::BadRequest(
            "rate limit exceeded; slow down the lookups".into(),
        ));
    }

    // Validate AND canonicalise so SQL bind and provider URLs all
    // see the same trimmed form.
    let canonical = validate_barcode(&barcode).map_err(|_| {
        ApiError::BadRequest(
            "barcode must be 1–32 chars, alphanumeric/_/- only".into(),
        )
    })?;
    let canonical = canonical.to_string();

    // Local first — most barcodes will already exist in the catalog.
    let local: Option<String> =
        sqlx::query("SELECT id FROM items WHERE barcode = ? LIMIT 1")
            .bind(&canonical)
            .fetch_optional(&state.pool)
            .await?
            .map(|r| r.get::<String, _>("id"));

    let providers_tried: Vec<&'static str> =
        state.providers.iter().map(|p| p.name()).collect();

    let external = if state.providers.is_empty() {
        Vec::new()
    } else {
        match lookup_chain(&state.providers, &canonical).await {
            Ok(hits) => hits,
            Err(LookupError::NoProvider) => Vec::new(),
            Err(LookupError::InvalidBarcode) => {
                return Err(ApiError::BadRequest("invalid barcode".into()))
            }
            Err(e) => {
                tracing::warn!(%e, barcode = %canonical, "external lookup failed");
                Vec::new()
            }
        }
    };

    Ok(Json(LookupResponse {
        barcode: canonical,
        local_item_id: local,
        external,
        providers_tried,
    }))
}

/// Process-wide token-bucket. The bucket holds up to `capacity`
/// tokens (we call it the burst), and tokens refill at
/// `rate / 60` per second. Each lookup consumes one token. Single
/// replica is the assumed deployment shape, which matches the rest
/// of the SQLite-backed posture; horizontal scaling needs an
/// out-of-process limiter (Redis, etc.).
fn lookup_rate_limit_allow() -> bool {
    static BUCKET: Mutex<Option<TokenBucket>> = Mutex::new(None);
    let mut guard = BUCKET.lock().unwrap();
    let bucket = guard.get_or_insert_with(|| {
        let per_min = std::env::var("RACKLOG_LOOKUP_RATE_PER_MIN")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(30.0)
            .max(1.0);
        let burst = std::env::var("RACKLOG_LOOKUP_BURST")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(per_min)
            .max(1.0);
        TokenBucket {
            tokens: burst,
            capacity: burst,
            refill_per_sec: per_min / 60.0,
            last: Instant::now(),
        }
    });
    bucket.try_take(1.0)
}

struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_per_sec: f64,
    last: Instant,
}

impl TokenBucket {
    fn try_take(&mut self, n: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last = now;
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TokenBucket;
    use std::time::Instant;

    #[test]
    fn bucket_allows_initial_burst_then_blocks() {
        let mut b = TokenBucket {
            tokens: 3.0, capacity: 3.0, refill_per_sec: 0.001, last: Instant::now(),
        };
        assert!(b.try_take(1.0));
        assert!(b.try_take(1.0));
        assert!(b.try_take(1.0));
        assert!(!b.try_take(1.0), "burst exhausted");
    }
    #[test]
    fn bucket_refills_over_time() {
        let mut b = TokenBucket {
            tokens: 0.0, capacity: 1.0, refill_per_sec: 100.0,
            last: Instant::now() - std::time::Duration::from_millis(50),
        };
        assert!(b.try_take(1.0), "should have refilled");
    }
}
