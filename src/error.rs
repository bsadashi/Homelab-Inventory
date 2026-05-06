//! Centralised API error type. Maps internal failures to RFC-7807-ish JSON
//! payloads and HTTP status codes without leaking implementation details.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("invalid input: {0}")]
    BadRequest(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Database(sqlx::Error::RowNotFound) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            ApiError::NotFound => "not_found",
            ApiError::Unauthorized => "unauthorized",
            ApiError::Forbidden => "forbidden",
            ApiError::BadRequest(_) => "bad_request",
            ApiError::Conflict(_) => "conflict",
            ApiError::Database(sqlx::Error::RowNotFound) => "not_found",
            ApiError::Database(_) => "database_error",
            ApiError::Json(_) => "json_error",
            ApiError::Other(_) => "internal_error",
        }
    }
}

#[derive(Serialize)]
struct ApiErrorBody {
    error: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Server-side log carries the full chain; the client only sees the
        // sanitised public shape.
        if status.is_server_error() {
            tracing::error!(error = %self, code = self.code(), "request failed");
        } else {
            tracing::debug!(error = %self, code = self.code(), "request rejected");
        }

        let public_message = match status {
            StatusCode::INTERNAL_SERVER_ERROR => "internal server error".to_string(),
            _ => self.to_string(),
        };

        let body = ApiErrorBody {
            error: self.code(),
            message: public_message,
        };
        (status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
