//! /api/admin — server administration surface.
//!
//! Operator endpoints for the things you need to do during the life of
//! a deployment: health detail beyond `/api/healthz`, database
//! statistics, integrity checks, on-demand vacuum / WAL checkpoint,
//! activity log retention, and Prometheus-style metrics.
//!
//! All endpoints inherit the `/api` auth gate, so they require a valid
//! bearer token whenever `RACKLOG_AUTH_TOKEN` is set. Mutating
//! endpoints additionally require `?confirm=1` to keep an accidental
//! GET / curl with no body from triggering a heavy operation.

use crate::audit;
use crate::auth::RequireAdmin;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Instant;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/info", get(info))
        .route("/stats", get(stats))
        .route("/integrity", get(integrity))
        .route("/vacuum", post(vacuum))
        .route("/wal_checkpoint", post(wal_checkpoint))
        .route("/retain_activity", post(retain_activity))
        .route("/metrics", get(metrics))
        .route("/whoami", get(whoami))
}

/// Process start time, captured on first call. Used for uptime
/// reporting in `/info` and `/metrics` so we don't have to thread
/// state.
fn started_at() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

// ---- /info ------------------------------------------------------------

#[derive(Serialize)]
struct InfoResponse {
    version: &'static str,
    uptime_seconds: u64,
    database_url_kind: &'static str,
    database_path: Option<String>,
    pool_size: u32,
    pool_idle: usize,
    audit_head: Option<String>,
    audit_version: u32,
    audit_entries: i64,
    auth_required: bool,
}

async fn info(State(state): State<AppState>) -> ApiResult<Json<InfoResponse>> {
    let chain = audit::verify_chain(&state.pool).await?;
    let database_path = sqlite_path(&state.cfg.database_url);
    Ok(Json(InfoResponse {
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: started_at().elapsed().as_secs(),
        database_url_kind: "sqlite",
        database_path,
        pool_size: state.pool.size(),
        pool_idle: state.pool.num_idle(),
        audit_head: chain.head.clone(),
        audit_version: chain.version,
        audit_entries: chain.entries,
        auth_required: state.cfg.auth_token.is_some(),
    }))
}

// ---- /stats ------------------------------------------------------------

#[derive(Serialize)]
struct StatsResponse {
    tables: Vec<TableStat>,
    database_bytes: u64,
    wal_bytes: u64,
    activity_oldest: Option<String>,
    activity_newest: Option<String>,
}

#[derive(Serialize)]
struct TableStat {
    name: &'static str,
    rows: i64,
}

async fn stats(State(state): State<AppState>) -> ApiResult<Json<StatsResponse>> {
    let tables: &[&'static str] = &[
        "items",
        "item_stock",
        "locations",
        "suppliers",
        "purchase_orders",
        "purchase_order_lines",
        "sales_orders",
        "sales_order_lines",
        "transfers",
        "transfer_lines",
        "counts",
        "activity_log",
    ];
    let mut out = Vec::with_capacity(tables.len());
    for &t in tables {
        // Identifier is from a fixed compile-time list, never user input,
        // so format!() into the SQL is safe here.
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", t))
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
        out.push(TableStat { name: t, rows: n });
    }

    let (database_bytes, wal_bytes) = file_sizes(&state.cfg.database_url);
    let oldest: Option<String> = sqlx::query_scalar("SELECT MIN(ts) FROM activity_log")
        .fetch_optional(&state.pool)
        .await?
        .flatten();
    let newest: Option<String> = sqlx::query_scalar("SELECT MAX(ts) FROM activity_log")
        .fetch_optional(&state.pool)
        .await?
        .flatten();

    Ok(Json(StatsResponse {
        tables: out,
        database_bytes,
        wal_bytes,
        activity_oldest: oldest,
        activity_newest: newest,
    }))
}

// ---- /integrity --------------------------------------------------------

#[derive(Serialize)]
struct IntegrityResponse {
    sqlite_ok: bool,
    sqlite_message: String,
    audit_chain_valid: bool,
    audit_entries: i64,
    audit_head: Option<String>,
    audit_broken_at: Option<i64>,
    audit_version: u32,
}

async fn integrity(State(state): State<AppState>) -> ApiResult<Json<IntegrityResponse>> {
    // PRAGMA integrity_check returns one row "ok" when healthy, or
    // multiple rows describing problems otherwise.
    let rows: Vec<(String,)> = sqlx::query_as("PRAGMA integrity_check")
        .fetch_all(&state.pool)
        .await?;
    let lines: Vec<String> = rows.into_iter().map(|(s,)| s).collect();
    let sqlite_ok = lines.len() == 1 && lines[0].eq_ignore_ascii_case("ok");

    let chain = audit::verify_chain(&state.pool).await?;

    Ok(Json(IntegrityResponse {
        sqlite_ok,
        sqlite_message: if sqlite_ok {
            "ok".into()
        } else {
            // Cap to avoid arbitrarily large response bodies.
            lines.into_iter().take(20).collect::<Vec<_>>().join("; ")
        },
        audit_chain_valid: chain.valid,
        audit_entries: chain.entries,
        audit_head: chain.head,
        audit_broken_at: chain.broken_at,
        audit_version: chain.version,
    }))
}

// ---- /vacuum -----------------------------------------------------------

#[derive(Deserialize)]
struct ConfirmQuery {
    #[serde(default)]
    confirm: Option<String>,
}

fn require_confirm(q: &ConfirmQuery, op: &str) -> ApiResult<()> {
    match q.confirm.as_deref() {
        Some("1") | Some("true") | Some("yes") => Ok(()),
        _ => Err(ApiError::BadRequest(format!(
            "{op} is destructive; pass ?confirm=1 to proceed"
        ))),
    }
}

#[derive(Serialize)]
struct OperationResult {
    ok: bool,
    operation: &'static str,
    duration_ms: u128,
    notes: String,
}

async fn vacuum(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Query(q): Query<ConfirmQuery>,
) -> ApiResult<Json<OperationResult>> {
    require_confirm(&q, "vacuum")?;
    let started = Instant::now();
    sqlx::query("VACUUM")
        .execute(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    audit::log(
        &state.pool,
        &auth.username,
        "admin.vacuum",
        None,
        "VACUUM completed",
    )
    .await
    .ok();
    Ok(Json(OperationResult {
        ok: true,
        operation: "vacuum",
        duration_ms: started.elapsed().as_millis(),
        notes: "database file rewritten; free space reclaimed".into(),
    }))
}

async fn wal_checkpoint(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Query(q): Query<ConfirmQuery>,
) -> ApiResult<Json<OperationResult>> {
    require_confirm(&q, "wal_checkpoint")?;
    let started = Instant::now();
    // TRUNCATE flushes the WAL into the main file and shrinks it to 0 bytes.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    audit::log(
        &state.pool,
        &auth.username,
        "admin.wal_checkpoint",
        None,
        "WAL checkpoint TRUNCATE completed",
    )
    .await
    .ok();
    Ok(Json(OperationResult {
        ok: true,
        operation: "wal_checkpoint",
        duration_ms: started.elapsed().as_millis(),
        notes: "WAL flushed to main DB and truncated to 0 bytes".into(),
    }))
}

// ---- /retain_activity --------------------------------------------------

#[derive(Deserialize)]
struct RetainQuery {
    /// Keep entries strictly newer than this many days. Default 365.
    #[serde(default)]
    days: Option<i64>,
    #[serde(default)]
    confirm: Option<String>,
    /// `?dry_run=1` reports what would be deleted without doing it.
    #[serde(default)]
    dry_run: Option<String>,
}

#[derive(Serialize)]
struct RetainResponse {
    /// Cut-off timestamp (UTC). Entries with `ts < cutoff` are
    /// candidates for deletion.
    cutoff_ts: String,
    days: i64,
    candidates: i64,
    deleted: i64,
    /// Hash of the last entry that survives; pin this externally to
    /// detect post-truncation tampering. After deletion the chain is
    /// "anchored" at this hash — verify_chain will report broken_at=0
    /// because the new first row's prev_hash points at a row that is
    /// no longer present, which is the documented retention behaviour.
    anchor: Option<String>,
    dry_run: bool,
}

async fn retain_activity(
    State(state): State<AppState>,
    RequireAdmin(auth): RequireAdmin,
    Query(q): Query<RetainQuery>,
) -> ApiResult<Json<RetainResponse>> {
    let dry_run = matches!(q.dry_run.as_deref(), Some("1") | Some("true") | Some("yes"));
    if !dry_run {
        require_confirm(
            &ConfirmQuery {
                confirm: q.confirm.clone(),
            },
            "retain_activity",
        )?;
    }
    let days = q.days.unwrap_or(365).max(1);
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
    let cutoff_ts = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let candidates: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log WHERE ts < ?")
        .bind(&cutoff_ts)
        .fetch_one(&state.pool)
        .await?;

    if dry_run || candidates == 0 {
        return Ok(Json(RetainResponse {
            cutoff_ts,
            days,
            candidates,
            deleted: 0,
            anchor: None,
            dry_run,
        }));
    }

    // Capture the hash of the last row we're about to delete so the
    // operator can pin a verifiable anchor for the truncated tail.
    let anchor: Option<String> =
        sqlx::query_scalar("SELECT hash FROM activity_log WHERE ts < ? ORDER BY id DESC LIMIT 1")
            .bind(&cutoff_ts)
            .fetch_optional(&state.pool)
            .await?;

    let mut tx = state.pool.begin().await?;
    let res = sqlx::query("DELETE FROM activity_log WHERE ts < ?")
        .bind(&cutoff_ts)
        .execute(&mut *tx)
        .await?;
    let deleted = res.rows_affected() as i64;

    // Record the retention itself in the chain so a future operator
    // can see when the truncation happened.
    audit::log_in_tx(
        &mut tx,
        &auth.username,
        "admin.retain_activity",
        anchor.as_deref(),
        &format!(
            "Pruned {} activity rows older than {} (anchor: {})",
            deleted,
            cutoff_ts,
            anchor.as_deref().unwrap_or("none"),
        ),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(RetainResponse {
        cutoff_ts,
        days,
        candidates,
        deleted,
        anchor,
        dry_run: false,
    }))
}

// ---- /metrics (Prometheus) ---------------------------------------------

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let chain = audit::verify_chain(&state.pool).await.ok();
    let mut s = String::new();
    s.push_str("# HELP racklog_uptime_seconds Process uptime in seconds.\n");
    s.push_str("# TYPE racklog_uptime_seconds counter\n");
    s.push_str(&format!(
        "racklog_uptime_seconds {}\n",
        started_at().elapsed().as_secs()
    ));

    s.push_str("# HELP racklog_pool_size Open SQLite pool connections.\n");
    s.push_str("# TYPE racklog_pool_size gauge\n");
    s.push_str(&format!("racklog_pool_size {}\n", state.pool.size()));

    s.push_str("# HELP racklog_pool_idle Idle SQLite pool connections.\n");
    s.push_str("# TYPE racklog_pool_idle gauge\n");
    s.push_str(&format!("racklog_pool_idle {}\n", state.pool.num_idle()));

    if let Some(chain) = chain {
        s.push_str("# HELP racklog_audit_entries Number of activity log entries.\n");
        s.push_str("# TYPE racklog_audit_entries gauge\n");
        s.push_str(&format!("racklog_audit_entries {}\n", chain.entries));
        s.push_str("# HELP racklog_audit_chain_valid 1 if the audit chain verifies.\n");
        s.push_str("# TYPE racklog_audit_chain_valid gauge\n");
        s.push_str(&format!(
            "racklog_audit_chain_valid {}\n",
            if chain.valid { 1 } else { 0 }
        ));
    }

    let counts: &[&str] = &[
        "items",
        "locations",
        "suppliers",
        "purchase_orders",
        "sales_orders",
        "transfers",
        "counts",
    ];
    for &t in counts {
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", t))
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
        s.push_str(&format!(
            "# HELP racklog_{}_total Number of rows in {}.\n# TYPE racklog_{}_total gauge\nracklog_{}_total {}\n",
            t, t, t, t, n
        ));
    }

    let (db_size, wal_size) = file_sizes(&state.cfg.database_url);
    s.push_str("# HELP racklog_database_bytes On-disk size of the SQLite main file.\n");
    s.push_str("# TYPE racklog_database_bytes gauge\n");
    s.push_str(&format!("racklog_database_bytes {}\n", db_size));
    s.push_str("# HELP racklog_wal_bytes On-disk size of the SQLite write-ahead log.\n");
    s.push_str("# TYPE racklog_wal_bytes gauge\n");
    s.push_str(&format!("racklog_wal_bytes {}\n", wal_size));

    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        s,
    )
}

// ---- /whoami -----------------------------------------------------------

#[derive(Serialize)]
struct WhoamiResponse {
    authenticated: bool,
    auth_required: bool,
    /// Time of this call. Useful for clock-skew checks against the
    /// caller.
    server_time: String,
}

async fn whoami(State(state): State<AppState>) -> Json<WhoamiResponse> {
    // The auth middleware would have already 401'd if a token was
    // required and missing. Reaching here means the request passed
    // (or no token is required at all).
    let auth_required = state.cfg.auth_token.is_some();
    Json(WhoamiResponse {
        authenticated: auth_required,
        auth_required,
        server_time: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

// ---- helpers -----------------------------------------------------------

fn sqlite_path(database_url: &str) -> Option<String> {
    let raw = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .unwrap_or(database_url);
    let path = raw.split('?').next().unwrap_or(raw);
    if path.is_empty() || path == ":memory:" {
        None
    } else {
        Some(path.to_string())
    }
}

fn file_sizes(database_url: &str) -> (u64, u64) {
    let path = match sqlite_path(database_url) {
        Some(p) => p,
        None => return (0, 0),
    };
    let main = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let wal = std::fs::metadata(format!("{}-wal", path))
        .map(|m| m.len())
        .unwrap_or(0);
    (main, wal)
}

// Touch started_at on first request through any handler so the OnceLock
// is initialised at boot order (the first /healthz probe will trip it).
#[allow(dead_code)]
pub fn touch_uptime() {
    let _ = started_at();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_confirm_accepts_truthy() {
        let q = ConfirmQuery {
            confirm: Some("1".into()),
        };
        assert!(require_confirm(&q, "x").is_ok());
        let q = ConfirmQuery {
            confirm: Some("true".into()),
        };
        assert!(require_confirm(&q, "x").is_ok());
    }
    #[test]
    fn require_confirm_rejects_missing() {
        let q = ConfirmQuery { confirm: None };
        assert!(require_confirm(&q, "x").is_err());
    }
    #[test]
    fn require_confirm_rejects_falsy() {
        let q = ConfirmQuery {
            confirm: Some("no".into()),
        };
        assert!(require_confirm(&q, "x").is_err());
    }
}
