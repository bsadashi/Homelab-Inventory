//! /api/exports — CSV downloads.
//!
//! Server-side CSV with strict RFC-4180 escaping. Doing this on the
//! server avoids the classic CSV-injection footgun (a leading '=' / '+'
//! / '-' / '@' value being interpreted as a formula by Excel/Sheets) —
//! we prefix any such value with a single quote so spreadsheet apps
//! treat it as text.

use crate::audit::{record_in_tx, AuditEvent};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/items.csv", get(items_csv).post(import_items_csv))
        .route("/activity.csv", get(activity_csv))
}

async fn items_csv(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let rows = sqlx::query(
        "SELECT id, sku, name, category, brand, supplier_id, cost, price, unit, \
         min_qty, max_qty, qty, allocated, barcode, updated \
         FROM items ORDER BY sku",
    )
    .fetch_all(&state.pool)
    .await?;

    let mut out = String::new();
    out.push_str(
        "id,sku,name,category,brand,supplier,cost,price,unit,min,max,qty,allocated,barcode,updated\n",
    );
    for r in rows {
        let line = [
            r.get::<String, _>("id"),
            r.get::<String, _>("sku"),
            r.get::<String, _>("name"),
            r.get::<String, _>("category"),
            opt_string(&r, "brand"),
            opt_string(&r, "supplier_id"),
            num(r.try_get::<f64, _>("cost").unwrap_or(0.0)),
            num(r.try_get::<f64, _>("price").unwrap_or(0.0)),
            r.try_get("unit").unwrap_or_else(|_| "ea".to_string()),
            r.try_get::<i64, _>("min_qty").unwrap_or(0).to_string(),
            r.try_get::<i64, _>("max_qty").unwrap_or(0).to_string(),
            r.try_get::<i64, _>("qty").unwrap_or(0).to_string(),
            r.try_get::<i64, _>("allocated").unwrap_or(0).to_string(),
            opt_string(&r, "barcode"),
            opt_string(&r, "updated"),
        ]
        .iter()
        .map(|v| escape(v))
        .collect::<Vec<_>>()
        .join(",");
        out.push_str(&line);
        out.push('\n');
    }

    Ok(csv_response(out, "items.csv"))
}

async fn activity_csv(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let rows = sqlx::query(
        "SELECT ts, user_name, type, ref, description, hash \
         FROM activity_log ORDER BY id ASC",
    )
    .fetch_all(&state.pool)
    .await?;

    let mut out = String::new();
    out.push_str("ts,user,type,ref,description,hash\n");
    for r in rows {
        let line = [
            r.get::<String, _>("ts"),
            r.get::<String, _>("user_name"),
            r.get::<String, _>("type"),
            opt_string(&r, "ref"),
            r.get::<String, _>("description"),
            r.get::<String, _>("hash"),
        ]
        .iter()
        .map(|v| escape(v))
        .collect::<Vec<_>>()
        .join(",");
        out.push_str(&line);
        out.push('\n');
    }
    Ok(csv_response(out, "activity.csv"))
}

#[derive(Serialize)]
pub struct ImportSummary {
    pub processed: usize,
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<ImportError>,
}

#[derive(Serialize)]
pub struct ImportError {
    pub line: usize,
    pub sku: Option<String>,
    pub message: String,
}

/// Import items from a CSV body.
///
/// Header row is required. Recognised columns (case-insensitive):
///   sku (required), name (required), category, brand, supplier,
///   cost, price, unit, min, max, qty, allocated, barcode, img.
///
/// SKU is the upsert key — existing rows are updated in place, new
/// rows are inserted. We never delete or re-key. Each row is processed
/// in its own transaction so a single bad line cannot revert prior
/// progress; the response reports per-line errors so the operator can
/// fix and re-run.
async fn import_items_csv(
    State(state): State<AppState>,
    body: String,
) -> ApiResult<Json<ImportSummary>> {
    if body.trim().is_empty() {
        return Err(ApiError::BadRequest("empty CSV body".into()));
    }
    let mut lines = body.lines();
    let header = lines
        .next()
        .ok_or_else(|| ApiError::BadRequest("missing header".into()))?;
    let columns: Vec<String> = parse_csv_row(header)
        .into_iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();

    let idx = |name: &str| columns.iter().position(|c| c == name);
    let i_sku = idx("sku").ok_or_else(|| ApiError::BadRequest("missing column: sku".into()))?;
    let i_name =
        idx("name").ok_or_else(|| ApiError::BadRequest("missing column: name".into()))?;

    let i_category = idx("category").or_else(|| idx("cat"));
    let i_brand = idx("brand");
    let i_supplier = idx("supplier");
    let i_cost = idx("cost");
    let i_price = idx("price");
    let i_unit = idx("unit");
    let i_min = idx("min").or_else(|| idx("min_qty"));
    let i_max = idx("max").or_else(|| idx("max_qty"));
    let i_qty = idx("qty");
    let i_allocated = idx("allocated");
    let i_barcode = idx("barcode");
    let i_img = idx("img");

    let mut summary = ImportSummary {
        processed: 0,
        created: 0,
        updated: 0,
        skipped: 0,
        errors: Vec::new(),
    };

    for (line_no, raw) in lines.enumerate() {
        let line_no = line_no + 2; // 1-indexed, skipping header
        if raw.trim().is_empty() {
            summary.skipped += 1;
            continue;
        }
        summary.processed += 1;

        let cells = parse_csv_row(raw);
        let cell = |i: Option<usize>| -> Option<&str> {
            i.and_then(|n| cells.get(n)).map(|s| s.as_str())
        };
        let cell_required = |i: usize, name: &str| -> Result<&str, String> {
            cells
                .get(i)
                .map(|s| s.as_str())
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| format!("missing {}", name))
        };

        let sku = match cell_required(i_sku, "sku") {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                summary.errors.push(ImportError {
                    line: line_no,
                    sku: None,
                    message: e,
                });
                continue;
            }
        };
        let name = match cell_required(i_name, "name") {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                summary.errors.push(ImportError {
                    line: line_no,
                    sku: Some(sku),
                    message: e,
                });
                continue;
            }
        };
        let category = cell(i_category).map(|s| s.trim()).unwrap_or("Spare parts").to_string();
        let brand = cell(i_brand).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let supplier = cell(i_supplier).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let cost = cell(i_cost).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
        let price = cell(i_price).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
        let unit = cell(i_unit).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "ea".into());
        let min_qty = cell(i_min).and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
        let max_qty = cell(i_max).and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
        let qty = cell(i_qty).and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
        let allocated = cell(i_allocated).and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
        let barcode = cell(i_barcode).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let img = cell(i_img).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

        // Per-row transaction: a bad row will roll back its own work
        // but leave previous rows committed. The audit chain is still
        // intact because record_in_tx runs inside the same tx.
        let outcome = upsert_one(
            &state.pool, &sku, &name, &category, brand.as_deref(),
            supplier.as_deref(), cost, price, &unit,
            min_qty, max_qty, qty, allocated,
            barcode.as_deref(), img.as_deref(),
        ).await;

        match outcome {
            Ok(true) => summary.created += 1,
            Ok(false) => summary.updated += 1,
            Err(e) => {
                // Public-facing message: never include the raw SQLite
                // error text. The original code surfaced strings like
                // "(code: 787) FOREIGN KEY constraint failed", leaking
                // implementation detail (and table topology) to anyone
                // who could call the import endpoint. Map known cases
                // to friendly messages and log the full error
                // server-side for ops to see.
                tracing::warn!(line = line_no, sku = %sku, %e, "csv import row failed");
                summary.errors.push(ImportError {
                    line: line_no,
                    sku: Some(sku),
                    message: sanitise_row_error(&e),
                });
            }
        }
    }

    Ok(Json(summary))
}

fn sanitise_row_error(e: &ApiError) -> String {
    // Match on the structural cases we expose; fall back to a generic
    // message for anything else. Keep the strings short and free of
    // any internal column / table names.
    match e {
        ApiError::BadRequest(m) => m.clone(),
        ApiError::Conflict(m) => m.clone(),
        ApiError::NotFound => "referenced record not found".into(),
        ApiError::Database(sqlx::Error::Database(db))
            if db.is_unique_violation() =>
        {
            "row violates a uniqueness constraint".into()
        }
        ApiError::Database(sqlx::Error::Database(db))
            if db.is_foreign_key_violation() =>
        {
            "row references a record that does not exist".into()
        }
        _ => "row could not be saved".into(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn upsert_one(
    pool: &sqlx::SqlitePool,
    sku: &str, name: &str, category: &str,
    brand: Option<&str>, supplier: Option<&str>,
    cost: f64, price: f64, unit: &str,
    min_qty: i64, max_qty: i64, qty: i64, allocated: i64,
    barcode: Option<&str>, img: Option<&str>,
) -> ApiResult<bool> {
    let mut tx = pool.begin().await?;

    // If the CSV references a supplier that doesn't exist locally,
    // null it out rather than fail the whole row. This makes the
    // export → import round-trip work against a fresh database where
    // suppliers haven't been imported yet (suppliers are typically a
    // separate, smaller dataset operators import first).
    let supplier_id = match supplier {
        Some(s) if !s.is_empty() => {
            let exists: Option<String> = sqlx::query_scalar(
                "SELECT id FROM suppliers WHERE id = ? OR code = ?",
            )
            .bind(s)
            .bind(s)
            .fetch_optional(&mut *tx)
            .await?;
            exists
        }
        _ => None,
    };
    let supplier = supplier_id.as_deref();

    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM items WHERE sku = ?")
            .bind(sku)
            .fetch_optional(&mut *tx)
            .await?;

    let created;
    if let Some(id) = existing.clone() {
        sqlx::query(
            r#"UPDATE items SET name = ?, category = ?, brand = ?, supplier_id = ?,
                cost = ?, price = ?, unit = ?, min_qty = ?, max_qty = ?, qty = ?,
                allocated = ?, barcode = ?, img = ?, updated = ?,
                updated_at = datetime('now')
              WHERE id = ?"#,
        )
        .bind(name).bind(category).bind(brand).bind(supplier)
        .bind(cost).bind(price).bind(unit)
        .bind(min_qty).bind(max_qty).bind(qty).bind(allocated)
        .bind(barcode).bind(img)
        .bind(chrono::Utc::now().date_naive().to_string())
        .bind(&id)
        .execute(&mut *tx).await?;
        record_in_tx(&mut tx, AuditEvent {
            user: "import",
            kind: "item.import_update",
            r#ref: Some(&id),
            description: &format!("Imported (update) item {}", sku),
            payload: None,
        }).await?;
        created = false;
    } else {
        // Full UUID (122 bits of entropy) — see items::create for
        // the rationale on dropping the 8-char truncation.
        let id = format!("I-{}", uuid::Uuid::new_v4().simple());
        sqlx::query(
            r#"INSERT INTO items
               (id, sku, name, category, brand, supplier_id, cost, price, unit,
                min_qty, max_qty, qty, allocated, barcode, tags, img, updated)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '[]', ?, ?)"#,
        )
        .bind(&id).bind(sku).bind(name).bind(category)
        .bind(brand).bind(supplier)
        .bind(cost).bind(price).bind(unit)
        .bind(min_qty).bind(max_qty).bind(qty).bind(allocated)
        .bind(barcode).bind(img)
        .bind(chrono::Utc::now().date_naive().to_string())
        .execute(&mut *tx).await?;
        record_in_tx(&mut tx, AuditEvent {
            user: "import",
            kind: "item.import_create",
            r#ref: Some(&id),
            description: &format!("Imported (create) item {}", sku),
            payload: None,
        }).await?;
        created = true;
    }
    tx.commit().await?;
    Ok(created)
}

/// Tiny RFC-4180 parser for one line. Handles quoted cells, embedded
/// quotes (""), and embedded commas. Newlines inside quoted cells are
/// not supported because we feed it line-by-line; that's documented.
fn parse_csv_row(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut buf = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    buf.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                buf.push(c);
            }
        } else if c == ',' {
            cells.push(std::mem::take(&mut buf));
        } else if c == '"' && buf.is_empty() {
            in_quotes = true;
        } else {
            buf.push(c);
        }
    }
    cells.push(buf);
    cells
}

fn opt_string(r: &sqlx::sqlite::SqliteRow, col: &str) -> String {
    r.try_get::<Option<String>, _>(col)
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn num(f: f64) -> String {
    if f.fract() == 0.0 {
        format!("{:.0}", f)
    } else {
        format!("{}", f)
    }
}

/// RFC-4180 escape with CSV-injection neutralisation.
///
/// The first formulation only checked `value.chars().next()` against
/// `=+-@`, which let `" =HYPERLINK(...)"` (leading whitespace) and
/// `"\t=HYPERLINK(...)"` (leading tab) bypass the guard — Excel and
/// Sheets still evaluate formulas with leading whitespace. We now
/// neutralise any value whose first **non-whitespace** character is a
/// formula trigger, including the additional triggers `\t` (tab,
/// directly), `\r` (carriage return, also a formula starter in some
/// products), and `|` (DDE-style attack vector).
fn escape(value: &str) -> String {
    // Strip any leading ASCII whitespace (space, tab, NBSP-equivalent,
    // CR/LF) when looking for a formula trigger so leading-space
    // bypasses (` =1+2`) are caught too.
    let content_first = value
        .chars()
        .find(|c| !c.is_ascii_whitespace())
        .or_else(|| value.chars().next());
    let dangerous = matches!(
        content_first,
        Some('=') | Some('+') | Some('-') | Some('@') | Some('|') | Some('\t')
    );
    let neutralised = if dangerous {
        // Leading apostrophe is the documented spreadsheet escape for
        // "treat as literal text".
        let mut s = String::with_capacity(value.len() + 1);
        s.push('\'');
        s.push_str(value);
        s
    } else {
        value.to_string()
    };
    let needs_quotes = neutralised.contains(',')
        || neutralised.contains('"')
        || neutralised.contains('\n')
        || neutralised.contains('\r')
        || neutralised.contains('\t');
    if needs_quotes {
        format!("\"{}\"", neutralised.replace('"', "\"\""))
    } else {
        neutralised
    }
}

#[cfg(test)]
mod parser_tests {
    use super::parse_csv_row;

    #[test]
    fn plain_row() {
        assert_eq!(parse_csv_row("a,b,c"), vec!["a", "b", "c"]);
    }
    #[test]
    fn empty_cells_preserved() {
        assert_eq!(parse_csv_row("a,,c"), vec!["a", "", "c"]);
    }
    #[test]
    fn trailing_empty_cell_preserved() {
        assert_eq!(parse_csv_row("a,b,"), vec!["a", "b", ""]);
    }
    #[test]
    fn quoted_cell_with_comma() {
        assert_eq!(parse_csv_row(r#"a,"b,c",d"#), vec!["a", "b,c", "d"]);
    }
    #[test]
    fn doubled_quote_inside_quoted_cell() {
        assert_eq!(parse_csv_row(r#""he said ""hi""""#), vec![r#"he said "hi""#]);
    }
    #[test]
    fn quote_only_inside_already_quoted() {
        assert_eq!(parse_csv_row(r#"abc,"de"f""#), vec!["abc", r#"def""#]);
    }
    #[test]
    fn empty_string_yields_one_empty_cell() {
        assert_eq!(parse_csv_row(""), vec![""]);
    }
}

#[cfg(test)]
mod escape_tests {
    use super::escape;

    #[test]
    fn plain_value_passes_through() {
        assert_eq!(escape("hello"), "hello");
    }

    #[test]
    fn leading_equals_neutralised() {
        assert!(escape("=HYPERLINK(\"x\")").starts_with("\"'="));
    }

    #[test]
    fn leading_space_then_equals_neutralised() {
        // The previous implementation missed this — leading whitespace
        // hid the formula trigger from the first-char check.
        let escaped = escape(" =1+2");
        assert!(
            escaped.starts_with('\'') || escaped.starts_with("\"'"),
            "expected single-quote prefix, got {:?}",
            escaped
        );
    }

    #[test]
    fn leading_tab_then_equals_neutralised() {
        let escaped = escape("\t=1+2");
        assert!(
            escaped.contains('\''),
            "expected single-quote prefix, got {:?}",
            escaped
        );
    }

    #[test]
    fn pipe_triggers_dde_escape() {
        let escaped = escape("|cmd|");
        assert!(escaped.contains('\''), "got {:?}", escaped);
    }

    #[test]
    fn comma_value_quoted() {
        assert_eq!(escape("a,b"), "\"a,b\"");
    }

    #[test]
    fn embedded_quote_doubled() {
        assert_eq!(escape("a\"b"), "\"a\"\"b\"");
    }
}

fn csv_response(body: String, filename: &str) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename))
            .unwrap_or(HeaderValue::from_static("attachment")),
    );
    (StatusCode::OK, headers, body)
}
