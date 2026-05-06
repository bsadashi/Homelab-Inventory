//! Integration tests for the multi-user auth surface.
//!
//! Covers: pre-bootstrap open mode, first-signup-becomes-admin,
//! login + cookie issuance, /me, logout invalidating sessions, the
//! open-signup env knob, and the auth-required gate on a normal API
//! call once at least one user exists.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::{body_string, expect_ok, expect_status, json, Harness};
use serde_json::json as j;
use tower::util::ServiceExt;

async fn raw_post(h: &Harness, path: &str, body: serde_json::Value) -> axum::http::Response<Body> {
    h.router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn raw_get(h: &Harness, path: &str, cookie: Option<&str>) -> axum::http::Response<Body> {
    let mut b = Request::builder().method("GET").uri(path);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    h.router.clone().oneshot(b.body(Body::empty()).unwrap()).await.unwrap()
}

fn extract_session_cookie(resp: &axum::http::Response<Body>) -> Option<String> {
    let v = resp.headers().get(header::SET_COOKIE)?.to_str().ok()?;
    let prefix = "racklog_session=";
    let start = v.find(prefix)? + prefix.len();
    let end = v[start..].find(';').unwrap_or(v.len() - start);
    Some(format!("racklog_session={}", &v[start..start + end]))
}

#[tokio::test]
async fn pre_bootstrap_lets_anyone_in_then_first_signup_becomes_admin() {
    let h = Harness::boot_with(None, false).await;
    // Pre-bootstrap: hitting an authed endpoint without credentials succeeds.
    let resp = raw_get(&h, "/api/items", None).await;
    expect_ok(&resp);

    // First signup → admin.
    let resp = raw_post(&h, "/api/auth/signup", j!({
        "username": "alice",
        "password": "correcthorse"
    })).await;
    expect_status(&resp, StatusCode::CREATED);
    let cookie = extract_session_cookie(&resp).expect("session cookie set");
    let body = json(resp).await;
    assert_eq!(body["user"]["username"], "alice");
    assert_eq!(body["user"]["role"], "admin");

    // /me returns the same identity.
    let me = json(raw_get(&h, "/api/auth/me", Some(&cookie)).await).await;
    assert_eq!(me["user"]["username"], "alice");
    assert_eq!(me["user"]["role"], "admin");
    assert_eq!(me["bootstrap_open"], false);

    // After bootstrap, anonymous access to /api is rejected.
    let resp = raw_get(&h, "/api/items", None).await;
    expect_status(&resp, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn second_signup_requires_open_signup_env() {
    let h = Harness::boot_with(None, false).await;
    let _ = raw_post(&h, "/api/auth/signup", j!({
        "username": "alice",
        "password": "correcthorse"
    })).await;
    // Second signup without RACKLOG_OPEN_SIGNUP → 403.
    let resp = raw_post(&h, "/api/auth/signup", j!({
        "username": "bob",
        "password": "anothergoodone"
    })).await;
    expect_status(&resp, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_logout_round_trip() {
    let h = Harness::boot_with(None, false).await;
    let _ = raw_post(&h, "/api/auth/signup", j!({
        "username": "alice",
        "password": "correcthorse"
    })).await;

    let resp = raw_post(&h, "/api/auth/login", j!({
        "username": "alice",
        "password": "correcthorse"
    })).await;
    expect_ok(&resp);
    let cookie = extract_session_cookie(&resp).expect("session cookie set");

    // /me works with the cookie.
    let me = json(raw_get(&h, "/api/auth/me", Some(&cookie)).await).await;
    assert_eq!(me["user"]["username"], "alice");

    // Logout invalidates the session.
    let logout = h.router.clone().oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/auth/logout")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    expect_status(&logout, StatusCode::NO_CONTENT);

    // The cookie no longer authenticates.
    let after = raw_get(&h, "/api/auth/me", Some(&cookie)).await;
    expect_status(&after, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let h = Harness::boot_with(None, false).await;
    let _ = raw_post(&h, "/api/auth/signup", j!({
        "username": "alice",
        "password": "correcthorse"
    })).await;
    let resp = raw_post(&h, "/api/auth/login", j!({
        "username": "alice",
        "password": "WRONG"
    })).await;
    expect_status(&resp, StatusCode::UNAUTHORIZED);

    // Same status for unknown user (no enumeration leak).
    let resp = raw_post(&h, "/api/auth/login", j!({
        "username": "ghost",
        "password": "irrelevant"
    })).await;
    expect_status(&resp, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signup_validates_username_and_password() {
    let h = Harness::boot_with(None, false).await;
    // Bad username chars.
    let resp = raw_post(&h, "/api/auth/signup", j!({
        "username": "alice and bob",
        "password": "correcthorse"
    })).await;
    expect_status(&resp, StatusCode::BAD_REQUEST);
    // Short password.
    let resp = raw_post(&h, "/api/auth/signup", j!({
        "username": "alice",
        "password": "short"
    })).await;
    expect_status(&resp, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn me_returns_bootstrap_open_when_no_users() {
    let h = Harness::boot_with(None, false).await;
    let body = json(raw_get(&h, "/api/auth/me", None).await).await;
    assert_eq!(body["bootstrap_open"], true);
    assert_eq!(body["user"]["role"], "admin"); // synthetic
    assert_eq!(body["source"], "bootstrap");
}

#[tokio::test]
async fn signup_writes_audit_entry() {
    let h = Harness::boot_with(None, false).await;
    let resp = raw_post(&h, "/api/auth/signup", j!({
        "username": "alice",
        "password": "correcthorse"
    })).await;
    let cookie = extract_session_cookie(&resp).expect("session cookie");
    // Use the cookie to read the (now-protected) activity log.
    let entries = json(raw_get(&h, "/api/activity", Some(&cookie)).await).await;
    assert!(
        entries.as_array().unwrap().iter().any(|e| e["type"] == "auth.signup" && e["user"] == "alice"),
        "expected an auth.signup row by alice; got {:?}",
        entries
    );
    let verify = json(raw_get(&h, "/api/activity/verify", Some(&cookie)).await).await;
    assert_eq!(verify["valid"], true);
}

#[tokio::test]
async fn login_uses_real_username_in_audit() {
    let h = Harness::boot_with(None, false).await;
    let _ = raw_post(&h, "/api/auth/signup", j!({
        "username": "alice",
        "password": "correcthorse"
    })).await;
    let resp = raw_post(&h, "/api/auth/login", j!({
        "username": "alice",
        "password": "correcthorse"
    })).await;
    let cookie = extract_session_cookie(&resp).expect("session cookie");
    let entries = json(raw_get(&h, "/api/activity", Some(&cookie)).await).await;
    assert!(
        entries.as_array().unwrap().iter().any(|e| e["type"] == "auth.login" && e["user"] == "alice"),
        "expected auth.login row by alice"
    );
    let _ = body_string;
}
