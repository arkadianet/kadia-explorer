//! The API's single error type and its RFC 7807 "problem details" rendering.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use xp_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn title(&self) -> &'static str {
        match self {
            ApiError::NotFound => "Not Found",
            ApiError::BadRequest(_) => "Bad Request",
            ApiError::Internal(_) => "Internal Server Error",
        }
    }

    fn detail(&self) -> String {
        match self {
            ApiError::NotFound => "the requested resource does not exist".to_owned(),
            ApiError::BadRequest(d) => d.clone(),
            // Never leak the internal cause to the client; it is logged instead.
            ApiError::Internal(_) => "internal error".to_owned(),
        }
    }
}

/// RFC 7807 problem details. `type` is always `about:blank`, which per the RFC means the
/// status code itself is the whole problem type.
#[derive(Serialize)]
struct Problem {
    r#type: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        if let ApiError::Internal(msg) = &self {
            tracing::error!(error = %msg, "api internal error");
        }
        let body = Problem {
            r#type: "about:blank",
            title: self.title(),
            status: status.as_u16(),
            detail: self.detail(),
        };
        (status, Json(body)).into_response()
    }
}

/// Every store failure is a server-side fault: a caller cannot provoke one with a
/// well-formed request, so it maps to 500 (and is logged when rendered).
impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> ApiError {
        ApiError::Internal(format!("store: {e}"))
    }
}
