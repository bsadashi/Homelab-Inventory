//! Frontend serving — both the static asset bundle and the templated
//! index that hydrates the dashboard with a single bootstrap snapshot.
//!
//! The dashboard files come from one of two sources:
//!   1. RACKLOG_STATIC_DIR (if set) — useful for hot-reloading during dev.
//!   2. The assets compiled into the binary, so a fresh container needs
//!      no host-side files to serve a fully working UI.

use crate::error::ApiResult;
use crate::routes::bootstrap::Bootstrap;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use std::path::PathBuf;

const INDEX_TEMPLATE: &str = include_str!("../../static/RACKLOG.html");

// Each asset is embedded twice: once as bytes (for serving) and once as
// the path it should answer to. Keeping this list in one place makes it
// trivial to verify which files ship inside the binary.
macro_rules! embed {
    ( $( $path:literal ),* $(,)? ) => {
        &[
            $( ( $path, include_bytes!(concat!("../../static/", $path)) as &[u8] ) ),*
        ]
    };
}

const EMBEDDED_ASSETS: &[(&str, &[u8])] = embed![
    "styles.css",
    "icons.jsx",
    "tweaks-panel.jsx",
    "widgets.jsx",
    "view-dashboard.jsx",
    "view-items.jsx",
    "view-other.jsx",
    "app.jsx",
    "data.jsx",
];

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/RACKLOG.html", get(index))
        .route("/favicon.ico", get(favicon))
        .route("/*path", get(asset))
}

async fn index(State(state): State<AppState>) -> ApiResult<Response> {
    let html = render_index(&state).await?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8")),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        html,
    )
        .into_response())
}

async fn render_index(state: &AppState) -> ApiResult<String> {
    let template = load_template(state).map_err(|e| {
        crate::error::ApiError::Other(anyhow::anyhow!("template load failed: {e}"))
    })?;
    let snapshot = build_bootstrap(&state.pool).await?;
    let payload = serde_json::to_string(&snapshot)?;
    let injection = format!(
        r#"<script>window.__RACKLOG_BOOTSTRAP__ = {};</script>"#,
        payload
    );
    Ok(template.replace("<!-- RACKLOG_BOOTSTRAP -->", &injection))
}

fn load_template(state: &AppState) -> std::io::Result<String> {
    if let Some(dir) = &state.cfg.static_dir {
        let p = dir.join("RACKLOG.html");
        if p.exists() {
            return std::fs::read_to_string(p);
        }
    }
    Ok(INDEX_TEMPLATE.to_string())
}

async fn build_bootstrap(pool: &sqlx::SqlitePool) -> ApiResult<Bootstrap> {
    use crate::routes::bootstrap as bs;
    let resp = bs::snapshot(pool).await?;
    Ok(resp)
}

async fn favicon() -> Response {
    // Tiny transparent .ico — we don't ship one yet, so just answer 204.
    StatusCode::NO_CONTENT.into_response()
}

async fn asset(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    // Block path traversal explicitly — we never need ".." in asset paths.
    if path.contains("..") || path.starts_with('/') {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if let Some(dir) = &state.cfg.static_dir {
        let candidate: PathBuf = dir.join(&path);
        if candidate.starts_with(dir) {
            if let Ok(bytes) = std::fs::read(&candidate) {
                return serve(&path, bytes);
            }
        }
    }

    for (asset_path, bytes) in EMBEDDED_ASSETS {
        if *asset_path == path {
            return serve(asset_path, bytes.to_vec());
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

fn serve(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=300")
        .body(Body::from(bytes))
        .unwrap()
}
