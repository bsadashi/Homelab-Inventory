-- Multi-user authentication and authorization.
--
-- Roles:
--   admin    — full access including /api/admin/*
--   operator — full CRUD on inventory; no admin
--   viewer   — read-only
--
-- Sessions and API keys store SHA-256 hashes of the bearer secret;
-- the plaintext exists only on the wire (cookie / header) and never
-- on disk, so a leaked database backup does not yield live tokens.

CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    -- Argon2id PHC string. Empty for SSO-provisioned users (auth lives
    -- upstream; only the username + role matter on this side).
    password_hash   TEXT NOT NULL DEFAULT '',
    role            TEXT NOT NULL CHECK (role IN ('admin', 'operator', 'viewer')),
    disabled        INTEGER NOT NULL DEFAULT 0,
    -- Optional source label so admins can see where each user came
    -- from: 'signup' | 'admin' | 'sso' | 'bootstrap'.
    source          TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    last_login_at   TEXT
);
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);

CREATE TABLE IF NOT EXISTS sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    -- SHA-256 hex of the plaintext token.
    token_hash      TEXT NOT NULL UNIQUE,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at      TEXT NOT NULL,
    last_seen_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_sessions_user    ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);

CREATE TABLE IF NOT EXISTS api_keys (
    id              TEXT PRIMARY KEY,
    label           TEXT NOT NULL,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- SHA-256 hex of the plaintext key. Plaintext is shown to the
    -- caller exactly once on creation.
    key_hash        TEXT NOT NULL UNIQUE,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at    TEXT,
    revoked_at      TEXT
);
CREATE INDEX IF NOT EXISTS idx_api_keys_user ON api_keys(user_id);
