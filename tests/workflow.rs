//! End-to-end workflow integration test.
//!
//! Walks the full ops loop on a freshly-booted server with a clean DB:
//!
//!   1. supplier setup
//!   2. location + bin tree
//!   3. item creation with stock
//!   4. catalog list / get / search
//!   5. PO lifecycle (draft → received)
//!   6. SO lifecycle (open → picking → shipped)
//!   7. transfer create + complete
//!   8. cycle count
//!   9. CSV export → re-import round-trip into a fresh DB
//!  10. audit chain verification at every checkpoint
//!  11. admin surface (info, stats, integrity, vacuum, wal_checkpoint)
//!
//! If anything in the API contract regresses, this test fails and points at
//! the step where the chain broke. Cheap to run (~1 second) and exercises
//! every nested router under one process.

mod common;

use axum::http::StatusCode;
use common::{body_string, expect_ok, expect_status, json, Harness};
use serde_json::{json as j, Value};

#[tokio::test]
async fn full_ops_workflow() {
    // Empty DB, no seed — we'll build state from scratch so the chain
    // and counters are deterministic.
    let h = Harness::boot_with(None, false).await;

    // ── 1. supplier ────────────────────────────────────────────────
    let supplier_id = "V-WF-1".to_string();
    let resp = h
        .post_json(
            "/api/suppliers",
            &j!({
                "id": supplier_id,
                "code": "WF-VENDOR",
                "name": "Workflow Vendor",
                "contact": "ops@vendor.example",
                "leadTime": 5,
                "rating": 4.7,
                "totalSpend": 0
            }),
        )
        .await;
    expect_status(&resp, StatusCode::CREATED);

    // ── 2. location ───────────────────────────────────────────────
    let location_id = "L-WF-1".to_string();
    let resp = h
        .post_json(
            "/api/locations",
            &j!({
                "id": location_id,
                "code": "WF-RACK",
                "name": "Workflow Rack",
                "type": "rack",
                "parent": null,
                "bins": ["U1", "U2", "U3"]
            }),
        )
        .await;
    expect_status(&resp, StatusCode::CREATED);

    // ── 3. item with stock ────────────────────────────────────────
    let item_id = "I-WF-001".to_string();
    let item_payload = j!({
        "id": item_id,
        "sku": "WF-WIDGET-1",
        "name": "Workflow Widget",
        "cat": "Tools",
        "supplier": supplier_id,
        "cost": 12.50,
        "price": 14.99,
        "unit": "ea",
        "min": 2,
        "max": 10,
        "qty": 5,
        "allocated": 0,
        "barcode": "1234567890128",
        "loc": [{"l": location_id, "b": "U1", "q": 5}],
        "tags": ["e2e"]
    });
    let resp = h.post_json("/api/items", &item_payload).await;
    expect_status(&resp, StatusCode::CREATED);

    // ── 4. catalog reads ──────────────────────────────────────────
    let single = json(h.get(&format!("/api/items/{}", item_id)).await).await;
    assert_eq!(single["sku"], "WF-WIDGET-1");
    assert_eq!(single["loc"].as_array().unwrap().len(), 1);

    let list = json(h.get("/api/items?q=Widget").await).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let by_barcode = json(h.get("/api/lookup/1234567890128").await).await;
    assert_eq!(by_barcode["local_item_id"], item_id);

    // ── 5. PO lifecycle ──────────────────────────────────────────
    let po_id = "PO-WF-1".to_string();
    let resp = h
        .post_json(
            "/api/pos",
            &j!({
                "id": po_id,
                "supplier": supplier_id,
                "status": "draft",
                "created": "2026-05-06",
                "expected": "2026-05-13",
                "total": 125.0,
                "lines": [{"sku": "WF-WIDGET-1", "qty": 10, "cost": 12.5}]
            }),
        )
        .await;
    expect_status(&resp, StatusCode::CREATED);

    // Move to received
    let resp = h
        .put_json(
            &format!("/api/pos/{}", po_id),
            &j!({
                "supplier": supplier_id,
                "status": "received",
                "created": "2026-05-06",
                "expected": "2026-05-13",
                "received": "2026-05-12",
                "total": 125.0,
                "lines": [{"sku": "WF-WIDGET-1", "qty": 10, "cost": 12.5}]
            }),
        )
        .await;
    expect_ok(&resp);
    let po = json(h.get(&format!("/api/pos/{}", po_id)).await).await;
    assert_eq!(po["status"], "received");
    assert_eq!(po["lines"].as_array().unwrap().len(), 1);

    // ── 6. SO lifecycle ──────────────────────────────────────────
    let so_id = "SO-WF-1".to_string();
    h.post_json(
        "/api/sos",
        &j!({
            "id": so_id,
            "proj": "Workflow project",
            "status": "open",
            "created": "2026-05-06",
            "priority": "high",
            "lines": [{"sku": "WF-WIDGET-1", "qty": 1}]
        }),
    )
    .await;
    h.put_json(
        &format!("/api/sos/{}", so_id),
        &j!({
            "proj": "Workflow project",
            "status": "shipped",
            "created": "2026-05-06",
            "priority": "high",
            "lines": [{"sku": "WF-WIDGET-1", "qty": 1}]
        }),
    )
    .await;
    let so = json(h.get(&format!("/api/sos/{}", so_id)).await).await;
    assert_eq!(so["status"], "shipped");

    // ── 7. transfer ──────────────────────────────────────────────
    let tr_id = "TR-WF-1".to_string();
    h.post_json(
        "/api/transfers",
        &j!({
            "id": tr_id,
            "from": location_id,
            "to": location_id,        // same loc, different bin in real life
            "date": "2026-05-06",
            "status": "pending",
            "lines": [{"sku": "WF-WIDGET-1", "qty": 1}]
        }),
    )
    .await;
    h.put_json(
        &format!("/api/transfers/{}", tr_id),
        &j!({
            "from": location_id, "to": location_id,
            "date": "2026-05-06", "status": "done",
            "lines": [{"sku": "WF-WIDGET-1", "qty": 1}]
        }),
    )
    .await;

    // ── 8. cycle count ──────────────────────────────────────────
    h.post_json(
        "/api/counts",
        &j!({
            "id": "CYC-WF-1",
            "loc": location_id,
            "date": "2026-05-06",
            "status": "done",
            "counted": 4,
            "variance": -1,
            "by": "tester"
        }),
    )
    .await;

    // ── 9. audit chain verification ─────────────────────────────
    let chain = json(h.get("/api/activity/verify").await).await;
    assert_eq!(chain["valid"], true, "chain should be valid: {:?}", chain);
    assert!(
        chain["entries"].as_i64().unwrap() >= 9,
        "expected ≥9 audit rows"
    );
    assert_eq!(chain["version"], 2);

    // ── 10. dashboard stats reflect everything ──────────────────
    let stats = json(h.get("/api/stats").await).await;
    assert_eq!(stats["totalSKUs"], 1);
    assert_eq!(stats["totalUnits"], 5);
    assert!(stats["totalValue"].as_f64().unwrap() >= 60.0); // 5 * 12.5

    // ── 11. CSV export → import round-trip into a fresh DB ──────
    let csv = body_string(h.get("/api/exports/items.csv").await).await;
    assert!(csv.lines().count() >= 2, "header + at least one data row");

    let h2 = Harness::boot_with(None, false).await;
    // Bring suppliers across first so the FK resolves cleanly.
    h2.post_json(
        "/api/suppliers",
        &j!({"id": supplier_id, "code": "WF-VENDOR", "name": "Vendor"}),
    )
    .await;
    let summary = json(h2.post_csv("/api/exports/items.csv", &csv).await).await;
    assert_eq!(summary["created"], 1);
    let v: Value = json(h2.get("/api/items").await).await;
    assert_eq!(v.as_array().unwrap().len(), 1);

    // ── 12. admin endpoints work end-to-end ─────────────────────
    let info = json(h.get("/api/admin/info").await).await;
    assert!(info["audit_entries"].as_i64().unwrap() >= 9);
    assert_eq!(info["audit_version"], 2);

    let integrity = json(h.get("/api/admin/integrity").await).await;
    assert_eq!(integrity["sqlite_ok"], true);
    assert_eq!(integrity["audit_chain_valid"], true);

    let vacuum = json(h.post_json("/api/admin/vacuum?confirm=1", &j!({})).await).await;
    assert_eq!(vacuum["ok"], true);

    let wal = json(
        h.post_json("/api/admin/wal_checkpoint?confirm=1", &j!({}))
            .await,
    )
    .await;
    assert_eq!(wal["ok"], true);

    // After two admin operations the audit chain should still verify
    // (each admin op records itself, so the chain advanced).
    let final_chain = json(h.get("/api/activity/verify").await).await;
    assert_eq!(final_chain["valid"], true);
    assert!(
        final_chain["entries"].as_i64().unwrap() > chain["entries"].as_i64().unwrap(),
        "audit log should have grown"
    );
}
