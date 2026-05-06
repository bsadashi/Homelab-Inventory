//! /api/exports — CSV downloads.
//!
//! Server-side CSV with strict RFC-4180 escaping. Doing this on the
//! server avoids the classic CSV-injection footgun (a leading '=' / '+'
//! / '-' / '@' value being interpreted as a formula by Excel/Sheets) —
//! we prefix any such value with a single quote so spreadsheet apps
//! treat it as text.

use crate::error::ApiResult;
use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/items.csv", get(items_csv))
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
fn escape(value: &str) -> String {
    let needs_quotes = value.contains(',')
        || value.contains('"')
        || value.contains('\n')
        || value.contains('\r');
    let neutralised = match value.chars().next() {
        // Block formula-prefix injection. The leading apostrophe is the
        // documented spreadsheet escape for "treat as literal text".
        Some('=') | Some('+') | Some('-') | Some('@') => {
            let mut s = String::with_capacity(value.len() + 1);
            s.push('\'');
            s.push_str(value);
            s
        }
        _ => value.to_string(),
    };
    if needs_quotes {
        format!("\"{}\"", neutralised.replace('"', "\"\""))
    } else {
        neutralised
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
