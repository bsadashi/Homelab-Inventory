//! /api/admin/users + /api/admin/api_keys integration tests.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::{
    delete_with, expect_ok, expect_status, get_with, json, post_with, signup_user as signup_admin,
    Harness,
};
use serde_json::json as j;
use tower::util::ServiceExt;

#[tokio::test]
async fn admin_can_create_users_with_specific_roles() {
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;

    let resp = post_with(
        &h,
        "/api/admin/users",
        &admin,
        j!({
            "username": "bob", "password": "anothergoodone", "role": "operator"
        }),
    )
    .await;
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
    let _ = post_with(
        &h,
        "/api/admin/users",
        &admin,
        j!({
            "username": "bob", "password": "anothergoodone", "role": "viewer"
        }),
    )
    .await;
    let bob_id = json(get_with(&h, "/api/admin/users", &admin).await)
        .await
        .as_array()
        .unwrap()
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
    )
    .await;
    expect_ok(&resp);
    let body = json(resp).await;
    assert_eq!(body["role"], "operator");

    // Disable.
    let resp = post_with(
        &h,
        &format!("/api/admin/users/{}/disabled", bob_id),
        &admin,
        j!({"disabled": true}),
    )
    .await;
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
    )
    .await;
    expect_status(&resp, StatusCode::BAD_REQUEST);

    let resp = delete_with(&h, &format!("/api/admin/users/{}", alice_id), &admin).await;
    expect_status(&resp, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_password_reset_invalidates_target_sessions() {
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;

    // Bob signs up via admin, logs in.
    let _ = post_with(
        &h,
        "/api/admin/users",
        &admin,
        j!({
            "username": "bob", "password": "anothergoodone", "role": "viewer"
        }),
    )
    .await;
    let bob_login = h
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&j!({
                        "username": "bob", "password": "anothergoodone"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let v = bob_login
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let prefix = "racklog_session=";
    let start = v.find(prefix).unwrap() + prefix.len();
    let end = v[start..].find(';').unwrap();
    let bob_cookie = format!("racklog_session={}", &v[start..start + end]);

    // Verify bob can use his session.
    expect_ok(&get_with(&h, "/api/auth/me", &bob_cookie).await);

    // Find bob's id.
    let bob_id = json(get_with(&h, "/api/admin/users", &admin).await)
        .await
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "bob")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Admin resets bob's password.
    let resp = post_with(
        &h,
        &format!("/api/admin/users/{}/password", bob_id),
        &admin,
        j!({"password": "abrandnewone"}),
    )
    .await;
    expect_status(&resp, StatusCode::NO_CONTENT);

    // Bob's old session is now invalid.
    let after = get_with(&h, "/api/auth/me", &bob_cookie).await;
    expect_status(&after, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn role_change_revokes_existing_sessions() {
    // Audit regression: a demoted admin previously kept their
    // elevated cookie until the 30-day TTL expired, because
    // update_role didn't revoke sessions like disable / password
    // reset already do. After the fix, role change must invalidate
    // the user's outstanding sessions.
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;
    // Create bob as an operator and grab his session cookie.
    let _ = post_with(
        &h,
        "/api/admin/users",
        &admin,
        j!({"username": "bob", "password": "anothergoodone", "role": "operator"}),
    )
    .await;
    let bob_cookie = common::login_user(&h, "bob", "anothergoodone").await;
    expect_ok(&get_with(&h, "/api/auth/me", &bob_cookie).await);

    let bob_id = json(get_with(&h, "/api/admin/users", &admin).await)
        .await
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "bob")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Admin demotes bob to viewer.
    let resp = post_with(
        &h,
        &format!("/api/admin/users/{}/role", bob_id),
        &admin,
        j!({"role": "viewer"}),
    )
    .await;
    expect_ok(&resp);

    // Bob's old session must be 401 now — no more elevated
    // cookie hanging around for the 30-day TTL.
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

    let resp = post_with(
        &h,
        "/api/admin/api_keys",
        &admin,
        j!({
            "user_id": alice_id, "label": "ci runner"
        }),
    )
    .await;
    expect_status(&resp, StatusCode::CREATED);
    let body = json(resp).await;
    let plaintext = body["plaintext"].as_str().unwrap().to_string();
    let key_id = body["id"].as_str().unwrap().to_string();
    assert!(plaintext.starts_with("rl_"));

    // The key authenticates a request to /api/items.
    let resp = h
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/items")
                .header(header::AUTHORIZATION, format!("Bearer {}", plaintext))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    expect_ok(&resp);

    // Revoke it.
    let resp = delete_with(&h, &format!("/api/admin/api_keys/{}", key_id), &admin).await;
    expect_status(&resp, StatusCode::NO_CONTENT);

    // It no longer authenticates.
    let resp = h
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/items")
                .header(header::AUTHORIZATION, format!("Bearer {}", plaintext))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    expect_status(&resp, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn viewer_scoped_api_key_cannot_mutate_even_for_admin_user() {
    // Per-key scope regression: an admin should be able to issue a
    // read-only key for their own user without losing their own
    // admin powers. The key gets clamped to Viewer regardless of
    // the user's stored role.
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;
    let me = json(get_with(&h, "/api/auth/me", &admin).await).await;
    let alice_id = me["user"]["id"].as_str().unwrap().to_string();

    let resp = post_with(
        &h,
        "/api/admin/api_keys",
        &admin,
        j!({"user_id": alice_id, "label": "kpi scraper", "scope": "viewer"}),
    )
    .await;
    expect_status(&resp, StatusCode::CREATED);
    let body = json(resp).await;
    assert_eq!(body["scope"], "viewer");
    let key = body["plaintext"].as_str().unwrap().to_string();

    // GET works (Viewer is enough).
    let resp = h
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/items")
                .header(header::AUTHORIZATION, format!("Bearer {}", key))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    expect_ok(&resp);

    // POST is forbidden — the key clamped admin alice down to Viewer.
    let resp = h
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/items")
                .header(header::AUTHORIZATION, format!("Bearer {}", key))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sku":"X","name":"X","cat":"X"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    expect_status(&resp, StatusCode::FORBIDDEN);

    // Alice's session still has full admin — the cap is on the key,
    // not on the user. Use no supplier so the empty-DB harness
    // doesn't trip on a missing FK.
    let resp = post_with(
        &h,
        "/api/items",
        &admin,
        j!({"sku":"OK-1","name":"Item","cat":"Tools","cost":1,"price":1,"min":1,"max":2,"qty":1,"loc":[]}),
    )
    .await;
    expect_status(&resp, StatusCode::CREATED);
}

#[tokio::test]
async fn non_admin_cannot_reach_user_admin_endpoints() {
    let h = Harness::boot_open_signup().await;
    let _ = signup_admin(&h, "alice", "correcthorse").await;
    let bob = signup_admin(&h, "bob", "anothergoodone").await; // viewer (second signup, open)

    let resp = get_with(&h, "/api/admin/users", &bob).await;
    expect_status(&resp, StatusCode::FORBIDDEN);

    let resp = post_with(
        &h,
        "/api/admin/users",
        &bob,
        j!({
            "username": "carol", "password": "yetanotherone", "role": "viewer"
        }),
    )
    .await;
    expect_status(&resp, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn non_admin_cannot_reach_admin_observability_endpoints() {
    // Pen-test regression: /api/admin/info, /stats, /integrity,
    // /metrics, /whoami were previously gated only by the global
    // Authed middleware, so a viewer-tier user could read the SQLite
    // file path, table row counts, audit head, and trigger PRAGMA
    // integrity_check (a free DoS handle). All five must return 403
    // for non-admin callers now.
    let h = Harness::boot_open_signup().await;
    let _ = signup_admin(&h, "alice", "correcthorse").await;
    let bob = signup_admin(&h, "bob", "anothergoodone").await;

    for path in [
        "/api/admin/info",
        "/api/admin/stats",
        "/api/admin/integrity",
        "/api/admin/metrics",
        "/api/admin/whoami",
    ] {
        let resp = get_with(&h, path, &bob).await;
        expect_status(&resp, StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn admin_can_reach_admin_observability_endpoints() {
    // Companion to the regression test above: confirm the gate is
    // tight, not over-tight.
    let h = Harness::boot_with(None, false).await;
    let admin = signup_admin(&h, "alice", "correcthorse").await;

    for path in [
        "/api/admin/info",
        "/api/admin/stats",
        "/api/admin/integrity",
        "/api/admin/metrics",
        "/api/admin/whoami",
    ] {
        let resp = get_with(&h, path, &admin).await;
        expect_ok(&resp);
    }
}
