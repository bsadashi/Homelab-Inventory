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
    // Token lives in an Arc<String> so every request only clones the
    // pointer, not the bytes — matters when the token is long.
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

    let presented = strip_bearer_scheme(header).trim();
    if !constant_time_equal(presented.as_bytes(), expected.as_str().as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

/// RFC 6750: the `Bearer` scheme name is case-insensitive. Match
/// any case so clients sending `bearer ` or `BEARER ` still work.
fn strip_bearer_scheme(header: &str) -> &str {
    if header.len() < 7 {
        return "";
    }
    let (scheme, rest) = header.split_at(6);
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return "";
    }
    // Require a separator after the scheme name.
    let rest_chars = rest.chars().next();
    if !matches!(rest_chars, Some(' ') | Some('\t')) {
        return "";
    }
    &rest[1..]
}

fn constant_time_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        // Still walk a fixed-length comparison so total wall time leaks
        // length information, not equality.
        return false;
    }
    bool::from(subtle::ConstantTimeEq::ct_eq(a, b))
}

#[cfg(test)]
mod tests {
    use super::strip_bearer_scheme;

    #[test]
    fn strips_canonical_bearer() {
        assert_eq!(strip_bearer_scheme("Bearer abc"), "abc");
    }
    #[test]
    fn strips_lowercase_bearer() {
        assert_eq!(strip_bearer_scheme("bearer abc"), "abc");
    }
    #[test]
    fn strips_uppercase_bearer() {
        assert_eq!(strip_bearer_scheme("BEARER abc"), "abc");
    }
    #[test]
    fn rejects_other_schemes() {
        assert_eq!(strip_bearer_scheme("Basic abc"), "");
    }
    #[test]
    fn requires_separator() {
        assert_eq!(strip_bearer_scheme("BearerNoSpace"), "");
    }
    #[test]
    fn handles_short_strings() {
        assert_eq!(strip_bearer_scheme(""), "");
        assert_eq!(strip_bearer_scheme("Bear"), "");
    }
}
