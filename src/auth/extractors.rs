//! Per-handler axum extractors that enforce role policy.
//!
//! Usage:
//!   async fn handler(Authed(user): Authed) { ... }      // any authed
//!   async fn handler(_: RequireOperator) { ... }        // ≥ operator
//!
//! axum 0.7's FromRequestParts uses async-trait, hence the macro.

use crate::auth::identity::{AuthIdentity, Role};
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;

/// Pulls the auth identity from request extensions. Returns 401 if
/// the middleware didn't insert one (i.e. anonymous request).
pub struct Authed(pub AuthIdentity);

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for Authed {
    type Rejection = StatusCode;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthIdentity>()
            .cloned()
            .map(Authed)
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

macro_rules! role_extractor {
    ($name:ident, $role:expr, $doc:literal) => {
        #[doc = $doc]
        pub struct $name(pub AuthIdentity);

        #[async_trait]
        impl<S: Send + Sync> FromRequestParts<S> for $name {
            type Rejection = StatusCode;
            async fn from_request_parts(
                parts: &mut Parts,
                _state: &S,
            ) -> Result<Self, Self::Rejection> {
                let id = parts
                    .extensions
                    .get::<AuthIdentity>()
                    .cloned()
                    .ok_or(StatusCode::UNAUTHORIZED)?;
                if id.role.at_least($role) {
                    Ok($name(id))
                } else {
                    Err(StatusCode::FORBIDDEN)
                }
            }
        }
    };
}

role_extractor!(
    RequireViewer,
    Role::Viewer,
    "Reads only — every authed user is at least Viewer."
);
role_extractor!(
    RequireOperator,
    Role::Operator,
    "Allowed to mutate inventory."
);
role_extractor!(
    RequireAdmin,
    Role::Admin,
    "Allowed to manage users + reach /api/admin/*."
);
