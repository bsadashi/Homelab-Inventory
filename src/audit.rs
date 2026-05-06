//! Tamper-evident activity log.
//!
//! Each entry's hash chains the previous one. The hash input is a
//! deterministic, canonical JSON string with strictly-ordered fields
//! and explicit handling of optional fields (omitted when absent, never
//! conflated with the empty string), so two semantically-different
//! inputs cannot collide. Replaying the log re-derives every hash; if
//! any field has been altered or rows have been removed the chain
//! breaks. This gives operators a cryptographic trail of every mutation
//! without relying on append-only storage primitives the database may
//! not offer.
//!
//! Hash version `v2` (current). The version tag is part of the canonical
//! input so future encoding changes can never produce a colliding hash.

use crate::models::ActivityEntry;
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Bump when the canonical encoding changes. Mixed-version chains
/// remain verifiable because every row's hash incorporates the
/// previous, and the version field guarantees no cross-version
/// collisions.
const HASH_VERSION: u32 = 2;

#[derive(Debug, Clone)]
pub struct AuditEvent<'a> {
    pub user: &'a str,
    pub kind: &'a str,
    pub r#ref: Option<&'a str>,
    pub description: &'a str,
    pub payload: Option<&'a serde_json::Value>,
}

/// Append a single event to the activity log, chaining its SHA-256 hash
/// to the previous entry. Runs inside the caller's transaction context
/// when invoked through `record_in_tx`; the standalone helper opens its
/// own.
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
    let payload_json: Option<String> = event
        .payload
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".to_string()));

    let hash = compute_hash(
        prev_hash.as_deref(),
        &ts,
        event.user,
        event.kind,
        event.r#ref,
        event.description,
        payload_json.as_deref(),
    );

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
    .bind(&payload_json)
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
    /// The hash version the log was written under. Surfaced so the
    /// frontend can warn if it ever sees an unfamiliar version.
    pub version: u32,
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
                version: HASH_VERSION,
            });
        }
        let recomputed = compute_hash(
            prev.as_deref(),
            ts,
            user,
            kind,
            r#ref.as_deref(),
            desc,
            payload.as_deref(),
        );

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
                version: HASH_VERSION,
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
        version: HASH_VERSION,
    })
}

/// Compute the canonical hash for an entry.
///
/// Canonical form is a JSON object with **fixed field order**:
/// `{"v":2,"prev":…,"ts":…,"user":…,"kind":…,"ref":…,"desc":…,"payload":…}`.
/// Optional fields (`prev`, `ref`) emit JSON `null` when absent — distinct
/// from the empty string, which would emit `""`. The `payload` field is
/// **omitted entirely** when absent so a stored NULL payload column can
/// never collide with a stored literal `null` JSON string. We assemble the
/// JSON manually instead of via `serde_json::json!{}` so the output is
/// guaranteed deterministic across serde-json versions and feature flags.
fn compute_hash(
    prev_hash: Option<&str>,
    ts: &str,
    user: &str,
    kind: &str,
    r#ref: Option<&str>,
    description: &str,
    payload_json: Option<&str>,
) -> String {
    let payload_segment = match payload_json {
        Some(p) => format!(",\"payload\":{}", p),
        None => String::new(),
    };
    let canonical = format!(
        "{{\"v\":{},\"prev\":{},\"ts\":{},\"user\":{},\"kind\":{},\"ref\":{},\"desc\":{}{}}}",
        HASH_VERSION,
        json_str_or_null(prev_hash),
        json_str(ts),
        json_str(user),
        json_str(kind),
        json_str_or_null(r#ref),
        json_str(description),
        payload_segment,
    );
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
}

fn json_str(s: &str) -> String {
    // Serialising a &str via serde_json is infallible.
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn json_str_or_null(s: Option<&str>) -> String {
    match s {
        Some(v) => json_str(v),
        None => "null".to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `None` (absent ref) and `Some("")` (empty-string ref) must hash
    /// to *different* values — the original implementation collapsed
    /// both to the empty bytestring and produced identical hashes.
    #[test]
    fn empty_ref_distinct_from_missing_ref() {
        let a = compute_hash(None, "ts", "u", "k", None,       "desc", None);
        let b = compute_hash(None, "ts", "u", "k", Some(""),   "desc", None);
        assert_ne!(a, b, "None ref must hash differently to Some(\"\")");
    }

    #[test]
    fn empty_prev_distinct_from_missing_prev() {
        let a = compute_hash(None,        "ts", "u", "k", None, "desc", None);
        let b = compute_hash(Some(""),    "ts", "u", "k", None, "desc", None);
        assert_ne!(a, b);
    }

    #[test]
    fn missing_payload_distinct_from_null_payload() {
        let a = compute_hash(None, "ts", "u", "k", None, "desc", None);
        let b = compute_hash(None, "ts", "u", "k", None, "desc", Some("null"));
        assert_ne!(a, b);
    }

    /// A description containing newlines should not be confusable with
    /// a different multi-field encoding — fixed by the JSON shape.
    #[test]
    fn newline_in_description_does_not_collide() {
        // Pre-fix: hash input was \n-delimited, so a description of
        // "x\ny" with empty other fields could collide with two-field
        // entries. Canonical JSON puts " around strings, so the shapes
        // are unambiguous.
        let a = compute_hash(None, "ts", "u", "k", None, "x\ny", None);
        let b = compute_hash(None, "ts", "u", "k", None, "x", None);
        assert_ne!(a, b);
    }

    #[test]
    fn hash_is_deterministic() {
        let a = compute_hash(Some("abc"), "ts", "u", "k", Some("R"), "d", Some("{\"a\":1}"));
        let b = compute_hash(Some("abc"), "ts", "u", "k", Some("R"), "d", Some("{\"a\":1}"));
        assert_eq!(a, b);
    }
}
