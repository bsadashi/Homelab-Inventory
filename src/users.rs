//! Persistence layer for the `users` table.

use crate::auth::identity::Role;
use serde::Serialize;
use sqlx::SqlitePool;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize)]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    pub role: Role,
    pub disabled: bool,
    pub source: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserPublic {
    pub id: String,
    pub username: String,
    pub role: Role,
    pub disabled: bool,
}

impl From<UserRecord> for UserPublic {
    fn from(r: UserRecord) -> Self {
        Self {
            id: r.id,
            username: r.username,
            role: r.role,
            disabled: r.disabled,
        }
    }
}

pub async fn count(pool: &SqlitePool) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
}

pub async fn find_by_username(
    pool: &SqlitePool,
    username: &str,
) -> sqlx::Result<Option<(UserRecord, String /* password_hash */)>> {
    let row: Option<(
        String,
        String,
        String,
        String,
        i64,
        Option<String>,
        String,
        String,
        Option<String>,
    )> = sqlx::query_as(
        r#"SELECT id, username, password_hash, role, disabled, source,
                  created_at, updated_at, last_login_at
           FROM users WHERE username = ?"#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(
            id,
            username,
            password_hash,
            role,
            disabled,
            source,
            created_at,
            updated_at,
            last_login_at,
        )| {
            let user = UserRecord {
                id,
                username,
                role: Role::from_str(&role).unwrap_or(Role::Viewer),
                disabled: disabled != 0,
                source,
                created_at,
                updated_at,
                last_login_at,
            };
            (user, password_hash)
        },
    ))
}

pub async fn find_by_id(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<UserRecord>> {
    let row: Option<(
        String,
        String,
        String,
        i64,
        Option<String>,
        String,
        String,
        Option<String>,
    )> = sqlx::query_as(
        r#"SELECT id, username, role, disabled, source,
                      created_at, updated_at, last_login_at
               FROM users WHERE id = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(id, username, role, disabled, source, created_at, updated_at, last_login_at)| {
            UserRecord {
                id,
                username,
                role: Role::from_str(&role).unwrap_or(Role::Viewer),
                disabled: disabled != 0,
                source,
                created_at,
                updated_at,
                last_login_at,
            }
        },
    ))
}

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<UserRecord>> {
    let rows: Vec<(
        String,
        String,
        String,
        i64,
        Option<String>,
        String,
        String,
        Option<String>,
    )> = sqlx::query_as(
        r#"SELECT id, username, role, disabled, source,
                      created_at, updated_at, last_login_at
               FROM users ORDER BY username COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, username, role, disabled, source, created_at, updated_at, last_login_at)| {
                UserRecord {
                    id,
                    username,
                    role: Role::from_str(&role).unwrap_or(Role::Viewer),
                    disabled: disabled != 0,
                    source,
                    created_at,
                    updated_at,
                    last_login_at,
                }
            },
        )
        .collect())
}

pub async fn create(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
    role: Role,
    source: &str,
) -> sqlx::Result<UserRecord> {
    let id = format!("u-{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        r#"INSERT INTO users (id, username, password_hash, role, source)
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(username)
    .bind(password_hash)
    .bind(role.as_str())
    .bind(source)
    .execute(pool)
    .await?;
    Ok(find_by_id(pool, &id)
        .await?
        .expect("just-inserted user must exist"))
}

pub async fn set_role(pool: &SqlitePool, id: &str, role: Role) -> sqlx::Result<u64> {
    let res = sqlx::query("UPDATE users SET role = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(role.as_str())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn set_disabled(pool: &SqlitePool, id: &str, disabled: bool) -> sqlx::Result<u64> {
    let res =
        sqlx::query("UPDATE users SET disabled = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(if disabled { 1 } else { 0 })
            .bind(id)
            .execute(pool)
            .await?;
    Ok(res.rows_affected())
}

pub async fn set_password_hash(pool: &SqlitePool, id: &str, hash: &str) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE users SET password_hash = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(hash)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn touch_last_login(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET last_login_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> sqlx::Result<u64> {
    let res = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}
