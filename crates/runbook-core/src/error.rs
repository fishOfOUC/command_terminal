use serde::Serialize;
use serde_json::{json, Value};

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Value,
}

impl ApiError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: Value::Null,
        }
    }
    pub fn field(field: &str, message: impl Into<String>) -> Self {
        Self {
            code: "VALIDATION_ERROR".into(),
            message: message.into(),
            details: json!({"field":field}),
        }
    }
    pub fn not_found(kind: &str, id: &str) -> Self {
        Self {
            code: "NOT_FOUND".into(),
            message: format!("{kind} '{id}' was not found"),
            details: json!({"kind":kind,"id":id}),
        }
    }
    pub fn conflict(expected: u64, actual: u64) -> Self {
        Self {
            code: "REVISION_CONFLICT".into(),
            message: "Data changed. Fetch the latest object and retry your patch.".into(),
            details: json!({"expectedRevision":expected,"actualRevision":actual}),
        }
    }
    pub fn exit_code(&self) -> u8 {
        match self.code.as_str() {
            "INVALID_ARGUMENT" | "INVALID_JSON" | "VALIDATION_ERROR" => 2,
            "NOT_FOUND" => 3,
            "REVISION_CONFLICT" | "ALREADY_EXISTS" => 4,
            _ => 1,
        }
    }
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ApiError {}
impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        Self::new("IO_ERROR", e.to_string())
    }
}
impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("DATABASE_ERROR", e.to_string())
    }
}
impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("INVALID_JSON", e.to_string())
    }
}
