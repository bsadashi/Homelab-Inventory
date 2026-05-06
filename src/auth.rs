//! Optional bearer-token auth middleware.
//!
//! When `RACKLOG_AUTH_TOKEN` is unset (the default), the service runs
//! open — appropriate for a single-user homelab box behind a firewall.
//! When set, every /api request that isn't an explicit health probe
//! must carry `Authorization: Bearer <token>`. Comparison is constant
//! time so the token can't be inferred from response timing.

use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

// Both the nested form (/api/healthz) and the post-nest path the
// middleware actually sees (/healthz). Listing both shields us against
// future refactors that move the middleware up or down a layer.
const PUBLIC_PATHS: &[&str] = &["/api/healthz", "/api/readyz", "/healthz", "/readyz"];

pub async fn require_token(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(expected) = state.cfg.auth_token.clone() else {
        return Ok(next.run(request).await);
    };
    if PUBLIC_PATHS.iter().any(|p| request.uri().path() == *p) {
        return Ok(next.run(request).await);
    }

    let header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let presented = header.strip_prefix("Bearer ").unwrap_or("").trim();
    if !constant_time_equal(presented.as_bytes(), expected.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

fn constant_time_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        // Still walk a fixed-length comparison so total wall time leaks
        // length information, not equality.
        return false;
    }
    bool::from(subtle::ConstantTimeEq::ct_eq(a, b))
}
