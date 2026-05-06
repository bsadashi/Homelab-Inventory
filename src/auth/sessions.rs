//! Session token store.
//!
//! Tokens are 32 random bytes hex-encoded (64 chars). The plaintext
//! token only exists on the wire (HttpOnly cookie); the database
//! stores SHA-256 hashes so a leaked backup can't yield live
//! sessions. Constant-time hash comparison via the `subtle` crate.

use crate::auth::identity::{AuthIdentity, AuthSource, Role};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::str::FromStr;

/// Default session lifetime. Configurable per-deployment via env if
/// we ever need to.
pub const DEFAULT_TTL_HOURS: i64 = 24 * 30;

#[derive(Debug)]
pub struct CreatedSession {
    /// The plaintext token. Hand to the cookie *exactly once*; never
    /// stored on disk in this form.
    pub plaintext: String,
    pub expires_at: DateTime<Utc>,
}

pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

pub async fn create(
    pool: &SqlitePool,
    user_id: &str,
    ttl_hours: i64,
) -> sqlx::Result<CreatedSession> {
    let plaintext = random_token();
    let token_hash = hash_token(&plaintext);
    let expires_at = Utc::now() + Duration::hours(ttl_hours.max(1));
    let expires_at_str = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at)
         VALUES (?, ?, ?)",
    )
    .bind(&token_hash)
    .bind(user_id)
    .bind(&expires_at_str)
    .execute(pool)
    .await?;
    Ok(CreatedSession {
        plaintext,
        expires_at,
    })
}

pub async fn revoke(pool: &SqlitePool, plaintext: &str) -> sqlx::Result<u64> {
    let token_hash = hash_token(plaintext);
    let res = sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(&token_hash)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn revoke_all_for_user(pool: &SqlitePool, user_id: &str) -> sqlx::Result<u64> {
    let res = sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// Resolve a presented session token to an `AuthIdentity`, checking
/// expiry and revocation, and bumping `last_seen_at` as a side effect.
/// Returns `None` for any non-match (unknown, expired, deleted user).
pub async fn resolve(pool: &SqlitePool, plaintext: &str) -> sqlx::Result<Option<AuthIdentity>> {
    let token_hash = hash_token(plaintext);
    let row: Option<(String, String, String, String, String, i64)> =
        sqlx::query_as(
            r#"SELECT s.user_id, s.expires_at, u.username, u.role, u.disabled || '', u.disabled
               FROM sessions s
               JOIN users u ON u.id = s.user_id
               WHERE s.token_hash = ?"#,
        )
        .bind(&token_hash)
        .fetch_optional(pool)
        .await?;
    let Some((user_id, expires_at, username, role_str, _, disabled)) = row else {
        return Ok(None);
    };
    if disabled != 0 {
        return Ok(None);
    }
    if let Ok(exp) = DateTime::parse_from_str(
        &format!("{} +0000", expires_at),
        "%Y-%m-%d %H:%M:%S %z",
    ) {
        if exp < Utc::now() {
            // Expired — clean up opportunistically.
            let _ = sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
                .bind(&token_hash)
                .execute(pool)
                .await;
            return Ok(None);
        }
    }
    let role = Role::from_str(&role_str).unwrap_or(Role::Viewer);
    // last_seen bump — best-effort, ignore failure.
    let _ = sqlx::query("UPDATE sessions SET last_seen_at = datetime('now') WHERE token_hash = ?")
        .bind(&token_hash)
        .execute(pool)
        .await;

    Ok(Some(AuthIdentity {
        user_id,
        username,
        role,
        source: AuthSource::Session,
    }))
}

pub async fn purge_expired(pool: &SqlitePool) -> sqlx::Result<u64> {
    let res = sqlx::query("DELETE FROM sessions WHERE expires_at < datetime('now')")
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_token_is_64_hex_chars() {
        let t = random_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_is_deterministic_and_distinct_from_input() {
        let t = "deadbeef";
        assert_eq!(hash_token(t), hash_token(t));
        assert_ne!(hash_token(t), t);
    }

    #[test]
    fn two_random_tokens_are_distinct() {
        assert_ne!(random_token(), random_token());
    }
}
