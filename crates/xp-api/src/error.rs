//! The API's single error type and its RFC 7807 "problem details" rendering.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use xp_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{detail}")]
    History {
        status: StatusCode,
        code: &'static str,
        detail: String,
    },
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Internal(String),
    #[error("rate limited")]
    TooManyRequests { retry_after: u32 },
    #[error("overloaded")]
    Overloaded,
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::History { status, .. } => *status,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Overloaded => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn title(&self) -> &'static str {
        match self {
            ApiError::History { status, .. } => status.canonical_reason().unwrap_or("Error"),
            ApiError::NotFound => "Not Found",
            ApiError::BadRequest(_) => "Bad Request",
            ApiError::Internal(_) => "Internal Server Error",
            ApiError::TooManyRequests { .. } => "Too Many Requests",
            ApiError::Overloaded => "Service Unavailable",
        }
    }

    fn detail(&self) -> String {
        match self {
            ApiError::History { detail, .. } => detail.clone(),
            ApiError::NotFound => "the requested resource does not exist".to_owned(),
            ApiError::BadRequest(d) => d.clone(),
            // Never leak the internal cause to the client; it is logged instead.
            ApiError::Internal(_) => "internal error".to_owned(),
            ApiError::TooManyRequests { retry_after } => {
                format!("rate limit exceeded; retry after {retry_after} s")
            }
            ApiError::Overloaded => "too many concurrent reads; retry shortly".to_owned(),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        if let ApiError::Internal(msg) = &self {
            tracing::error!(error = %msg, "api internal error");
        }
        // Back-pressure answers carry `Retry-After` in whole seconds (minimum 1); they are
        // routine load shedding, so they are never logged.
        let retry_after = match &self {
            ApiError::TooManyRequests { retry_after } => Some(*retry_after),
            ApiError::Overloaded => Some(1),
            ApiError::History {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            } => Some(1),
            _ => None,
        };
        let body = Problem {
            r#type: "about:blank",
            title: self.title(),
            status: status.as_u16(),
            detail: self.detail(),
            code: match &self {
                ApiError::History { code, .. } => Some(*code),
                _ => None,
            },
        };
        let mut resp = (status, Json(body)).into_response();
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        if let Some(s) = retry_after {
            resp.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(s));
        }
        resp
    }
}

/// Every store failure is a server-side fault: a caller cannot provoke one with a
/// well-formed request, so it maps to 500 (and is logged when rendered).
impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> ApiError {
        ApiError::Internal(format!("store: {e}"))
    }
}
