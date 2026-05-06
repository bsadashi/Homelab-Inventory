//! Authentication and authorization.
//!
//! Resolution order on every request:
//!   1. `racklog_session` cookie — primary path for humans on the
//!      dashboard. Looked up in `sessions`, expiry + revocation
//!      enforced, last_seen bumped.
//!   2. `Authorization: Bearer <token>` — first interpreted as an
//!      API key; on miss, when the `users` table is empty, accepted
//!      as the bootstrap admin token from `RACKLOG_AUTH_TOKEN`.
//!   3. `X-Forwarded-User` — only when
//!      `RACKLOG_TRUST_FORWARDED_HEADERS=1`. Auto-creates the user
//!      and maps `X-Forwarded-Groups` → role.
//!
//! Successful auth populates a request extension with `AuthIdentity`,
//! which extractors and audit-log calls read.

pub mod api_keys;
pub mod extractors;
pub mod identity;
pub mod middleware;
pub mod password;
pub mod sessions;

pub use extractors::{Authed, RequireAdmin, RequireOperator, RequireViewer};
pub use identity::{AuthIdentity, AuthSource, Role};
pub use middleware::{require_auth, SESSION_COOKIE};
