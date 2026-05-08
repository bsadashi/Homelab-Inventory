//! /api/events — webhook ingest for Matter / Home Assistant / any
//! external sensor that wants to drop a line into the audit chain.
//!
//! Any authenticated identity can POST here, but the event `kind` is
//! whitelisted to a small set of caller-domain prefixes so a service
//! account can't masquerade as core inventory activity (item.create
//! etc). Pair with a dedicated "homeassistant" / "matter-bridge"
//! user + API key in production; the key grants ingest only because
//! Viewer role can't reach any mutating inventory endpoint.

use crate::audit::log;
use crate::auth::Authed;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::Router;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// Caller-domain prefixes the audit log will accept from this
/// endpoint. Anything else is rejected so a stolen sensor key can
/// never impersonate a core system event.
const ALLOWED_KIND_PREFIXES: &[&str] = &[
    "ext.",    // generic external integrations
    "sensor.", // generic sensor readings
    "matter.", // Matter device events
    "ha.",     // Home Assistant
    "iot.",    // ambient-IoT / BLE
    "vision.", // computer-vision pipelines
];

/// Maximum size of any single field. Stops a misconfigured sensor
/// from filling the activity log with megabyte payloads.
const MAX_FIELD_LEN: usize = 2048;

pub fn router() -> Router<AppState> {
    Router::new().route("/", post(ingest))
}

#[derive(Debug, Deserialize)]
pub struct EventInput {
    /// Namespaced event type. Must begin with one of the allowed
    /// prefixes (e.g. `matter.cabinet_opened`, `ha.rack_a_temp_warn`).
    pub kind: String,
    #[serde(default, rename = "ref")]
    pub r#ref: Option<String>,
    pub description: String,
    /// Optional structured context, stored as canonical JSON in the
    /// audit row's `payload` column. Bring whatever the upstream
    /// sensor produces.
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct IngestResponse {
    ok: bool,
    /// Echoes back the canonical `kind` we recorded. Useful for the
    /// caller to confirm whitelist match without parsing 4xx bodies.
    recorded_as: String,
}

/// Maximum age of `X-Racklog-Timestamp` accepted by the HMAC
/// verifier. Generous enough to cover normal client/server clock
/// drift; tight enough to make replay infeasible.
const HMAC_TIMESTAMP_WINDOW_SECS: i64 = 300;

async fn ingest(
    State(state): State<AppState>,
    Authed(auth): Authed,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<(StatusCode, axum::Json<IngestResponse>)> {
    // 0. Optional HMAC + timestamp verification. Disabled by default
    // (homelab API keys already provide authenticity); enabled by
    // setting RACKLOG_EVENTS_HMAC_SECRET. When on, the client signs
    // `<timestamp>.<body>` with HMAC-SHA256 and presents both the
    // timestamp and signature in headers. The timestamp covers
    // replay; the signature covers tampering even if a key has
    // been re-used to forge a synthetic event.
    if let Some(secret) = state.cfg.events_hmac_secret.as_deref() {
        verify_hmac(secret, &headers, &body)?;
    }
    let input: EventInput = serde_json::from_slice(&body)
        .map_err(|e| ApiError::BadRequest(format!("invalid JSON: {e}")))?;
    // 1. Validate prefix.
    let kind_ok = ALLOWED_KIND_PREFIXES
        .iter()
        .any(|p| input.kind.starts_with(p));
    if !kind_ok {
        return Err(ApiError::BadRequest(format!(
            "kind must start with one of: {}",
            ALLOWED_KIND_PREFIXES.join(", ")
        )));
    }
    // 2. Bound every field.
    if input.kind.len() > 64 || input.description.is_empty() {
        return Err(ApiError::BadRequest(
            "kind ≤ 64 chars; description must not be empty".into(),
        ));
    }
    if input.description.len() > MAX_FIELD_LEN {
        return Err(ApiError::BadRequest(format!(
            "description must be ≤ {MAX_FIELD_LEN} chars"
        )));
    }
    if let Some(r) = &input.r#ref {
        if r.len() > 128 {
            return Err(ApiError::BadRequest("ref must be ≤ 128 chars".into()));
        }
    }
    // 3. Cap payload size by re-serialising and length-checking.
    if let Some(p) = &input.payload {
        let bytes = serde_json::to_vec(p)?;
        if bytes.len() > 16 * 1024 {
            return Err(ApiError::BadRequest("payload must be ≤ 16 KiB".into()));
        }
    }
    // Append to the audit chain. We use `audit::log` (one-shot
    // transaction) since events arrive independently and don't
    // share a tx with anything else. Payload-bearing events use
    // `record` directly so the structured context is preserved on
    // disk for forensic queries.
    if let Some(payload) = &input.payload {
        crate::audit::record(
            &state.pool,
            crate::audit::AuditEvent {
                user: &auth.username,
                kind: &input.kind,
                r#ref: input.r#ref.as_deref(),
                description: &input.description,
                payload: Some(payload),
            },
        )
        .await?;
    } else {
        log(
            &state.pool,
            &auth.username,
            &input.kind,
            input.r#ref.as_deref(),
            &input.description,
        )
        .await?;
    }

    Ok((
        StatusCode::CREATED,
        axum::Json(IngestResponse {
            ok: true,
            recorded_as: input.kind,
        }),
    ))
}

fn verify_hmac(secret: &str, headers: &HeaderMap, body: &[u8]) -> ApiResult<()> {
    let ts = headers
        .get("X-Racklog-Timestamp")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| ApiError::BadRequest("X-Racklog-Timestamp header required".into()))?;
    let sig_hdr = headers
        .get("X-Racklog-Signature")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| ApiError::BadRequest("X-Racklog-Signature header required".into()))?;
    let presented = sig_hdr
        .strip_prefix("sha256=")
        .ok_or_else(|| ApiError::BadRequest("X-Racklog-Signature must be sha256=<hex>".into()))?;
    let presented_bytes = hex::decode(presented)
        .map_err(|_| ApiError::BadRequest("X-Racklog-Signature is not hex".into()))?;

    let now = chrono::Utc::now().timestamp();
    let ts_n: i64 = ts
        .parse()
        .map_err(|_| ApiError::BadRequest("X-Racklog-Timestamp must be unix seconds".into()))?;
    if (now - ts_n).abs() > HMAC_TIMESTAMP_WINDOW_SECS {
        return Err(ApiError::Unauthorized);
    }

    // Compute HMAC over "<timestamp>.<body>" so the timestamp is
    // bound into the signature — otherwise an attacker could
    // replace the timestamp on a captured request.
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .map_err(|_| ApiError::Other(anyhow::anyhow!("hmac key error")))?;
    mac.update(ts.as_bytes());
    mac.update(b".");
    mac.update(body);
    let expected = mac.finalize().into_bytes();
    // Constant-time compare to keep the signature path from
    // leaking timing info on prefix-correct attempts.
    if expected.ct_eq(&presented_bytes).unwrap_u8() == 1 {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}
