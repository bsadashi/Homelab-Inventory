//! End-to-end integration tests for the RACKLOG REST API. Each test boots
//! an isolated SQLite DB in a tempdir, applies migrations, optionally
//! seeds the homelab dataset, and exercises real handlers via tower's
//! `oneshot` — no listening sockets involved.

mod common;

use axum::http::StatusCode;
use common::{expect_ok, expect_status, json, Harness};
use serde_json::json;

// ---- health probes ------------------------------------------------------

#[tokio::test]
async fn healthz_responds_ok() {
    let h = Harness::boot().await;
    let resp = h.get("/api/healthz").await;
    expect_ok(&resp);
    let body = json(resp).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "racklog");
}

#[tokio::test]
async fn readyz_pings_database() {
    let h = Harness::boot().await;
    let resp = h.get("/api/readyz").await;
    expect_ok(&resp);
    let body = json(resp).await;
    assert_eq!(body["status"], "ready");
}

// ---- seed dataset ------------------------------------------------------

#[tokio::test]
async fn seed_loads_canonical_dataset() {
    let h = Harness::boot().await;
    let items = json(h.get("/api/items").await).await;
    let suppliers = json(h.get("/api/suppliers").await).await;
    let locations = json(h.get("/api/locations").await).await;

    let items = items.as_array().expect("items array");
    assert_eq!(items.len(), 36, "seeded item count");
    assert!(items.iter().any(|i| i["sku"] == "NET-USW-PRO-24"));

    assert_eq!(suppliers.as_array().unwrap().len(), 8);
    assert_eq!(locations.as_array().unwrap().len(), 8);
}

#[tokio::test]
async fn seed_skips_when_data_present() {
    // First boot seeds; second boot against the SAME dataset (via a fresh
    // harness) would still seed, so test the no-op path by re-running on
    // an already-populated pool.
    let h = Harness::boot().await;
    let inserted_again = racklog::seed::seed_if_empty(&h.pool).await.unwrap();
    assert!(!inserted_again, "seed must be idempotent on a populated db");
}

#[tokio::test]
async fn empty_database_has_zero_kpis() {
    let h = Harness::boot_with(None, false).await;
    let stats = json(h.get("/api/stats").await).await;
    assert_eq!(stats["totalSKUs"], 0);
    assert_eq!(stats["totalUnits"], 0);
    assert_eq!(stats["openPOs"], 0);
}

// ---- audit log + chain integrity ---------------------------------------

#[tokio::test]
async fn audit_chain_verifies_after_seed() {
    let h = Harness::boot().await;
    let resp = h.get("/api/activity/verify").await;
    expect_ok(&resp);
    let body = json(resp).await;
    assert_eq!(body["valid"], true);
    assert!(body["entries"].as_i64().unwrap() >= 12, "seed activity replayed");
    assert!(body["head"].is_string(), "head hash exposed");
}

#[tokio::test]
async fn audit_chain_grows_on_mutation() {
    let h = Harness::boot().await;
    let before = json(h.get("/api/activity/verify").await).await["entries"]
        .as_i64()
        .unwrap();

    let payload = json!({
        "code": "TEST-RACK",
        "name": "Test Rack",
        "type": "rack",
        "parent": null,
        "bins": ["U1", "U2"]
    });
    let resp = h.post_json("/api/locations", &payload).await;
    expect_status(&resp, StatusCode::CREATED);

    let verify = json(h.get("/api/activity/verify").await).await;
    assert_eq!(verify["valid"], true, "chain still valid after a write");
    assert_eq!(
        verify["entries"].as_i64().unwrap(),
        before + 1,
        "exactly one new audit row"
    );
}

#[tokio::test]
async fn tampering_with_audit_log_breaks_chain() {
    let h = Harness::boot().await;
    // Mutate a description in the middle of the log without recomputing
    // the hash. Verifier must catch this.
    sqlx::query("UPDATE activity_log SET description = 'forged' WHERE id = (SELECT id FROM activity_log ORDER BY id ASC LIMIT 1 OFFSET 3)")
        .execute(&h.pool)
        .await
        .expect("forge");

    let body = json(h.get("/api/activity/verify").await).await;
    assert_eq!(body["valid"], false, "tamper must be detected");
    assert!(body["broken_at"].is_number(), "broken_at points at the bad row");
}

// ---- catalog CRUD ------------------------------------------------------

#[tokio::test]
async fn item_create_then_read_then_delete() {
    let h = Harness::boot().await;

    let create = h
        .post_json(
            "/api/items",
            &json!({
                "id": "I-TEST-001",
                "sku": "TEST-SKU-001",
                "name": "Test Widget",
                "cat": "Tools",
                "supplier": "V1",
                "cost": 10.0,
                "price": 12.5,
                "min": 1,
                "max": 5,
                "qty": 3,
                "loc": [{"l": "L7", "b": "D1", "q": 3}]
            }),
        )
        .await;
    expect_status(&create, StatusCode::CREATED);

    let read = json(h.get("/api/items/I-TEST-001").await).await;
    assert_eq!(read["sku"], "TEST-SKU-001");
    assert_eq!(read["loc"][0]["q"], 3);
    assert_eq!(read["loc"][0]["b"], "D1");

    let del = h.delete("/api/items/I-TEST-001").await;
    expect_status(&del, StatusCode::NO_CONTENT);

    let after = h.get("/api/items/I-TEST-001").await;
    expect_status(&after, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn item_create_rejects_blank_sku() {
    let h = Harness::boot().await;
    let resp = h
        .post_json(
            "/api/items",
            &json!({"sku": "", "name": "x", "cat": "Tools"}),
        )
        .await;
    expect_status(&resp, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn duplicate_sku_returns_conflict() {
    let h = Harness::boot().await;
    let payload = json!({
        "sku": "NET-USW-PRO-24",   // already in the seed
        "name": "Duplicate",
        "cat": "Networking"
    });
    let resp = h.post_json("/api/items", &payload).await;
    expect_status(&resp, StatusCode::CONFLICT);
}

#[tokio::test]
async fn location_create_and_list() {
    let h = Harness::boot().await;
    let resp = h
        .post_json(
            "/api/locations",
            &json!({
                "code": "RACK-Z",
                "name": "Z Rack",
                "type": "rack",
                "bins": ["U1", "U2"]
            }),
        )
        .await;
    expect_status(&resp, StatusCode::CREATED);

    let list = json(h.get("/api/locations").await).await;
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["code"] == "RACK-Z"));
}

// ---- bootstrap snapshot -------------------------------------------------

#[tokio::test]
async fn bootstrap_returns_full_snapshot() {
    let h = Harness::boot().await;
    let body = json(h.get("/api/bootstrap").await).await;
    for key in [
        "locations",
        "suppliers",
        "items",
        "purchaseOrders",
        "salesOrders",
        "transfers",
        "counts",
        "activity",
        "status",
    ] {
        assert!(body.get(key).is_some(), "snapshot missing {key}");
    }
    assert_eq!(body["items"].as_array().unwrap().len(), 36);
    assert_eq!(body["status"]["totalSKUs"], 36);
}

// ---- web shell ---------------------------------------------------------

#[tokio::test]
async fn index_serves_html_with_bootstrap_injection() {
    let h = Harness::boot().await;
    let resp = h.get("/").await;
    expect_ok(&resp);
    let html = common::body_string(resp).await;
    assert!(html.contains("RACKLOG"));
    assert!(
        html.contains("window.__RACKLOG_BOOTSTRAP__"),
        "index must inject the hydration snapshot"
    );
}

#[tokio::test]
async fn embedded_static_assets_are_reachable() {
    let h = Harness::boot().await;
    let resp = h.get("/styles.css").await;
    expect_ok(&resp);
    let body = common::body_string(resp).await;
    assert!(body.len() > 1000, "stylesheet served from embed");
}

#[tokio::test]
async fn path_traversal_is_rejected() {
    let h = Harness::boot().await;
    let resp = h.get("/..%2Fetc%2Fpasswd").await;
    // Either 400 (our explicit guard) or 404 (axum normalises) — both
    // indicate refusal to serve files outside the asset bundle.
    assert!(
        matches!(resp.status(), StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND),
        "got {}",
        resp.status()
    );
}

// ---- security headers --------------------------------------------------

#[tokio::test]
async fn security_headers_are_present() {
    let h = Harness::boot().await;
    let resp = h.get("/api/healthz").await;
    let headers = resp.headers();
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["x-frame-options"], "DENY");
    assert_eq!(headers["referrer-policy"], "no-referrer");
    assert!(headers.contains_key("permissions-policy"));
    assert!(headers.contains_key("strict-transport-security"));
}

// ---- bearer auth -------------------------------------------------------

#[tokio::test]
async fn auth_gate_blocks_unauthenticated_api() {
    let h = Harness::boot_with(Some("s3cr3t".into()), true).await;
    let blocked = h.get_unauthed("/api/items").await;
    expect_status(&blocked, StatusCode::UNAUTHORIZED);

    let allowed = h.get("/api/items").await;
    expect_ok(&allowed);
}

#[tokio::test]
async fn auth_gate_skips_health_probes() {
    let h = Harness::boot_with(Some("s3cr3t".into()), true).await;
    let resp = h.get_unauthed("/api/healthz").await;
    expect_ok(&resp);
}

#[tokio::test]
async fn auth_with_wrong_token_is_rejected() {
    let h = Harness::boot_with(Some("right-token".into()), true).await;
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/api/items")
        .header(axum::http::header::AUTHORIZATION, "Bearer wrong-token")
        .body(axum::body::Body::empty())
        .unwrap();
    use tower::util::ServiceExt;
    let resp = h.router.clone().oneshot(req).await.unwrap();
    expect_status(&resp, StatusCode::UNAUTHORIZED);
}

// ---- CSV exports -------------------------------------------------------

#[tokio::test]
async fn items_csv_export_has_header_and_rows() {
    let h = Harness::boot().await;
    let resp = h.get("/api/exports/items.csv").await;
    expect_ok(&resp);
    assert_eq!(
        resp.headers()[axum::http::header::CONTENT_TYPE],
        "text/csv; charset=utf-8"
    );
    assert!(resp.headers()[axum::http::header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .contains("items.csv"));

    let body = common::body_string(resp).await;
    let lines: Vec<&str> = body.lines().collect();
    assert!(lines[0].starts_with("id,sku,name,category"));
    assert_eq!(lines.len(), 37, "header + 36 seeded items");
    assert!(body.contains("NET-USW-PRO-24"));
}

#[tokio::test]
async fn activity_csv_export_includes_chain_hashes() {
    let h = Harness::boot().await;
    let resp = h.get("/api/exports/activity.csv").await;
    expect_ok(&resp);
    let body = common::body_string(resp).await;
    let lines: Vec<&str> = body.lines().collect();
    assert!(lines[0].starts_with("ts,user,type,ref,description,hash"));
    assert!(lines.len() >= 13, "header + ≥12 seeded activity rows");
}

#[tokio::test]
async fn csv_export_neutralises_formula_injection() {
    // Inject an item whose name starts with '=' — a textbook CSV
    // injection vector. The export must escape it so spreadsheet apps
    // treat it as literal text rather than evaluating a formula.
    let h = Harness::boot().await;
    let payload = serde_json::json!({
        "id": "I-CSVI-001",
        "sku": "CSVI-EVIL",
        "name": "=HYPERLINK(\"http://evil\")",
        "cat": "Tools",
        "qty": 1,
        "loc": []
    });
    let create = h.post_json("/api/items", &payload).await;
    expect_status(&create, axum::http::StatusCode::CREATED);

    let body = common::body_string(h.get("/api/exports/items.csv").await).await;
    // The leading '=' must be neutralised by a single-quote prefix and
    // wrapped in quotes because the value contains a comma / quote.
    assert!(
        body.contains("'=HYPERLINK"),
        "formula prefix must be neutralised: {}",
        body
    );
    assert!(
        !body.lines().any(|l| l.starts_with('=')),
        "no value line may start with a bare '='"
    );
}

// ---- CSV import (SKU bulk management) ----------------------------------

#[tokio::test]
async fn csv_import_creates_new_items() {
    let h = Harness::boot_with(None, false).await; // empty DB
    let csv = "sku,name,category,brand,cost,price,unit,min,max,qty,barcode\n\
               IMP-001,Test A,Tools,Brand1,10,12,ea,1,5,3,123456789012\n\
               IMP-002,Test B,Cables,Brand2,5,5,ea,5,10,7,";
    let resp = h.post_csv("/api/exports/items.csv", csv).await;
    expect_ok(&resp);
    let body = json(resp).await;
    assert_eq!(body["created"], 2);
    assert_eq!(body["updated"], 0);
    assert_eq!(body["errors"].as_array().unwrap().len(), 0);

    // Confirm they're queryable.
    let items = json(h.get("/api/items?q=IMP-").await).await;
    assert_eq!(items.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn csv_import_updates_existing_skus() {
    let h = Harness::boot().await; // seeded
    let csv = "sku,name,category,cost,price,qty\n\
               NET-USW-PRO-24,UniFi Switch (updated),Networking,800,820,2\n";
    let resp = h.post_csv("/api/exports/items.csv", csv).await;
    expect_ok(&resp);
    let body = json(resp).await;
    assert_eq!(body["created"], 0);
    assert_eq!(body["updated"], 1);

    // Find the item and assert the new values stuck.
    let items = json(h.get("/api/items?q=NET-USW-PRO-24").await).await;
    let arr = items.as_array().unwrap();
    let found = arr.iter().find(|i| i["sku"] == "NET-USW-PRO-24").unwrap();
    assert_eq!(found["name"], "UniFi Switch (updated)");
    assert_eq!(found["cost"], 800.0);
    assert_eq!(found["qty"], 2);
}

#[tokio::test]
async fn csv_import_reports_per_row_errors() {
    let h = Harness::boot_with(None, false).await;
    let csv = "sku,name\n\
               GOOD-1,Valid item\n\
               ,blank sku here\n\
               GOOD-2,Another valid\n";
    let body = json(h.post_csv("/api/exports/items.csv", csv).await).await;
    assert_eq!(body["created"], 2, "good rows still committed");
    let errors = body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["line"], 3);
}

#[tokio::test]
async fn csv_import_round_trip_via_export() {
    // Export the seeded items, import them back into a fresh empty DB,
    // and confirm we end up with the same SKU set. Exercises the CSV
    // escape rules (quoted cells, special characters) end-to-end and
    // the importer's tolerance for supplier references that don't yet
    // exist on the destination side.
    let exporter = Harness::boot().await;
    let csv = common::body_string(exporter.get("/api/exports/items.csv").await).await;

    let importer = Harness::boot_with(None, false).await;
    let body = json(importer.post_csv("/api/exports/items.csv", &csv).await).await;
    assert_eq!(body["created"], 36);
    assert_eq!(body["errors"].as_array().unwrap().len(), 0);

    let stats = json(importer.get("/api/stats").await).await;
    assert_eq!(stats["totalSKUs"], 36);
}

#[tokio::test]
async fn csv_import_rejects_missing_required_columns() {
    let h = Harness::boot().await;
    let csv = "name,cost\nNo SKU column,10\n";
    let resp = h.post_csv("/api/exports/items.csv", csv).await;
    expect_status(&resp, StatusCode::BAD_REQUEST);
}

// ---- SKU lookup --------------------------------------------------------

mod sku_stub {
    use racklog::sku_sync::{LookupError, LookupResult, Provider};

    pub struct StubProvider {
        pub name: &'static str,
        pub returns: Vec<LookupResult>,
    }

    #[async_trait::async_trait]
    impl Provider for StubProvider {
        fn name(&self) -> &'static str { self.name }
        async fn lookup(&self, _barcode: &str) -> Result<Vec<LookupResult>, LookupError> {
            Ok(self.returns.clone())
        }
    }
}

#[tokio::test]
async fn lookup_finds_local_match() {
    let h = Harness::boot().await;
    let body = json(h.get("/api/lookup/810010071316").await).await;
    assert_eq!(body["barcode"], "810010071316");
    // I001 is the seeded UniFi switch with that barcode.
    assert_eq!(body["local_item_id"], "I001");
}

#[tokio::test]
async fn lookup_falls_back_to_external_provider() {
    use racklog::sku_sync::LookupResult;
    let mut hit = LookupResult::new("stub", "999999999999");
    hit.name = Some("Mystery Widget".into());
    hit.brand = Some("Stub Co".into());
    let providers: Vec<Box<dyn racklog::sku_sync::Provider>> = vec![Box::new(
        sku_stub::StubProvider {
            name: "stub",
            returns: vec![hit],
        },
    )];
    let h = Harness::boot_with_providers(providers).await;

    let body = json(h.get("/api/lookup/999999999999").await).await;
    assert!(
        body["local_item_id"].is_null(),
        "no local match for unknown barcode"
    );
    let ext = body["external"].as_array().unwrap();
    assert_eq!(ext.len(), 1);
    assert_eq!(ext[0]["source"], "stub");
    assert_eq!(ext[0]["name"], "Mystery Widget");

    let providers_endpoint = json(h.get("/api/lookup/providers").await).await;
    assert_eq!(
        providers_endpoint["providers"].as_array().unwrap()[0],
        "stub"
    );
}

#[tokio::test]
async fn lookup_rejects_invalid_barcodes() {
    let h = Harness::boot().await;
    // Whitespace + special chars are forbidden — the validator runs
    // before any network call so injection attempts can't escape.
    let resp = h.get("/api/lookup/%20bad%20%2F..").await;
    expect_status(&resp, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn lookup_returns_empty_when_no_provider_and_no_local() {
    let h = Harness::boot().await; // no external providers configured
    let body = json(h.get("/api/lookup/000000000000").await).await;
    assert!(body["local_item_id"].is_null());
    assert!(body["external"].as_array().unwrap().is_empty());
    assert!(body["providers_tried"].as_array().unwrap().is_empty());
}

// ---- aggregate stats ---------------------------------------------------

#[tokio::test]
async fn stats_reflect_seed_data() {
    let h = Harness::boot().await;
    let body = json(h.get("/api/stats").await).await;
    assert_eq!(body["totalSKUs"], 36);
    assert_eq!(body["totalUnits"], 161);
    assert_eq!(body["lowStock"], 2);
    assert_eq!(body["outOfStock"], 1);
    assert_eq!(body["openPOs"], 3);
    assert_eq!(body["openSOs"], 3);
    assert_eq!(body["pendingTransfers"], 1);
    assert_eq!(body["scheduledCounts"], 2);
}
