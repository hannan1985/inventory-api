use axum::{
    Json,
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Every error a handler can return. Each variant maps to one HTTP status,
/// so handlers just use `?` and never build responses by hand.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("database error")]
    Database(#[from] sqlx::Error),
}

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Log the real cause on the server, but never leak SQL details to the client.
        if let AppError::Database(e) = &self {
            tracing::error!(error = ?e, "database error");
        }

        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

/// Malformed or wrongly-typed JSON bodies become a 400 with our JSON error shape.
impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        AppError::BadRequest(rejection.body_text())
    }
}

pub type AppResult<T> = Result<T, AppError>;
