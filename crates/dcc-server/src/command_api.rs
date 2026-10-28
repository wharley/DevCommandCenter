//! The shape the desktop's commands answer in (`ApiError` with a code), and
//! the ids and timestamps they stamp; shared by the terminals and the desktop.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn db_error(message: impl Into<String>) -> ApiError {
    ApiError {
        code: "DB_ERROR",
        message: message.into(),
    }
}

pub fn next_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("dcc-{now:x}")
}

pub fn iso_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}
