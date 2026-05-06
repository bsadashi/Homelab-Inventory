//! Database connection pool + migration runner.

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::SqlitePool;
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
