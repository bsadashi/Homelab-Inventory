//! Tamper-evident activity log.
//!
//! Each entry's hash is `SHA-256(prev_hash || ts || user || type || ref ||
//! description || payload)`. Replaying the log re-derives every hash; if
//! any field has been altered or rows have been removed the chain breaks.
//! This gives operators a cryptographic trail of every mutation without
//! relying on append-only storage primitives the database may not offer.

use crate::models::ActivityEntry;
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct AuditEvent<'a> {
    pub user: &'a str,
    pub kind: &'a str,
    pub r#ref: Option<&'a str>,
    pub description: &'a str,
    pub payload: Option<&'a serde_json::Value>,
}

/// Append a single event to the activity log, chaining its SHA-256 hash to
/// the previous entry. Runs inside the caller's transaction context when
/// invoked through `record_in_tx`; the standalone helper opens its own.
pub async fn record(pool: &SqlitePool, event: AuditEvent<'_>) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    record_in_tx(&mut tx, event).await?;
    tx.commit().await
}

pub async fn record_in_tx<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Sqlite>,
    event: AuditEvent<'_>,
) -> sqlx::Result<()> {
    let prev_hash: Option<String> =
        sqlx::query_scalar("SELECT hash FROM activity_log ORDER BY id DESC LIMIT 1")
            .fetch_optional(&mut **tx)
            .await?;

    let ts = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let payload_json = match event.payload {
        Some(v) => serde_json::to_string(v).unwrap_or_default(),
        None => String::new(),
    };

    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_deref().unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(ts.as_bytes());
    hasher.update(b"\n");
    hasher.update(event.user.as_bytes());
    hasher.update(b"\n");
    hasher.update(event.kind.as_bytes());
    hasher.update(b"\n");
    hasher.update(event.r#ref.unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(event.description.as_bytes());
    hasher.update(b"\n");
    hasher.update(payload_json.as_bytes());
    let hash = hex::encode(hasher.finalize());

    sqlx::query(
        r#"INSERT INTO activity_log
           (ts, user_name, type, ref, description, payload, prev_hash, hash)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&ts)
    .bind(event.user)
    .bind(event.kind)
    .bind(event.r#ref)
    .bind(event.description)
    .bind(if payload_json.is_empty() { None } else { Some(payload_json) })
    .bind(&prev_hash)
    .bind(&hash)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Result of replaying the activity log.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChainStatus {
    pub entries: i64,
    pub valid: bool,
    /// 0-based index of the first row whose hash failed to verify.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broken_at: Option<i64>,
    /// Hex hash of the latest valid entry, useful for external pinning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
}

/// Re-hash the activity log from scratch and report whether the chain
/// matches what's stored. Constant-time hash comparison keeps timing
/// side-channels off the table even though the data is non-secret.
pub async fn verify_chain(pool: &SqlitePool) -> sqlx::Result<ChainStatus> {
    let rows: Vec<(i64, String, String, String, Option<String>, String, Option<String>, Option<String>, String)> =
        sqlx::query_as(
            r#"SELECT id, ts, user_name, type, ref, description, payload, prev_hash, hash
               FROM activity_log ORDER BY id ASC"#,
        )
        .fetch_all(pool)
        .await?;

    let mut prev: Option<String> = None;
    let mut head: Option<String> = None;
    for (idx, (_id, ts, user, kind, r#ref, desc, payload, stored_prev, stored_hash)) in
        rows.iter().enumerate()
    {
        if stored_prev.as_deref() != prev.as_deref() {
            return Ok(ChainStatus {
                entries: rows.len() as i64,
                valid: false,
                broken_at: Some(idx as i64),
                head,
            });
        }
        let mut hasher = Sha256::new();
        hasher.update(prev.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"\n");
        hasher.update(ts.as_bytes());
        hasher.update(b"\n");
        hasher.update(user.as_bytes());
        hasher.update(b"\n");
        hasher.update(kind.as_bytes());
        hasher.update(b"\n");
        hasher.update(r#ref.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"\n");
        hasher.update(desc.as_bytes());
        hasher.update(b"\n");
        hasher.update(payload.as_deref().unwrap_or("").as_bytes());
        let recomputed = hex::encode(hasher.finalize());

        let a = recomputed.as_bytes();
        let b = stored_hash.as_bytes();
        let eq = a.len() == b.len()
            && bool::from(subtle::ConstantTimeEq::ct_eq(a, b));
        if !eq {
            return Ok(ChainStatus {
                entries: rows.len() as i64,
                valid: false,
                broken_at: Some(idx as i64),
                head,
            });
        }
        prev = Some(stored_hash.clone());
        head = prev.clone();
    }

    Ok(ChainStatus {
        entries: rows.len() as i64,
        valid: true,
        broken_at: None,
        head,
    })
}

/// Read recent activity entries (newest first).
pub async fn recent(pool: &SqlitePool, limit: i64) -> sqlx::Result<Vec<ActivityEntry>> {
    let rows: Vec<(String, String, String, Option<String>, String, Option<String>, String)> =
        sqlx::query_as(
            r#"SELECT ts, user_name, type, ref, description, prev_hash, hash
               FROM activity_log ORDER BY id DESC LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(ts, user, kind, r#ref, desc, prev_hash, hash)| ActivityEntry {
            ts,
            user,
            kind,
            r#ref,
            desc,
            hash: Some(hash),
            prev_hash,
        })
        .collect())
}
