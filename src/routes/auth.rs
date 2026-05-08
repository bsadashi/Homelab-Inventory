//! /api/auth — signup, login, logout, current-user.
//!
//! All four endpoints are in the `PUBLIC_PATHS` allowlist of the auth
//! middleware; the per-handler logic enforces the actual policy
//! (signup is open only when bootstrapping or `RACKLOG_OPEN_SIGNUP=1`,
//! logout is harmless without a session, etc.).

use crate::audit::{record, AuditEvent};
use crate::auth::identity::{AuthIdentity, Role};
use crate::auth::{password, sessions, SESSION_COOKIE};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::users;
use axum::extract::State;
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/signup", post(signup))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

#[derive(Deserialize)]
struct CredentialsInput {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct AuthResponse {
    user: users::UserPublic,
    expires_at: String,
    /// Source of the just-resolved identity (always "session" here,
    /// but mirrored from the middleware enum for symmetry with /me).
    source: &'static str,
}

fn validate_username(s: &str) -> Result<&str, ApiError> {
    let s = s.trim();
    if s.is_empty() || s.len() > 64 {
        return Err(ApiError::BadRequest("username must be 1–64 chars".into()));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(ApiError::BadRequest(
            "username may only contain letters, digits, _, -, .".into(),
        ));
    }
    Ok(s)
}

fn build_session_cookie(token: &str, expires_at: chrono::DateTime<chrono::Utc>) -> String {
    // HttpOnly so JS can't read it; SameSite=Strict so it isn't sent
    // on cross-site navigations; Secure when served over HTTPS in
    // production (the reverse proxy can rewrite if it terminates TLS).
    let max_age = expires_at
        .signed_duration_since(chrono::Utc::now())
        .num_seconds()
        .max(0);
    format!(
        "{}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}; Secure",
        SESSION_COOKIE, token, max_age
    )
}

fn build_clear_cookie() -> String {
    format!(
        "{}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        SESSION_COOKIE
    )
}

async fn signup(
    State(state): State<AppState>,
    Json(input): Json<CredentialsInput>,
) -> ApiResult<(StatusCode, HeaderMap, Json<AuthResponse>)> {
    let username = validate_username(&input.username)?.to_string();
    let count = users::count(&state.pool).await?;

    // First user always becomes admin (bootstrap). Subsequent
    // signups are only allowed when the operator has explicitly
    // opted in via RACKLOG_OPEN_SIGNUP=1.
    let (role, source_label) = if count == 0 {
        (Role::Admin, "signup-bootstrap")
    } else if state.cfg.open_signup {
        (Role::Viewer, "signup-open")
    } else {
        return Err(ApiError::Forbidden);
    };

    let hash = password::hash(&input.password).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let user = match users::create(&state.pool, &username, &hash, role, source_label).await {
        Ok(u) => u,
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
            return Err(ApiError::Conflict("username already exists".into()));
        }
        Err(e) => return Err(ApiError::Database(e)),
    };
    let _ = users::touch_last_login(&state.pool, &user.id).await;

    let session = sessions::create(&state.pool, &user.id, sessions::DEFAULT_TTL_HOURS)
        .await
        .map_err(ApiError::Database)?;

    record(
        &state.pool,
        AuditEvent {
            user: &user.username,
            kind: "auth.signup",
            r#ref: Some(&user.id),
            description: &format!("User {} signed up ({})", user.username, role.as_str()),
            payload: None,
        },
    )
    .await
    .ok();

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&build_session_cookie(
            &session.plaintext,
            session.expires_at,
        ))
        .unwrap_or(HeaderValue::from_static("")),
    );
    Ok((
        StatusCode::CREATED,
        headers,
        Json(AuthResponse {
            user: user.into(),
            expires_at: session.expires_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            source: "session",
        }),
    ))
}

async fn login(
    State(state): State<AppState>,
    Json(input): Json<CredentialsInput>,
) -> ApiResult<(StatusCode, HeaderMap, Json<AuthResponse>)> {
    let username = validate_username(&input.username)?.to_string();
    let lookup = users::find_by_username(&state.pool, &username).await?;
    // Constant-ish "wrong creds" path: always 401, never leak whether
    // the username exists.
    let (user, hash) = match lookup {
        Some((u, h)) if !u.disabled => (u, h),
        _ => {
            // Run an Argon2 verify against a fixed dummy hash so the
            // wall-time of "no such user" matches "user exists, wrong
            // password". An earlier version of this code passed an
            // empty string to verify(), which short-circuits before
            // any Argon2 work — leaving a 20× timing oracle for
            // username enumeration.
            password::verify_dummy(&input.password);
            return Err(ApiError::Unauthorized);
        }
    };
    let ok = password::verify(&input.password, &hash).unwrap_or(false);
    if !ok {
        return Err(ApiError::Unauthorized);
    }

    let _ = users::touch_last_login(&state.pool, &user.id).await;

    let session = sessions::create(&state.pool, &user.id, sessions::DEFAULT_TTL_HOURS)
        .await
        .map_err(ApiError::Database)?;

    record(
        &state.pool,
        AuditEvent {
            user: &user.username,
            kind: "auth.login",
            r#ref: Some(&user.id),
            description: &format!("Login from {}", user.username),
            payload: None,
        },
    )
    .await
    .ok();

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&build_session_cookie(
            &session.plaintext,
            session.expires_at,
        ))
        .unwrap_or(HeaderValue::from_static("")),
    );
    Ok((
        StatusCode::OK,
        headers,
        Json(AuthResponse {
            user: user.into(),
            expires_at: session.expires_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            source: "session",
        }),
    ))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(cookie) = headers.get(COOKIE).and_then(|h| h.to_str().ok()) {
        if let Some(token) = parse_cookie(cookie, SESSION_COOKIE) {
            let _ = sessions::revoke(&state.pool, &token).await;
        }
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&build_clear_cookie()).unwrap_or(HeaderValue::from_static("")),
    );
    (StatusCode::NO_CONTENT, headers)
}

#[derive(Serialize)]
struct MeResponse {
    user: users::UserPublic,
    source: String,
    /// Whether the dashboard should show the "Sign up" path to the
    /// caller — true on a fresh deployment with no users yet.
    bootstrap_open: bool,
    open_signup: bool,
}

async fn me(
    State(state): State<AppState>,
    identity: Option<Extension<AuthIdentity>>,
) -> ApiResult<Json<MeResponse>> {
    let count = users::count(&state.pool).await.unwrap_or(0);
    if let Some(Extension(id)) = identity {
        let public = users::UserPublic {
            id: id.user_id.clone(),
            username: id.username.clone(),
            role: id.role,
            disabled: false,
        };
        return Ok(Json(MeResponse {
            user: public,
            source: serde_json::to_value(id.source)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".into()),
            bootstrap_open: count == 0,
            open_signup: state.cfg.open_signup,
        }));
    }
    Err(ApiError::Unauthorized)
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
