//! /api/admin/users + /api/admin/api_keys integration tests.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::{expect_ok, expect_status, json, Harness};
use serde_json::json as j;
use tower::util::ServiceExt;

async fn signup_admin(h: &Harness, name: &str, pw: &str) -> String {
    let resp = h.router.clone().oneshot(
        Request::builder()
            .method("POST").uri("/api/auth/signup")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&j!({
                "username": name, "password": pw
            })).unwrap())).unwrap(),
    ).await.unwrap();
    let v = resp.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string();
    let prefix = "racklog_session=";
    let start = v.find(prefix).unwrap() + prefix.len();
    let end = v[start..].find(';').unwrap();
    format!("racklog_session={}", &v[start..start + end])
}

async fn post_with(h: &Harness, path: &str, cookie: &str, body: serde_json::Value) -> axum::http::Response<Body> {
    h.router.clone().oneshot(
        Request::builder()
            .method("POST").uri(path)
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap(),
    ).await.unwrap()
}

async fn get_with(h: &Harness, path: &str, cookie: &str) -> axum::http::Response<Body> {
    h.router.clone().oneshot(
        Request::builder()
            .method("GET").uri(path)
            .header(header::COOKIE, cookie)
            .body(Body::empty()).unwrap(),
    ).await.unwrap()
}

async fn delete_with(h: &Harness, path: &str, cookie: &str) -> axum::http::Response<Body> {
    h.router.clone().oneshot(
        Request::builder()
            .method("DELETE").uri(path)
            .header(header::COOKIE, cookie)
            .body(Body::empty()).unwrap(),
    ).await.unwrap()
}

#[tokio::test]
async fn admin_can_create_users_with_specific_roles() {
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;

    let resp = post_with(&h, "/api/admin/users", &admin, j!({
        "username": "bob", "password": "anothergoodone", "role": "operator"
    })).await;
    expect_status(&resp, StatusCode::CREATED);
    let body = json(resp).await;
    assert_eq!(body["username"], "bob");
    assert_eq!(body["role"], "operator");

    let list = json(get_with(&h, "/api/admin/users", &admin).await).await;
    let arr = list.as_array().unwrap();
    let bob = arr.iter().find(|u| u["username"] == "bob").unwrap();
    assert_eq!(bob["role"], "operator");
}

#[tokio::test]
async fn admin_can_change_role_then_disable_then_delete() {
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;
    let _ = post_with(&h, "/api/admin/users", &admin, j!({
        "username": "bob", "password": "anothergoodone", "role": "viewer"
    })).await;
    let bob_id = json(get_with(&h, "/api/admin/users", &admin).await).await
        .as_array().unwrap()
        .iter()
        .find(|u| u["username"] == "bob")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Promote to operator.
    let resp = post_with(
        &h,
        &format!("/api/admin/users/{}/role", bob_id),
        &admin,
        j!({"role": "operator"}),
    ).await;
    expect_ok(&resp);
    let body = json(resp).await;
    assert_eq!(body["role"], "operator");

    // Disable.
    let resp = post_with(
        &h,
        &format!("/api/admin/users/{}/disabled", bob_id),
        &admin,
        j!({"disabled": true}),
    ).await;
    expect_ok(&resp);
    let body = json(resp).await;
    assert_eq!(body["disabled"], true);

    // Delete.
    let resp = delete_with(&h, &format!("/api/admin/users/{}", bob_id), &admin).await;
    expect_status(&resp, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn admin_cannot_change_own_role_or_delete_self() {
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;
    let me = json(get_with(&h, "/api/auth/me", &admin).await).await;
    let alice_id = me["user"]["id"].as_str().unwrap().to_string();

    let resp = post_with(
        &h,
        &format!("/api/admin/users/{}/role", alice_id),
        &admin,
        j!({"role": "viewer"}),
    ).await;
    expect_status(&resp, StatusCode::BAD_REQUEST);

    let resp = delete_with(&h, &format!("/api/admin/users/{}", alice_id), &admin).await;
    expect_status(&resp, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_password_reset_invalidates_target_sessions() {
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;

    // Bob signs up via admin, logs in.
    let _ = post_with(&h, "/api/admin/users", &admin, j!({
        "username": "bob", "password": "anothergoodone", "role": "viewer"
    })).await;
    let bob_login = h.router.clone().oneshot(
        Request::builder()
            .method("POST").uri("/api/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&j!({
                "username": "bob", "password": "anothergoodone"
            })).unwrap())).unwrap(),
    ).await.unwrap();
    let v = bob_login.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string();
    let prefix = "racklog_session=";
    let start = v.find(prefix).unwrap() + prefix.len();
    let end = v[start..].find(';').unwrap();
    let bob_cookie = format!("racklog_session={}", &v[start..start + end]);

    // Verify bob can use his session.
    expect_ok(&get_with(&h, "/api/auth/me", &bob_cookie).await);

    // Find bob's id.
    let bob_id = json(get_with(&h, "/api/admin/users", &admin).await).await
        .as_array().unwrap()
        .iter()
        .find(|u| u["username"] == "bob")
        .unwrap()["id"]
        .as_str().unwrap().to_string();

    // Admin resets bob's password.
    let resp = post_with(
        &h,
        &format!("/api/admin/users/{}/password", bob_id),
        &admin,
        j!({"password": "abrandnewone"}),
    ).await;
    expect_status(&resp, StatusCode::NO_CONTENT);

    // Bob's old session is now invalid.
    let after = get_with(&h, "/api/auth/me", &bob_cookie).await;
    expect_status(&after, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_can_create_revoke_api_key_and_use_it() {
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;

    // Create an api key for alice herself (admin scope).
    let me = json(get_with(&h, "/api/auth/me", &admin).await).await;
    let alice_id = me["user"]["id"].as_str().unwrap().to_string();

    let resp = post_with(&h, "/api/admin/api_keys", &admin, j!({
        "user_id": alice_id, "label": "ci runner"
    })).await;
    expect_status(&resp, StatusCode::CREATED);
    let body = json(resp).await;
    let plaintext = body["plaintext"].as_str().unwrap().to_string();
    let key_id = body["id"].as_str().unwrap().to_string();
    assert!(plaintext.starts_with("rl_"));

    // The key authenticates a request to /api/items.
    let resp = h.router.clone().oneshot(
        Request::builder()
            .method("GET").uri("/api/items")
            .header(header::AUTHORIZATION, format!("Bearer {}", plaintext))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    expect_ok(&resp);

    // Revoke it.
    let resp = delete_with(&h, &format!("/api/admin/api_keys/{}", key_id), &admin).await;
    expect_status(&resp, StatusCode::NO_CONTENT);

    // It no longer authenticates.
    let resp = h.router.clone().oneshot(
        Request::builder()
            .method("GET").uri("/api/items")
            .header(header::AUTHORIZATION, format!("Bearer {}", plaintext))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    expect_status(&resp, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn non_admin_cannot_reach_user_admin_endpoints() {
    let h = Harness::boot_open_signup().await;
    let _ = signup_admin(&h, "alice", "correcthorse").await;
    let bob = signup_admin(&h, "bob", "anothergoodone").await; // viewer (second signup, open)

    let resp = get_with(&h, "/api/admin/users", &bob).await;
    expect_status(&resp, StatusCode::FORBIDDEN);

    let resp = post_with(&h, "/api/admin/users", &bob, j!({
        "username": "carol", "password": "yetanotherone", "role": "viewer"
    })).await;
    expect_status(&resp, StatusCode::FORBIDDEN);
}
