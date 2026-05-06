//! /api/lookup — barcode → product info via configured external
//! providers, plus a local fallback that returns the matching item if
//! the barcode is already in our catalog.

use crate::error::{ApiError, ApiResult};
use crate::sku_sync::{lookup_chain, validate_barcode, LookupError, LookupResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/providers", get(providers))
        .route("/:barcode", get(lookup))
}

#[derive(Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<&'static str>,
}

async fn providers(State(state): State<AppState>) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        providers: state.providers.iter().map(|p| p.name()).collect(),
    })
}

#[derive(Serialize)]
pub struct LookupResponse {
    pub barcode: String,
    /// Matching local item id, if any. Lets the UI jump straight to the
    /// existing record instead of offering an import.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_item_id: Option<String>,
    pub external: Vec<LookupResult>,
    pub providers_tried: Vec<&'static str>,
}

async fn lookup(
    State(state): State<AppState>,
    Path(barcode): Path<String>,
) -> ApiResult<Json<LookupResponse>> {
    if validate_barcode(&barcode).is_err() {
        return Err(ApiError::BadRequest(
            "barcode must be 1–32 chars, alphanumeric/_/- only".into(),
        ));
    }

    // Local first — most barcodes will already exist in the catalog.
    let local: Option<String> =
        sqlx::query("SELECT id FROM items WHERE barcode = ? LIMIT 1")
            .bind(&barcode)
            .fetch_optional(&state.pool)
            .await?
            .map(|r| r.get::<String, _>("id"));

    let providers_tried: Vec<&'static str> =
        state.providers.iter().map(|p| p.name()).collect();

    let external = if state.providers.is_empty() {
        Vec::new()
    } else {
        match lookup_chain(&state.providers, &barcode).await {
            Ok(hits) => hits,
            Err(LookupError::NoProvider) => Vec::new(),
            Err(LookupError::InvalidBarcode) => {
                return Err(ApiError::BadRequest("invalid barcode".into()))
            }
            Err(e) => {
                tracing::warn!(%e, %barcode, "external lookup failed");
                Vec::new()
            }
        }
    };

    Ok(Json(LookupResponse {
        barcode,
        local_item_id: local,
        external,
        providers_tried,
    }))
}
