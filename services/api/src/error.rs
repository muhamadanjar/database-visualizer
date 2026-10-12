use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use db_types::DbError;
use serde_json::json;

pub type Result<T> = std::result::Result<T, ApiError>;
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: &'static str,
}
impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }
    pub fn invalid() -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "INVALID_REQUEST",
            "Request is invalid",
        )
    }
    pub fn denied() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "PERMISSION_DENIED",
            "Permission denied",
        )
    }
    pub fn missing() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "RESOURCE_NOT_FOUND",
            "Resource not found",
        )
    }
    pub fn conflict() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "REVISION_CONFLICT",
            "Resource changed; refresh before retrying",
        )
    }
    pub fn unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            "Service unavailable",
        )
    }
    pub fn auth() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "AUTH_REQUIRED", "Sign in again")
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref error) = error {
            if error.is_unique_violation() {
                return Self::conflict();
            }
            if error.is_foreign_key_violation() || error.is_check_violation() {
                return Self::invalid();
            }
        }
        Self::unavailable()
    }
}
impl From<DbError> for ApiError {
    fn from(error: DbError) -> Self {
        match error.code.as_str() {
            "EDIT_CAPACITY_EXCEEDED"
            | "SESSION_BUSY"
            | "SESSION_CAPACITY_EXCEEDED"
            | "RESULT_CAPACITY_EXCEEDED" => Self::new(
                StatusCode::CONFLICT,
                "CAPACITY_EXCEEDED",
                "Session or result capacity reached",
            ),
            "SESSION_NOT_FOUND" | "EXECUTION_NOT_FOUND" | "RESULT_NOT_FOUND" => Self::missing(),
            "EDIT_READBACK_UNAVAILABLE"
            | "EDIT_ALREADY_APPLIED"
            | "EDIT_SNAPSHOT_EXPIRED"
            | "SEARCH_CURSOR_EXPIRED" => Self::conflict(),
            "CLOSE_CONFIRMATION_REQUIRED" | "TRANSACTION_RESOLUTION_REQUIRED" => Self::new(
                StatusCode::CONFLICT,
                "TRANSACTION_RESOLUTION_REQUIRED",
                "Cancel or resolve transaction, or explicitly disconnect",
            ),
            "INVALID_EDIT"
            | "INVALID_SEARCH"
            | "INVALID_VALUE"
            | "TABLE_READ_ONLY"
            | "INVALID_SQL"
            | "INVALID_PROFILE"
            | "INVALID_REQUEST"
            | "UNSUPPORTED_STATEMENT" => Self::invalid(),
            _ => Self::new(
                StatusCode::BAD_GATEWAY,
                "DATABASE_OPERATION_FAILED",
                "Database operation failed; check connection and permissions",
            ),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status,Json(json!({"message":self.message,"details":[{"code":self.code,"message":self.message}]}))).into_response()
    }
}
pub fn data(value: impl serde::Serialize) -> Json<serde_json::Value> {
    Json(json!({"message":"Operation completed","data":value,"meta":{}}))
}
