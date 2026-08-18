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
    #[error("unprocessable: {0}")]
    Unprocessable(String),
    #[error("bluetooth: {0}")]
    Bluetooth(String),
    /// The host has no usable ALSA USB playback scanner.
    #[error("audio unavailable: {0}")]
    AudioUnavailable(String),
    /// The Bluetooth subsystem is unavailable (no adapter, BlueZ not running,
    /// or the platform does not support Bluetooth).
    #[error("bluetooth unavailable")]
    BluetoothUnavailable,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::Mpd(m) => (StatusCode::BAD_GATEWAY, m.clone()),
            AppError::Library(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
            AppError::Dsp(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::Unprocessable(m) => (StatusCode::UNPROCESSABLE_ENTITY, m.clone()),
            AppError::Bluetooth(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::AudioUnavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m.clone()),
            AppError::BluetoothUnavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, "Bluetooth is not available on this platform or no adapter was found".to_string())
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
