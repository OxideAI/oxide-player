use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("mpd error: {0}")]
    Mpd(String),
    #[error("library error: {0}")]
    Library(String),
    #[error("dsp error: {0}")]
    Dsp(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::Mpd(m) => (StatusCode::BAD_GATEWAY, m.clone()),
            AppError::Library(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
            AppError::Dsp(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
