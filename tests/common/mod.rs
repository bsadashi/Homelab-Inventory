//! Shared test fixtures. Each call to `boot()` returns a fully-wired
//! Axum router backed by an isolated SQLite file in a tempdir, so tests
//! run in parallel without contention.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, Response, StatusCode};
use axum::Router;
use racklog::{config::Config, db, routes, seed, state::AppState};
use serde_json::Value;
use tempfile::TempDir;
use tower::util::ServiceExt;

use racklog::sku_sync::Provider as SkuProvider;

pub struct Harness {
    pub router: Router,
    pub pool: sqlx::SqlitePool,
    pub token: Option<String>,
    // Held so the tempdir lives as long as the harness.
    _tmp: TempDir,
}

impl Harness {
    pub async fn boot() -> Self {
        Self::boot_with(None, true).await
    }

    pub async fn boot_with(token: Option<String>, seed_on_empty: bool) -> Self {
        Self::boot_full(token, seed_on_empty, Vec::new()).await
    }

    pub async fn boot_with_providers(providers: Vec<Box<dyn SkuProvider>>) -> Self {
        Self::boot_full(None, true, providers).await
    }

    /// Like boot() but with `cfg.open_signup = true` so tests can
    /// register a second non-admin user. Uses a Config flag rather
    /// than an env var because env vars leak across the parallel
    /// test process.
    pub async fn boot_open_signup() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut db_path = tmp.path().to_path_buf();
        db_path.push("racklog-test.db");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());

        let mut cfg = Config::for_test(&url, None);
        cfg.seed_on_empty = false;
        cfg.open_signup = true;

        let pool = db::connect(&url).await.expect("pool");
        db::migrate(&pool).await.expect("migrate");

        let state = AppState::with_providers(pool.clone(), cfg, Vec::new());
        let router = routes::serve(state);
        Self {
            router,
            pool,
            token: None,
            _tmp: tmp,
        }
    }

    async fn boot_full(
        token: Option<String>,
        seed_on_empty: bool,
        providers: Vec<Box<dyn SkuProvider>>,
    ) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut db_path = tmp.path().to_path_buf();
        db_path.push("racklog-test.db");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());

        let mut cfg = Config::for_test(&url, token.clone());
        cfg.seed_on_empty = seed_on_empty;

        let pool = db::connect(&url).await.expect("pool");
        db::migrate(&pool).await.expect("migrate");
        if seed_on_empty {
            seed::seed_if_empty(&pool).await.expect("seed");
        }

        let state = AppState::with_providers(pool.clone(), cfg, providers);
        // Same layer stack the binary uses, so the body-size limit /
        // timeout / compression / tracing layers are exercised in CI.
        let router = routes::serve(state);
        Self {
            router,
            pool,
            token,
            _tmp: tmp,
        }
    }

    pub async fn get(&self, path: &str) -> Response<Body> {
        self.send(Request::builder()
            .method("GET")
            .uri(path)
            .header(header::ACCEPT, "application/json")
            .body(Body::empty())
            .unwrap())
            .await
    }

    pub async fn post_json(&self, path: &str, body: &Value) -> Response<Body> {
        self.send(Request::builder()
            .method("POST")
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap())
            .await
    }

    pub async fn put_json(&self, path: &str, body: &Value) -> Response<Body> {
        self.send(Request::builder()
            .method("PUT")
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap())
            .await
    }

    pub async fn delete(&self, path: &str) -> Response<Body> {
        self.send(Request::builder()
            .method("DELETE")
            .uri(path)
            .body(Body::empty())
            .unwrap())
            .await
    }

    pub async fn post_csv(&self, path: &str, csv: &str) -> Response<Body> {
        self.send(Request::builder()
            .method("POST")
            .uri(path)
            .header(header::CONTENT_TYPE, "text/csv")
            .body(Body::from(csv.to_string()))
            .unwrap())
            .await
    }

    /// Like `get` but without the auth header, even when the harness has
    /// a token configured. Used for verifying the auth gate.
    pub async fn get_unauthed(&self, path: &str) -> Response<Body> {
        // Skip our auth-injecting send() and call the router directly.
        self.router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn send(&self, mut req: Request<Body>) -> Response<Body> {
        if let Some(t) = &self.token {
            req.headers_mut().insert(
                header::AUTHORIZATION,
                format!("Bearer {}", t).parse().unwrap(),
            );
        }
        self.router.clone().oneshot(req).await.unwrap()
    }
}

pub async fn json(resp: Response<Body>) -> Value {
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .expect("body");
    if bytes.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("non-JSON body (status={status}): {e}: {:?}", String::from_utf8_lossy(&bytes)))
}

#[allow(dead_code)]
pub async fn body_string(resp: Response<Body>) -> String {
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .expect("body");
    String::from_utf8(bytes.to_vec()).unwrap()
}

pub fn expect_ok(resp: &Response<Body>) {
    assert!(
        resp.status().is_success(),
        "expected 2xx, got {}",
        resp.status()
    );
}

pub fn expect_status(resp: &Response<Body>, want: StatusCode) {
    assert_eq!(resp.status(), want, "unexpected status");
}
