use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub enum AppError { Status(StatusCode, String), Internal(anyhow::Error) }

impl AppError {
    pub fn status(code: StatusCode, msg: impl Into<String>) -> Self { AppError::Status(code, msg.into()) }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self { AppError::Internal(e.into()) }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (code, msg) = match self {
            AppError::Status(c, m) => (c, m),
            AppError::Internal(e) => { tracing::error!(error = %e, "internal error"); (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string()) }
        };
        (code, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}
