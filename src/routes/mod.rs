//! Top-level router. Each domain area gets its own sub-module so the file
//! tree mirrors the URL tree.

pub mod activity;
pub mod admin;
pub mod admin_users;
pub mod auth;
pub mod bootstrap;
pub mod counts;
pub mod exports;
pub mod health;
pub mod items;
pub mod locations;
pub mod lookup;
pub mod pagination;
pub mod purchase_orders;
pub mod sales_orders;
pub mod stats;
pub mod suppliers;
pub mod transfers;
pub mod web;

use crate::auth as crate_auth;
use crate::config::AllowOrigin;
use crate::state::AppState;
use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::middleware;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

pub fn build(state: AppState) -> Router {
    let cors = build_cors(&state.cfg.allow_origin);
    let api = api_routes()
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate_auth::require_auth,
        ))
        .layer(cors);

    Router::new()
        .nest("/api", api)
        // Frontend bundle. Stays outside the API auth middleware so the
        // browser can load HTML/CSS/JS without a Bearer header — privacy
        // for actual inventory data is enforced by /api auth.
        .merge(web::router())
        // Defence-in-depth response headers applied to every response.
        // CSP is intentionally restrictive but permits the inline scripts
        // the in-browser Babel demo needs; tighten further by serving a
        // pre-compiled bundle and removing `unsafe-inline`.
        .layer(set_header(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .layer(set_header(
            HeaderName::from_static("x-frame-options"),
            "DENY",
        ))
        .layer(set_header(
            HeaderName::from_static("referrer-policy"),
            "no-referrer",
        ))
        .layer(set_header(
            HeaderName::from_static("permissions-policy"),
            "geolocation=(), microphone=(), camera=(self)",
        ))
        .layer(set_header(
            HeaderName::from_static("strict-transport-security"),
            "max-age=31536000; includeSubDomains",
        ))
        .with_state(state)
}

/// Build the fully-armed router, including the outer infrastructure
/// layers (request body limit, per-request timeout, gzip compression,
/// HTTP tracing). Both `main.rs` and the integration test harness
/// build through this so layer regressions are caught in CI.
pub fn serve(state: AppState) -> Router {
    let cfg = state.cfg.clone();
    build(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::timeout::TimeoutLayer::new(cfg.request_timeout))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            cfg.max_body_bytes,
        ))
        .layer(tower_http::compression::CompressionLayer::new())
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .nest("/auth", auth::router())
        .nest("/bootstrap", bootstrap::router())
        .nest("/items", items::router())
        .nest("/locations", locations::router())
        .nest("/suppliers", suppliers::router())
        .nest("/pos", purchase_orders::router())
        .nest("/sos", sales_orders::router())
        .nest("/transfers", transfers::router())
        .nest("/counts", counts::router())
        .nest("/activity", activity::router())
        .nest("/stats", stats::router())
        .nest("/exports", exports::router())
        .nest("/lookup", lookup::router())
        .nest("/admin", admin::router().merge(admin_users::router()))
}

/// Default-deny CORS. Set `RACKLOG_ALLOW_ORIGIN=https://...,https://...`
/// to widen the allowlist for cross-origin clients.
fn build_cors(allow: &AllowOrigin) -> CorsLayer {
    let base = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT]);

    match allow {
        AllowOrigin::SameOrigin => base, // no Access-Control-Allow-Origin → same-origin only
        AllowOrigin::List(origins) => {
            let parsed: Vec<HeaderValue> = origins
                .iter()
                .filter_map(|o| HeaderValue::from_str(o).ok())
                .collect();
            base.allow_origin(parsed).allow_credentials(true)
        }
    }
}

fn set_header(name: HeaderName, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(name, HeaderValue::from_static(value))
}
