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
    pub scope: KeyScope,
}

/// How much of the owning user's role this key inherits at auth time.
/// Stored as a short string in the api_keys.scope column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyScope {
    /// Legacy behaviour: key takes the user's full role.
    Inherit,
    /// Read-only — clamps to Viewer regardless of the user's role.
    Viewer,
    /// Write-but-not-admin. Capped at Operator: an Admin user can
    /// still issue an Operator-scoped key, but a Viewer user cannot.
    Operator,
}

impl KeyScope {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyScope::Inherit => "inherit",
            KeyScope::Viewer => "viewer",
            KeyScope::Operator => "operator",
        }
    }
    pub fn parse(s: &str) -> Option<KeyScope> {
        match s {
            "inherit" => Some(KeyScope::Inherit),
            "viewer" => Some(KeyScope::Viewer),
            "operator" => Some(KeyScope::Operator),
            _ => None,
        }
    }
    /// Apply the scope clamp on top of the user's stored role.
    fn effective(self, user_role: Role) -> Role {
        match self {
            KeyScope::Inherit => user_role,
            KeyScope::Viewer => Role::Viewer,
            // Cap at Operator. A Viewer user with an Operator-scoped
            // key is still only a Viewer — the clamp lowers but
            // never raises the user's stored role.
            KeyScope::Operator => {
                if user_role.at_least(Role::Operator) {
                    Role::Operator
                } else {
                    user_role
                }
            }
        }
    }
}

pub async fn create(
    pool: &SqlitePool,
    user_id: &str,
    label: &str,
    scope: KeyScope,
) -> sqlx::Result<CreatedApiKey> {
    let id = format!("ak-{}", Uuid::new_v4().simple());
    let token_body = random_token();
    let plaintext = format!("rl_{}", token_body);
    let key_hash = hash_token(&plaintext);
    sqlx::query(
        "INSERT INTO api_keys (id, label, user_id, key_hash, scope) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(label)
    .bind(user_id)
    .bind(&key_hash)
    .bind(scope.as_str())
    .execute(pool)
    .await?;
    Ok(CreatedApiKey {
        id,
        label: label.to_string(),
        plaintext,
        scope,
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
    pub scope: String,
}

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<ApiKeySummary>> {
    let rows: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    )> = sqlx::query_as(
        r#"SELECT id, label, user_id, created_at, last_used_at, revoked_at, scope
               FROM api_keys ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, label, user_id, created_at, last_used_at, revoked_at, scope)| ApiKeySummary {
                id,
                label,
                user_id,
                created_at,
                last_used_at,
                revoked_at,
                scope,
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
    let row: Option<(String, String, String, i64, Option<String>, String)> = sqlx::query_as(
        r#"SELECT k.user_id, u.username, u.role, u.disabled, k.revoked_at, k.scope
           FROM api_keys k
           JOIN users u ON u.id = k.user_id
           WHERE k.key_hash = ?"#,
    )
    .bind(&key_hash)
    .fetch_optional(pool)
    .await?;
    let Some((user_id, username, role_str, disabled, revoked_at, scope_str)) = row else {
        return Ok(None);
    };
    if disabled != 0 || revoked_at.is_some() {
        return Ok(None);
    }
    let _ = sqlx::query("UPDATE api_keys SET last_used_at = datetime('now') WHERE key_hash = ?")
        .bind(&key_hash)
        .execute(pool)
        .await;
    let user_role = Role::from_str(&role_str).unwrap_or(Role::Viewer);
    let scope = KeyScope::parse(&scope_str).unwrap_or(KeyScope::Inherit);
    let role = scope.effective(user_role);
    Ok(Some(AuthIdentity {
        user_id,
        username,
        role,
        source: AuthSource::ApiKey,
    }))
}
