# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository

RACKLOG — a privacy-focused homelab inventory ops platform. Single Rust binary (Axum + SQLx/SQLite) that serves both the JSON API at `/api/*` and an in-browser React/JSX dashboard at `/`. The dashboard JSX, `static/` assets and the seed dataset are all `include_bytes!`'d into the binary so the runtime image needs nothing on the host except a data volume. The full README and `CONTRIBUTING.md` carry the policy details — start there before introducing new conventions.

## Common commands

```sh
cargo run                                 # serves http://127.0.0.1:8080
cargo build --release --bin racklog       # production binary
cargo test --locked                       # full suite (unit + integration)
cargo test --test api                     # one integration file
cargo test --test api -- security_headers # one test, by filter
cargo fmt --all
cargo clippy --all-targets --locked -- -D warnings   # exact CI command
RACKLOG_STATIC_DIR=./static cargo run     # hot-reload UI changes (no rebuild)
```

CI fails on any clippy warning, any rustfmt diff, and any cargo-deny violation (`deny.toml`). `cargo clippy` here is also the CI command — local toolchains can lag, so when CI flags a lint that didn't fire locally, run with the same `--locked` flag against a fresh toolchain.

## Two binaries, one library

- `racklog` (`src/main.rs`) — HTTP server.
- `racklog-mcp` (`src/bin/mcp.rs`) — Model Context Protocol stdio bridge that wraps the REST API as MCP tools (`list_items`, `get_item`, `lookup_barcode`, `stats`, `list_locations`, `list_suppliers`, `recent_activity`, `verify_chain`, `forecast_reorder`).
- `src/lib.rs` exposes the crate as `racklog::*` so integration tests in `tests/` reach into modules directly (e.g. `racklog::auth::throttle::reset_for_tests()`).

When adding or removing a tool from `tools_descriptor()` in `src/bin/mcp.rs`, also add it to `dispatch_tool()` and update `tests/mcp.rs::EXPECTED_TOOLS`.

## Request-flow architecture

`routes::serve(state)` (`src/routes/mod.rs`) builds the fully-armed router. Layer stack, outermost → innermost:

1. `tower_http` outer infra: trace, per-request timeout (`RACKLOG_REQUEST_TIMEOUT_SECS`), body limit (`RACKLOG_MAX_BODY_BYTES`), gzip compression.
2. Default response headers: nosniff, X-Frame-Options DENY, Referrer-Policy, Permissions-Policy, HSTS, CSP (defined as `CSP` const in `routes/mod.rs`).
3. CORS — same-origin by default; widen with `RACKLOG_ALLOW_ORIGIN`.
4. `auth::middleware::require_auth` — runs on `/api/*` only.
5. Per-route extractor (`Authed` / `RequireViewer` / `RequireOperator` / `RequireAdmin`) enforces role.

`require_auth` snapshots all credentials from the request synchronously into a `Credentials` struct *before* the first `await` so the future stays `Send`. Identity resolution priority:

1. `racklog_session` cookie (HttpOnly, SameSite=Strict, Secure, 30-day TTL — DB stores SHA-256 hashes).
2. `Authorization: Bearer <token>` — first as API key (`rl_<64hex>`), then as bootstrap admin token (`RACKLOG_AUTH_TOKEN`, only valid while the users table is empty).
3. `X-Forwarded-User` / `X-Forwarded-Groups` — only when `RACKLOG_TRUST_FORWARDED_HEADERS=1`. Auto-provisions the user; `racklog-admin`/`racklog-operator` group prefixes map to roles.
4. **Pre-bootstrap fallback**: zero users + no auth_token + trust_forwarded off → an admin identity is granted to anyone. Closes as soon as any of those conditions changes.

`PUBLIC_PATHS` in `auth/middleware.rs` bypass auth entirely: `/api/healthz`, `/api/readyz`, `/api/auth/{signup,login,logout,me}`. Add to that list for any new public endpoint.

## Audit chain (security-critical, do not freelance)

Every state-changing handler must write its audit row in the *same* transaction as the data write:

```rust
let mut tx = state.pool.begin().await?;
sqlx::query("INSERT INTO ...").execute(&mut *tx).await?;
audit::log_in_tx(&mut tx, &auth.username, "kind", Some(&id), &desc).await?;
tx.commit().await?;
```

Each row carries `prev_hash` + `hash` linking it to the previous row; the boot path verifies the chain on startup and `/api/activity/verify` reproduces it on demand. Constant-time compare via `subtle`. Canonical-JSON v2 encoding is in `audit.rs::canonical_v2` — don't touch it without bumping the version int and writing a migration. `retain_activity` deliberately breaks the chain when pruning and records an *anchor hash* into the audit log itself, so a `broken_at: 0` after retention is documented behaviour, not corruption.

## Database

- SQLite-only. The schema is portable to Postgres but moving is not a feature flag — see `docs/postgres.md`.
- Migrations: `migrations/NNNN_short.sql`, zero-padded to 4 digits, monotonically increasing. `db::migrate()` is idempotent and runs at startup. The test harness reapplies every migration on a fresh tempdir DB per test, so migrations must be deterministic and self-contained.
- `IN (?, ?, …)` queries must chunk under SQLite's default `SQLITE_MAX_VARIABLE_NUMBER` of 999. The convention is `const SQLITE_IN_CHUNK: usize = 500;` — see `routes/items.rs::load_all_stock` and `routes/counts.rs::vision_ingest`.
- `RowExt` trait in `src/db.rs` provides `opt_string`, `opt_i64`, `opt_f64`, `string_or_default`, `i64_or`, `f64_or` to keep row-mapping code consistent.

## Testing

- Integration tests live in `tests/`. Each one declares `mod common;` to pull in `tests/common/mod.rs`, which exposes `Harness` plus helpers (`raw_post`, `get_with`, `post_with`, `delete_with`, `signup_user`, `expect_ok`, `expect_status`, `json`).
- `Harness` boots an in-process Axum router against a fresh SQLite tempdir per test. Variants:
  - `Harness::boot()` — seeded with the demo dataset, admin session ready.
  - `Harness::boot_with(None, false)` — empty DB, no seed.
  - `Harness::boot_open_signup()` — `RACKLOG_OPEN_SIGNUP=1` so second signup gets viewer role.
- Argon2 cost is reduced to the minimum legal params under `#[cfg(test)]` (`auth/password.rs::argon2_engine`), so tests run in seconds. Production cost is unaffected.
- `racklog-mcp` is exercised as a subprocess from `tests/mcp.rs` via `env!("CARGO_BIN_EXE_racklog-mcp")`.
- Process-wide state (login throttle, lookup token bucket) leaks across parallel tests — call `racklog::auth::throttle::reset_for_tests()` at the top of any test that asserts throttle behaviour. Note this helper is intentionally **not** `#[cfg(test)]`-gated, because integration tests link against the library compiled out of test cfg.

## Frontend

- The dashboard runs in-browser through Babel Standalone — there is **no build step**. Edit `static/*.jsx` or `static/*.js`, set `RACKLOG_STATIC_DIR=./static`, reload the page.
- The bootstrap snapshot is injected into `RACKLOG.html` as inline JSON. Escaping for `<`, `>`, `&`, U+2028, U+2029 lives in `routes/web.rs::html_safe_json` — when adding fields to that snapshot, never bypass it.
- Shared frontend helpers live on `window.RL` (defined in `static/api.js`, loaded first). `window.RL.util.escapeHtml` is the single canonical HTML escaper for any module that builds `innerHTML` strings.
- Third-party scripts (React/Babel/Three) are SRI-pinned in `RACKLOG.html`. Three.js loads via dynamic `import()` so its integrity is enforced through `<link rel="modulepreload" integrity="...">` tags. `scripts/compute-sri.sh` fetches each pinned URL and prints the SHA-384 — run it at deploy time when bumping versions.

## Config

All config flows through `Config::from_env` (`src/config.rs`). When you add a field, also add a matching default to `Config::for_test` so the test harness keeps tracking production semantics. Notable env vars are documented in `README.md`'s Configuration table — don't restate them here.

## Conventional commits

Existing history uses conventional prefixes: `feat:`, `fix:`, `chore:`, `refactor:`, `test:`, `docs:` plus an optional scope (`fix(auth):`, `feat(counts):`). Keep commits small. The session-id footer pattern (`https://claude.ai/code/session_…`) is on every commit Claude has authored on this branch — match the existing format.
