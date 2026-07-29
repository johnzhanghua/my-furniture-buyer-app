use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::Serialize;
use thiserror::Error;

/// Every failure the API can surface. `Display` is the human-readable message
/// sent to the client, except for `Internal`/`Database`, which are deliberately
/// opaque and logged server-side instead.
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("incorrect email or password")]
    InvalidCredentials,

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    /// A dependency failed, not us. Kept distinct from `Internal` so an
    /// upstream outage is diagnosable from the status code alone.
    #[error("{0}")]
    Upstream(String),

    /// Upstream `402`: the order costs more than the balance. Carries
    /// upstream's own wording for the log and for detail; the frontend renders
    /// its own sentence off the `insufficient_balance` code.
    #[error("{0}")]
    InsufficientBalance(String),

    #[error("something went wrong")]
    Database(#[from] sqlx::Error),

    #[error("something went wrong")]
    Internal(String),
}

impl ApiError {
    /// Stable machine-readable code; the frontend switches on this, not on the
    /// message text.
    fn code(&self) -> &'static str {
        match self {
            ApiError::BadRequest(_) => "bad_request",
            ApiError::Unauthorized(_) => "unauthorized",
            ApiError::InvalidCredentials => "invalid_credentials",
            ApiError::NotFound(_) => "not_found",
            ApiError::Conflict(_) => "conflict",
            ApiError::Upstream(_) => "upstream_error",
            ApiError::InsufficientBalance(_) => "insufficient_balance",
            ApiError::Database(_) | ApiError::Internal(_) => "internal",
        }
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    message: String,
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized(_) | ApiError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            // 422: the request was well-formed but violates a business rule.
            ApiError::InsufficientBalance(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::Upstream(_) => StatusCode::BAD_GATEWAY,
            ApiError::Database(_) | ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        // Log the real cause; the client only ever sees the opaque message.
        match self {
            ApiError::Database(e) => log::error!("database error: {e:?}"),
            ApiError::Internal(e) => log::error!("internal error: {e}"),
            _ => {}
        }

        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: self.code(),
            message: self.to_string(),
        })
    }
}
