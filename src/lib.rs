//! RACKLOG — privacy-focused homelab inventory service.
//!
//! This is the library face of the binary in `src/main.rs`. It's kept
//! `pub` so integration tests can build the router against a temp
//! database without spinning up a real listener; nothing here is yet
//! considered a stable API for downstream callers.
//!
//! # Module map
//!
//! ```text
//!   audit       Tamper-evident SHA-256 chain over every mutation.
//!   auth/       Multi-user authentication: identity, password,
//!               sessions, API keys, middleware, role extractors.
//!   config      Env-driven runtime configuration.
//!   db          SQLite pool + migration runner.
//!   error       ApiError → JSON response mapping.
//!   logging     tracing-subscriber init.
//!   models      DTOs that match the dashboard's wire shape.
//!   routes/     One sub-module per URL nest. routes::serve(state)
//!               returns the fully-armed Router used by main + tests.
//!   seed        First-boot dataset loader (idempotent).
//!   sku_sync/   External SKU lookup (UPCitemDB, DigiKey).
//!   state       Cheaply-cloneable AppState (pool + cfg + providers).
//!   users       Persistence layer for the users table.
//! ```
//!
//! # Conventions
//!
//! * **Auth identity** flows through axum request extensions as
//!   [`auth::AuthIdentity`]. Mutating handlers use the
//!   [`auth::RequireOperator`] / [`auth::RequireAdmin`] extractors.
//! * **Audit trail.** Every mutation calls [`audit::log_in_tx`] (or
//!   [`audit::log`]) inside the same transaction as the data write,
//!   recording the real caller's username.
//! * **Configuration is env-driven.** [`config::Config::from_env`] is
//!   the single source of truth; tests use [`config::Config::for_test`].
//! * **Errors**: handlers return [`error::ApiResult<T>`]; the
//!   [`error::ApiError`] enum maps to RFC-7807-ish JSON responses.

pub mod audit;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod logging;
pub mod models;
pub mod routes;
pub mod seed;
pub mod sku_sync;
pub mod state;
pub mod users;
