# Changelog

All notable changes to this project will be documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning will switch to [Semver](https://semver.org/) at the first
tagged release.

## [Unreleased]

### Added

- Multi-user authentication: signup, login, logout, /api/auth/me with
  HttpOnly/SameSite=Strict/Secure cookie sessions backed by Argon2id
  password hashes (`auth/`).
- Role-based authorization: `admin` ⊃ `operator` ⊃ `viewer`. Per-handler
  extractors (`RequireOperator`, `RequireAdmin`) gate every mutation
  and admin endpoint.
- Per-user API keys with `rl_<64-hex>` format, plaintext shown exactly
  once on creation (`/api/admin/api_keys`).
- SSO via trusted upstream headers (`X-Forwarded-User`,
  `X-Forwarded-Groups`) for deployments behind Authelia, oauth2-proxy,
  Authentik, or Pomerium. Auto-provisions users; group membership →
  role.
- `/api/admin/users` lifecycle: create / list / set-role / disable /
  reset-password / delete with self-protection.
- Server administration surface (`/api/admin/info`, `/stats`,
  `/integrity`, `/vacuum`, `/wal_checkpoint`, `/retain_activity`,
  `/metrics`, `/whoami`).
- Activity log retention with verifiable anchor hash for prunes.
- Prometheus-format `/api/admin/metrics`.
- Full /login HTML page; ops bar shows current user + logout.
- CSV bulk import with per-row error reporting and supplier
  resolution caching.
- Real camera-based scanner (BarcodeDetector + ZXing fallback) that
  hits `/api/lookup/{barcode}` and offers one-click import from
  external SKU providers.
- Pagination on every list endpoint via shared `PageQuery::resolve()`.
- Rate limit on `/api/lookup` (30 req/min default, configurable).
- End-to-end workflow integration test
  (`tests/workflow.rs::full_ops_workflow`).
- CI: rustfmt --check, clippy -D warnings, cargo test --locked,
  cargo build --release.
- `[lints]` table forbids `unsafe_code` and warns on
  `unreachable_pub`, `elided_lifetimes_in_paths`, `unused_must_use`,
  and the `clippy::all` group.

### Changed

- Audit chain encoding upgraded to v2 (canonical JSON with explicit
  null vs empty-string handling and a version tag).
- CSV formula-injection escape now neutralises leading whitespace and
  tab characters (e.g. `" =HYPERLINK(...)"`).
- `IN`-clause stock loader now chunks queries below the SQLite default
  `SQLITE_MAX_VARIABLE_NUMBER`.
- `reqwest::Client` for SKU sync is now a process-wide singleton
  (was rebuilt per call).
- All hardcoded `"api"` / `"import"` / `"admin"` audit identities
  replaced with the real authenticated user's username.
- `RACKLOG_AUTH_TOKEN` is now a *bootstrap-only* admin escape hatch:
  it works while the users table is empty and is implicitly disabled
  once the first user is created.

### Removed

- Aspirational claims that swapping `DATABASE_URL` to Postgres
  would Just Work. The runtime is hardcoded to SQLite today;
  see [`docs/postgres.md`](docs/postgres.md) for the actual list of
  blockers (~1–2 weeks of work, not a feature flag).

### Security

- Bootstrap-injection XSS closed: `serde_json` output is HTML-safe
  before splatting into a `<script>` tag.
- Constant-time comparison via `subtle` for tokens and audit hashes.
- Username enumeration on `/api/auth/login` flattened by an explicit
  hash-verify on the negative path.
- SSO header trust gated behind `RACKLOG_TRUST_FORWARDED_HEADERS=1` —
  trusting forwarded headers when no proxy is in front would let
  callers spoof identities.
