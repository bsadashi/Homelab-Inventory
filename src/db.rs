//! Database connection pool, migration runner, and a small `RowExt`
//! trait that collapses the noisy `r.try_get::<T, _>("col").ok().
//! flatten().unwrap_or(default)` chains repeated across every routes
//! file into a single readable call.

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteRow, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use std::time::Duration;

pub type Db = SqlitePool;

/// Build a SQLite pool tuned for a small, durable, single-writer service.
/// We enable WAL for read concurrency, NORMAL synchronous as a sensible
/// durability/throughput trade for inventory data, and `mode=rwc` so the
/// container can autocreate its database file on first boot.
pub async fn connect(database_url: &str) -> anyhow::Result<Db> {
    ensure_data_dir(database_url)?;

    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(16)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;

    Ok(pool)
}

/// Apply embedded migrations. `sqlx::migrate!` reads the `migrations/`
/// directory at compile time so the binary is fully self-contained.
pub async fn migrate(pool: &Db) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Best-effort creation of the parent directory for a `sqlite://` URL so
/// containers booting against an empty volume Just Work.
fn ensure_data_dir(database_url: &str) -> anyhow::Result<()> {
    let path = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .unwrap_or(database_url);

    let path = path.split('?').next().unwrap_or(path);
    if path.is_empty() || path == ":memory:" {
        return Ok(());
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).ok();
        }
    }
    Ok(())
}

/// Ergonomic accessors for `SqliteRow`. The hand-rolled
/// `r.try_get::<T, _>("col").ok().flatten().unwrap_or(default)` chains
/// repeated across every routes file collapse to e.g. `r.i64_or("qty",
/// 0)` here, with a single source of truth for the "missing/wrong-type
/// column → default" semantics.
pub trait RowExt {
    /// Fetch an `Option<String>` column, normalising any decode error
    /// (column missing, NULL, wrong type) to `None`.
    fn opt_string(&self, col: &str) -> Option<String>;
    /// Fetch an `Option<i64>` column with the same semantics.
    fn opt_i64(&self, col: &str) -> Option<i64>;
    /// Fetch an `Option<f64>` column with the same semantics.
    fn opt_f64(&self, col: &str) -> Option<f64>;
    /// Fetch a String column, defaulting to an empty `String` on any
    /// decode error. Use only when an empty string is a sensible
    /// fallback for the calling code path.
    fn string_or_default(&self, col: &str) -> String;
    /// Fetch an `i64` column, defaulting to `default` on any decode
    /// error or NULL value.
    fn i64_or(&self, col: &str, default: i64) -> i64;
    /// Fetch an `f64` column, defaulting to `default` on any decode
    /// error or NULL value.
    fn f64_or(&self, col: &str, default: f64) -> f64;
}

impl RowExt for SqliteRow {
    fn opt_string(&self, col: &str) -> Option<String> {
        self.try_get::<Option<String>, _>(col).ok().flatten()
    }
    fn opt_i64(&self, col: &str) -> Option<i64> {
        self.try_get::<Option<i64>, _>(col).ok().flatten()
    }
    fn opt_f64(&self, col: &str) -> Option<f64> {
        self.try_get::<Option<f64>, _>(col).ok().flatten()
    }
    fn string_or_default(&self, col: &str) -> String {
        self.opt_string(col).unwrap_or_default()
    }
    fn i64_or(&self, col: &str, default: i64) -> i64 {
        self.try_get::<i64, _>(col).unwrap_or(default)
    }
    fn f64_or(&self, col: &str, default: f64) -> f64 {
        self.try_get::<f64, _>(col).unwrap_or(default)
    }
}
