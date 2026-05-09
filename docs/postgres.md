# PostgreSQL support — current status and the work required

**Status: not implemented.** The runtime is hardcoded to SQLite; pointing
`DATABASE_URL` at a Postgres instance will not work on the current
binary. The schema *is* mostly portable, but adopting Postgres needs
a deliberate refactor across roughly six places in the code. This
document exists so the next person who needs HA knows what they're
signing up for.

## What works today

- Migrations use only ANSI SQL plus a couple of SQLite-specific bits
  (the `JSON1` `json_array_length()` call, `datetime('now')`,
  `INTEGER PRIMARY KEY AUTOINCREMENT`). The rest is portable.
- Every query is parameterised — no string concatenation of user
  input — and uses `?` placeholders, which both backends accept.
- `Db = SqlitePool`, exposed once in `src/db.rs`. Most call sites
  type their pool argument as `&sqlx::SqlitePool`.

## What doesn't

These are the actual barriers to adopting Postgres, in order of
intrusiveness:

### 1. The pool type is concrete

`src/db.rs` defines `pub type Db = SqlitePool;` and 30+ call sites
across `src/routes/`, `src/audit.rs`, `src/auth/`, and `src/users.rs`
take `&SqlitePool` directly. To support both backends:

- Either alias `Db` to `sqlx::AnyPool` and switch the entire codebase
  to it, accepting `Any`'s loss of statement caching and some
  per-backend fast paths.
- Or feature-gate (`#[cfg(feature = "postgres")] type Db = PgPool;`)
  and split the codebase into two compile paths. Less runtime
  flexibility, more conditional compilation.

### 2. SQLite-specific SQL fragments

Greppable today:

- `datetime('now')` — replace with `now()` (Postgres) or `CURRENT_TIMESTAMP`.
- `json_array_length(serials)` (in `routes/stats.rs`) — Postgres equivalent
  is `jsonb_array_length(serials::jsonb)`.
- `INTEGER PRIMARY KEY AUTOINCREMENT` — Postgres needs `BIGSERIAL`.
- `PRAGMA integrity_check`, `PRAGMA wal_checkpoint(TRUNCATE)`, `VACUUM`
  in `src/routes/admin.rs` — these are SQLite-only. Postgres
  equivalents (`VACUUM` works, integrity is pg-specific, no WAL
  checkpoint) need conditional implementations.
- `mode=rwc` in the connection URL.

### 3. Type encoding for the JSON-shaped columns

`items.variants`, `items.lots`, `items.tags`, `item_stock.serials`, and
`activity_log.payload` are `TEXT NOT NULL DEFAULT '[]'` in SQLite and
serialised by the application. Postgres has a real `JSONB` type that
would let queries reach inside them. Keeping them as `TEXT` works on
both backends but throws away a real Postgres advantage.

### 4. The audit chain hash format

The canonical hash input is application-side (`src/audit.rs`), so it's
backend-agnostic. Good — no change needed.

### 5. Migrations are single-file and SQLite-flavoured

`sqlx::migrate!` happily applies any `.sql` file, but the existing
`0001_initial.sql` won't apply to Postgres unmodified (mostly the
auto-increment + integrity-check syntax above). A Postgres deployment
needs its own migration directory or per-backend `#[cfg]`-gated
runtime variants.

### 6. The test harness

`tests/common/mod.rs` calls `tempfile::tempdir()` and builds a
`sqlite://…` URL. To run integration tests against Postgres in CI
we'd need a `testcontainers`-based ephemeral Postgres + a way to
parameterise the harness over backend.

## Concrete plan for whoever takes this on

1. Introduce `feature = "postgres"` in `Cargo.toml` and `[features]`
   it onto `sqlx`.
2. Behind `cfg(feature = "postgres")`, alias `Db = PgPool` and rewrite
   `db::connect`. Keep the SQLite default.
3. Sweep `datetime('now')` → `CURRENT_TIMESTAMP`. Both backends accept
   it.
4. Add a `migrations-postgres/` directory; pick the right one at
   compile-time via `cfg`.
5. Conditionally compile `/api/admin/wal_checkpoint` and the SQLite
   `PRAGMA integrity_check` path. Provide Postgres equivalents.
6. Add a CI matrix job that runs `cargo build --features postgres
   --no-default-features` and (ideally) integration tests against an
   ephemeral Postgres.
7. Update README + `deploy/kubernetes/README.md` to actually point at
   how to switch.

This is a 1–2 week project, not a feature flag. Don't pretend
otherwise in the README until the work is done.
