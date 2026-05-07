//! Long-lived API keys for service accounts / scripts.
//!
//! Keys are 32 random bytes hex-encoded with an `rl_` prefix so
//! humans can recognise them in logs / shells. Same hash-on-disk
//! posture as sessions: the plaintext is shown to the caller exactly
//! once (on creation) and never persisted.

use crate::auth::identity::{AuthIdentity, AuthSource, Role};
use crate::auth::sessions::{hash_token, random_token};
use sqlx::SqlitePool;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug)]
pub struct CreatedApiKey {
    pub id: String,
    pub label: String,
    /// Plaintext key (shown once). Format: `rl_<64-hex>`.
    pub plaintext: String,
}

pub async fn create(pool: &SqlitePool, user_id: &str, label: &str) -> sqlx::Result<CreatedApiKey> {
    let id = format!("ak-{}", Uuid::new_v4().simple());
    let token_body = random_token();
    let plaintext = format!("rl_{}", token_body);
    let key_hash = hash_token(&plaintext);
    sqlx::query("INSERT INTO api_keys (id, label, user_id, key_hash) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(label)
        .bind(user_id)
        .bind(&key_hash)
        .execute(pool)
        .await?;
    Ok(CreatedApiKey {
        id,
        label: label.to_string(),
        plaintext,
    })
}

pub async fn revoke(pool: &SqlitePool, id: &str) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE api_keys SET revoked_at = datetime('now') WHERE id = ? AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

#[derive(Debug, serde::Serialize)]
pub struct ApiKeySummary {
    pub id: String,
    pub label: String,
    pub user_id: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<ApiKeySummary>> {
    let rows: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        r#"SELECT id, label, user_id, created_at, last_used_at, revoked_at
               FROM api_keys ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, label, user_id, created_at, last_used_at, revoked_at)| ApiKeySummary {
                id,
                label,
                user_id,
                created_at,
                last_used_at,
                revoked_at,
            },
        )
        .collect())
}

/// Resolve a presented API key to an `AuthIdentity`. Bumps
/// `last_used_at`. Returns `None` for any miss (unknown, revoked,
/// disabled user).
pub async fn resolve(pool: &SqlitePool, plaintext: &str) -> sqlx::Result<Option<AuthIdentity>> {
    if !plaintext.starts_with("rl_") {
        return Ok(None);
    }
    let key_hash = hash_token(plaintext);
    let row: Option<(String, String, String, i64, Option<String>)> = sqlx::query_as(
        r#"SELECT k.user_id, u.username, u.role, u.disabled, k.revoked_at
           FROM api_keys k
           JOIN users u ON u.id = k.user_id
           WHERE k.key_hash = ?"#,
    )
    .bind(&key_hash)
    .fetch_optional(pool)
    .await?;
    let Some((user_id, username, role_str, disabled, revoked_at)) = row else {
        return Ok(None);
    };
    if disabled != 0 || revoked_at.is_some() {
        return Ok(None);
    }
    let _ = sqlx::query("UPDATE api_keys SET last_used_at = datetime('now') WHERE key_hash = ?")
        .bind(&key_hash)
        .execute(pool)
        .await;
    let role = Role::from_str(&role_str).unwrap_or(Role::Viewer);
    Ok(Some(AuthIdentity {
        user_id,
        username,
        role,
        source: AuthSource::ApiKey,
    }))
}
