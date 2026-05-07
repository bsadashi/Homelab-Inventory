//! Auth middleware. Resolves the request's identity via cookie /
//! API-key / bootstrap-token / trusted-header (in that order) and
//! injects an `AuthIdentity` into request extensions on success.
//!
//! Per-route extractors then enforce role policy on top of that.

use crate::auth::api_keys;
use crate::auth::identity::{AuthIdentity, AuthSource, Role};
use crate::auth::sessions;
use crate::state::AppState;
use crate::users;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

/// Cookie name for the dashboard session.
pub const SESSION_COOKIE: &str = "racklog_session";

/// Paths that bypass authentication entirely. Health probes have to be
/// reachable for Kubernetes; the /auth subtree has to be reachable or
/// there's no way to ever obtain credentials.
const PUBLIC_PATHS: &[&str] = &[
    "/api/healthz",
    "/api/readyz",
    "/healthz",
    "/readyz",
    "/api/auth/signup",
    "/auth/signup",
    "/api/auth/login",
    "/auth/login",
    "/api/auth/logout",
    "/auth/logout",
    "/api/auth/me",
    "/auth/me",
];

/// Snapshot of every credential the resolver needs, captured up-front
/// so the async resolution path doesn't hold a `&Request` across an
/// await — keeping the future `Send`.
#[derive(Default)]
struct Credentials {
    cookie: Option<String>,
    bearer: Option<String>,
    forwarded_user: Option<String>,
    forwarded_role: Role,
    path: String,
}

pub async fn require_auth(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let creds = snapshot_credentials(&request, state.cfg.trust_forwarded_headers);
    let public = PUBLIC_PATHS.iter().any(|p| creds.path == *p);
    let identity = resolve_identity(&state, &creds).await;
    if identity.is_none() && !public {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let mut request = request;
    if let Some(id) = identity {
        request.extensions_mut().insert(id);
    }
    Ok(next.run(request).await)
}

fn snapshot_credentials(request: &Request<axum::body::Body>, trust_forwarded: bool) -> Credentials {
    let path = request.uri().path().to_string();

    let cookie = request
        .headers()
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| parse_cookie(h, SESSION_COOKIE));

    let bearer = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(strip_bearer_scheme)
        .map(str::to_string);

    let (forwarded_user, forwarded_role) = if trust_forwarded {
        let user = request
            .headers()
            .get("X-Forwarded-User")
            .and_then(|h| h.to_str().ok())
            .map(str::trim)
            .filter(|u| !u.is_empty() && u.len() <= 64)
            .filter(|u| {
                u.chars().all(|c| {
                    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '@'
                })
            })
            .map(str::to_string);
        let role = request
            .headers()
            .get("X-Forwarded-Groups")
            .and_then(|h| h.to_str().ok())
            .map(map_groups_to_role)
            .unwrap_or(Role::Viewer);
        (user, role)
    } else {
        (None, Role::Viewer)
    };

    Credentials {
        cookie,
        bearer,
        forwarded_user,
        forwarded_role,
        path,
    }
}

async fn resolve_identity(state: &AppState, c: &Credentials) -> Option<AuthIdentity> {
    // 1. Session cookie (humans on the dashboard).
    if let Some(token) = &c.cookie {
        if let Ok(Some(id)) = sessions::resolve(&state.pool, token).await {
            return Some(id);
        }
    }
    // 2. Bearer — first as API key, then as bootstrap admin token.
    if let Some(presented) = &c.bearer {
        if let Ok(Some(id)) = api_keys::resolve(&state.pool, presented).await {
            return Some(id);
        }
        if let Some(expected) = state.cfg.auth_token.clone() {
            if presented == &*expected && users::count(&state.pool).await.unwrap_or(0) == 0 {
                return Some(AuthIdentity::bootstrap_admin());
            }
        }
    }
    // 3. Trusted upstream header.
    if let Some(username) = &c.forwarded_user {
        if let Some(id) = resolve_forwarded_user(state, username, c.forwarded_role).await {
            return Some(id);
        }
    }

    // 4. "Pre-bootstrap" open mode. The deployment has nothing
    // configured (no auth token, no SSO trust) AND has no users yet.
    // This matches the old single-user-homelab posture: install,
    // run, sign up your first user. Once any of those conditions
    // change, the gate closes.
    if state.cfg.auth_token.is_none()
        && !state.cfg.trust_forwarded_headers
        && users::count(&state.pool).await.unwrap_or(0) == 0
    {
        return Some(AuthIdentity::bootstrap_admin());
    }

    None
}

async fn resolve_forwarded_user(
    state: &AppState,
    username: &str,
    role: Role,
) -> Option<AuthIdentity> {
    let user = match users::find_by_username(&state.pool, username).await {
        Ok(Some((u, _))) => u,
        Ok(None) => match users::create(&state.pool, username, "", role, "sso").await {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(%e, %username, "SSO auto-provision failed");
                return None;
            }
        },
        Err(_) => return None,
    };
    if user.disabled {
        return None;
    }
    if user.role != role {
        let _ = users::set_role(&state.pool, &user.id, role).await;
    }
    Some(AuthIdentity {
        user_id: user.id,
        username: user.username,
        role,
        source: AuthSource::TrustedHeader,
    })
}

fn parse_cookie(header_value: &str, name: &str) -> Option<String> {
    for kv in header_value.split(';') {
        let kv = kv.trim();
        if let Some((k, v)) = kv.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// RFC 6750 case-insensitive `Bearer <token>` parser.
pub fn strip_bearer_scheme(header: &str) -> Option<&str> {
    if header.len() < 7 {
        return None;
    }
    let (scheme, rest) = header.split_at(6);
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    let mut chars = rest.chars();
    let sep = chars.next()?;
    if sep != ' ' && sep != '\t' {
        return None;
    }
    let token = rest[1..].trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn map_groups_to_role(groups: &str) -> Role {
    let mut best = Role::Viewer;
    for p in groups.split([',', '|']) {
        let p = p.trim().to_ascii_lowercase();
        match p.as_str() {
            "racklog-admin" | "admin" | "admins" => return Role::Admin,
            "racklog-operator" | "operator" | "operators" => {
                if !best.at_least(Role::Operator) {
                    best = Role::Operator;
                }
            }
            "racklog-viewer" | "viewer" | "viewers" => {}
            _ => {}
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_cookie() {
        let v = "foo=bar; racklog_session=abcdef; baz=qux";
        assert_eq!(
            parse_cookie(v, "racklog_session").as_deref(),
            Some("abcdef")
        );
        assert!(parse_cookie(v, "missing").is_none());
    }

    #[test]
    fn strip_bearer_handles_case_and_separator() {
        assert_eq!(strip_bearer_scheme("Bearer abc"), Some("abc"));
        assert_eq!(strip_bearer_scheme("bearer abc"), Some("abc"));
        assert_eq!(strip_bearer_scheme("BEARER abc"), Some("abc"));
        assert_eq!(strip_bearer_scheme("Bearer\tabc"), Some("abc"));
        assert!(strip_bearer_scheme("Basic abc").is_none());
        assert!(strip_bearer_scheme("BearerNoSpace").is_none());
        assert!(strip_bearer_scheme("Bearer ").is_none());
    }

    #[test]
    fn group_mapping_picks_strongest() {
        assert_eq!(map_groups_to_role("racklog-admin"), Role::Admin);
        assert_eq!(map_groups_to_role("operators,viewers"), Role::Operator);
        assert_eq!(map_groups_to_role("viewers"), Role::Viewer);
        assert_eq!(map_groups_to_role("ops,admins"), Role::Admin);
        assert_eq!(map_groups_to_role("nothing-known"), Role::Viewer);
    }
}
