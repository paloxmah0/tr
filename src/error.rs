use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("llm error: {0}")]
    Llm(String),
    #[error("market provider error: {0}")]
    Market(String),
    #[error("execution error: {0}")]
    Execution(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            AppError::Db(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::Llm(m) => (StatusCode::BAD_GATEWAY, m.clone()),
            AppError::Market(m) => (StatusCode::BAD_GATEWAY, m.clone()),
            AppError::Execution(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
            AppError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::Serde(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        tracing::error!(error = %self, "request failed");
        (status, Json(json!({ "error": msg }))).into_response()
    }
}
