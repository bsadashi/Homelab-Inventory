//! /api/admin/users + /api/admin/api_keys — user + service-account
//! lifecycle for admins.
//!
//! All endpoints require Admin role. Mutations are recorded in the
//! audit chain under the calling admin's username so a stolen API
//! key, or a rogue admin, can't quietly grant themselves rights and
//! later cover their tracks.

use crate::audit;
use crate::auth::api_keys;
use crate::auth::identity::Role;
use crate::auth::password;
use crate::auth::sessions;
use crate::auth::RequireAdmin;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::users;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/users", users_router())
        .nest("/api_keys", api_keys_router())
}

fn users_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/:id", get(get_user).delete(delete_user))
        .route("/:id/role", post(update_role))
        .route("/:id/disabled", post(set_disabled))
        .route("/:id/password", post(reset_password))
}

fn api_keys_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_api_keys).post(create_api_key))
        .route("/:id", axum::routing::delete(revoke_api_key))
}

// ---- users -------------------------------------------------------------

async fn list_users(
    State(state): State<AppState>,
    _: RequireAdmin,
) -> ApiResult<Json<Vec<users::UserRecord>>> {
    Ok(Json(users::list(&state.pool).await?))
}

async fn get_user(
    State(state): State<AppState>,
    _: RequireAdmin,
    Path(id): Path<String>,
) -> ApiResult<Json<users::UserRecord>> {
    users::find_by_id(&state.pool, &id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
struct CreateUserInput {
    username: String,
    password: String,
    role: String,
}

async fn create_user(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Json(input): Json<CreateUserInput>,
) -> ApiResult<(StatusCode, Json<users::UserPublic>)> {
    let username = validate_username(&input.username)?;
    let role = Role::from_str(&input.role)
        .map_err(|_| ApiError::BadRequest("role must be admin/operator/viewer".into()))?;
    let hash = password::hash(&input.password).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let user = match users::create(&state.pool, &username, &hash, role, "admin").await {
        Ok(u) => u,
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
            return Err(ApiError::Conflict("username already exists".into()));
        }
        Err(e) => return Err(ApiError::Database(e)),
    };
    audit::log(
        &state.pool,
        &auth.username,
        "user.create",
        Some(&user.id),
        &format!("Created user {} as {}", user.username, role.as_str()),
    )
    .await
    .ok();
    Ok((StatusCode::CREATED, Json(user.into())))
}

#[derive(Deserialize)]
struct UpdateRoleInput {
    role: String,
}

async fn update_role(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<UpdateRoleInput>,
) -> ApiResult<Json<users::UserPublic>> {
    if id == auth.user_id {
        return Err(ApiError::BadRequest(
            "cannot change your own role; ask another admin".into(),
        ));
    }
    let role = Role::from_str(&input.role)
        .map_err(|_| ApiError::BadRequest("role must be admin/operator/viewer".into()))?;
    let n = users::set_role(&state.pool, &id, role).await?;
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    // Revoke existing sessions so the role change takes effect on the
    // next request rather than waiting up to 30 days for the cookie
    // to expire. A demoted admin keeping their elevated session was
    // the original gap; the disable + password-reset paths already
    // do this — bring role-change in line with them.
    let _ = sessions::revoke_all_for_user(&state.pool, &id).await;
    let user = users::find_by_id(&state.pool, &id)
        .await?
        .ok_or(ApiError::NotFound)?;
    audit::log(
        &state.pool,
        &auth.username,
        "user.role",
        Some(&user.id),
        &format!("Set {} to role {}", user.username, role.as_str()),
    )
    .await
    .ok();
    Ok(Json(user.into()))
}

#[derive(Deserialize)]
struct SetDisabledInput {
    disabled: bool,
}

async fn set_disabled(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<SetDisabledInput>,
) -> ApiResult<Json<users::UserPublic>> {
    if id == auth.user_id && input.disabled {
        return Err(ApiError::BadRequest(
            "cannot disable yourself; ask another admin".into(),
        ));
    }
    let n = users::set_disabled(&state.pool, &id, input.disabled).await?;
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    if input.disabled {
        // Disabling kills active sessions immediately.
        let _ = sessions::revoke_all_for_user(&state.pool, &id).await;
    }
    let user = users::find_by_id(&state.pool, &id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let kind = if input.disabled {
        "user.disable"
    } else {
        "user.enable"
    };
    let verb = if input.disabled {
        "Disabled"
    } else {
        "Re-enabled"
    };
    audit::log(
        &state.pool,
        &auth.username,
        kind,
        Some(&user.id),
        &format!("{} user {}", verb, user.username),
    )
    .await
    .ok();
    Ok(Json(user.into()))
}

#[derive(Deserialize)]
struct ResetPasswordInput {
    password: String,
}

async fn reset_password(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<ResetPasswordInput>,
) -> ApiResult<StatusCode> {
    let hash = password::hash(&input.password).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let n = users::set_password_hash(&state.pool, &id, &hash).await?;
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    // Invalidate all of the target user's sessions so the new
    // password actually takes effect on the next login.
    let _ = sessions::revoke_all_for_user(&state.pool, &id).await;
    let user = users::find_by_id(&state.pool, &id)
        .await?
        .ok_or(ApiError::NotFound)?;
    audit::log(
        &state.pool,
        &auth.username,
        "user.reset_password",
        Some(&user.id),
        &format!("Admin reset password for {}", user.username),
    )
    .await
    .ok();
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    if id == auth.user_id {
        return Err(ApiError::BadRequest(
            "cannot delete yourself; ask another admin".into(),
        ));
    }
    let user = users::find_by_id(&state.pool, &id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let n = users::delete(&state.pool, &id).await?;
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    audit::log(
        &state.pool,
        &auth.username,
        "user.delete",
        Some(&id),
        &format!("Deleted user {}", user.username),
    )
    .await
    .ok();
    Ok(StatusCode::NO_CONTENT)
}

fn validate_username(s: &str) -> ApiResult<String> {
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
    Ok(s.to_string())
}

// ---- API keys ----------------------------------------------------------

async fn list_api_keys(
    State(state): State<AppState>,
    _: RequireAdmin,
) -> ApiResult<Json<Vec<api_keys::ApiKeySummary>>> {
    Ok(Json(api_keys::list(&state.pool).await?))
}

#[derive(Deserialize)]
struct CreateApiKeyInput {
    user_id: String,
    label: String,
    /// "inherit" (default; legacy behaviour), "viewer", or
    /// "operator". Lets an admin issue read-only keys to scripts
    /// without touching the owning user's role.
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Serialize)]
struct CreateApiKeyResponse {
    id: String,
    label: String,
    /// Plaintext key — shown exactly once. The dashboard surfaces
    /// this in a one-shot dialog.
    plaintext: String,
    scope: String,
}

async fn create_api_key(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Json(input): Json<CreateApiKeyInput>,
) -> ApiResult<(StatusCode, Json<CreateApiKeyResponse>)> {
    if input.label.trim().is_empty() || input.label.len() > 128 {
        return Err(ApiError::BadRequest("label must be 1–128 chars".into()));
    }
    let scope = api_keys::KeyScope::parse(input.scope.as_deref().unwrap_or("inherit"))
        .ok_or_else(|| ApiError::BadRequest("scope must be inherit / viewer / operator".into()))?;
    let _user = users::find_by_id(&state.pool, &input.user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let created = api_keys::create(&state.pool, &input.user_id, input.label.trim(), scope).await?;
    audit::log(
        &state.pool,
        &auth.username,
        "api_key.create",
        Some(&created.id),
        &format!(
            "Created API key {} (scope={}) for user {}",
            created.label,
            created.scope.as_str(),
            input.user_id
        ),
    )
    .await
    .ok();
    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResponse {
            id: created.id,
            label: created.label,
            plaintext: created.plaintext,
            scope: created.scope.as_str().to_string(),
        }),
    ))
}

async fn revoke_api_key(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let n = api_keys::revoke(&state.pool, &id).await?;
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    audit::log(
        &state.pool,
        &auth.username,
        "api_key.revoke",
        Some(&id),
        &format!("Revoked API key {}", id),
    )
    .await
    .ok();
    Ok(StatusCode::NO_CONTENT)
}
