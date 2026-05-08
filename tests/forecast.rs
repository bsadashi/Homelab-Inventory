//! /api/forecast/reorder integration tests.

mod common;

use axum::http::StatusCode;
use common::{expect_status, json, raw_get, raw_post, signup_user, Harness};
use serde_json::json as j;
use tower::util::ServiceExt;

#[tokio::test]
async fn reorder_report_lists_items_below_min() {
    // Seeded dataset has known low-stock items: I005 (NET-GBIC-LR
    // qty=1 min=2), I033 (CBL-HDMI-21 qty=3 min=4), I034 (CBL-DAC-3M
    // qty=2 min=2 — at min counts), I073 (CON-ISOPRO-1 qty=1 min=1
    // — at min). Plus I013 (SVR-NUC-I5 qty=0 min=1 — out of stock).
    let h = Harness::boot().await;
    let body = json(raw_get(&h, "/api/forecast/reorder", None).await).await;
    let suggestions = body["suggestions"].as_array().unwrap();
    assert!(!suggestions.is_empty(), "expected at least one suggestion");

    // The out-of-stock NUC must show up as urgent.
    let nuc = suggestions
        .iter()
        .find(|s| s["sku"] == "SVR-NUC-I5")
        .expect("expected SVR-NUC-I5 in reorder list");
    assert_eq!(nuc["urgency"], "urgent");
    assert_eq!(nuc["qty_on_hand"], 0);
    assert!(nuc["reorder_qty"].as_i64().unwrap() >= 1);

    // Drafts are grouped by supplier.
    let drafts = body["draft_pos"].as_array().unwrap();
    assert!(!drafts.is_empty());
    for d in drafts {
        let lines = d["lines"].as_array().unwrap();
        assert!(!lines.is_empty(), "every draft has lines");
        // total ~= sum(line.qty * line.cost)
        let computed: f64 = lines
            .iter()
            .map(|l| l["qty"].as_i64().unwrap() as f64 * l["cost"].as_f64().unwrap())
            .sum();
        let stated = d["total"].as_f64().unwrap();
        assert!((computed - stated).abs() < 0.01);
    }
}

#[tokio::test]
async fn reorder_apply_requires_admin_and_confirm() {
    let h = Harness::boot().await;
    // No confirm → 400
    let resp = raw_post(&h, "/api/forecast/reorder?lookback=30", j!({})).await;
    expect_status(&resp, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reorder_apply_creates_draft_pos_under_admin() {
    let h = Harness::boot().await;
    // The seeded dataset has zero users; bootstrap-admin can call
    // anything. Create a real admin to mirror production semantics.
    let admin = signup_user(&h, "alice", "correcthorse").await;

    // Fetch the report first so we can read the idempotency token
    // and echo it on the apply call.
    let report = json(raw_get(&h, "/api/forecast/reorder", Some(&admin)).await).await;
    let token = report["token"].as_str().unwrap().to_string();

    // Apply.
    let resp = h
        .router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/api/forecast/reorder?confirm=1&token={token}"))
                .header(axum::http::header::COOKIE, &admin)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    expect_status(&resp, StatusCode::CREATED);
    let body = json(resp).await;
    let pos = body["created_pos"].as_array().unwrap();
    assert!(!pos.is_empty(), "should have created at least one PO");

    // Replay the same token — must be rejected with 429 to stop
    // double-clicks materialising a second batch of PO-AUTO rows.
    let resp = h
        .router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/api/forecast/reorder?confirm=1&token={token}"))
                .header(axum::http::header::COOKIE, &admin)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    expect_status(&resp, StatusCode::TOO_MANY_REQUESTS);

    // Apply without a token at all — must be rejected with 400.
    let resp = h
        .router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/forecast/reorder?confirm=1")
                .header(axum::http::header::COOKIE, &admin)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    expect_status(&resp, StatusCode::BAD_REQUEST);

    // The new POs are now in the catalog with status=draft.
    let list = json(raw_get(&h, "/api/pos", Some(&admin)).await).await;
    let auto = list
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["id"].as_str().unwrap_or("").starts_with("PO-AUTO-"))
        .count();
    assert!(auto >= 1);

    // Audit chain still verifies.
    let v = json(raw_get(&h, "/api/activity/verify", Some(&admin)).await).await;
    assert_eq!(v["valid"], true);
}
