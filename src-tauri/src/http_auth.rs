use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::http_config::HttpConfig;

/// Middleware to validate API key from X-API-Key header
pub async fn api_key_auth(
    State(config): State<Arc<HttpConfig>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, ApiKeyError> {
    let headers = request.headers();

    // Extract API key from X-API-Key header
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiKeyError::Missing)?;

    // Validate against configured key
    if api_key != config.api_key {
        return Err(ApiKeyError::Invalid);
    }

    // API key is valid, proceed
    Ok(next.run(request).await)
}

/// Errors that can occur during API key authentication
#[derive(Debug)]
pub enum ApiKeyError {
    Missing,
    Invalid,
}

impl IntoResponse for ApiKeyError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiKeyError::Missing => (StatusCode::UNAUTHORIZED, "Missing API key"),
            ApiKeyError::Invalid => (StatusCode::FORBIDDEN, "Invalid API key"),
        };

        (
            status,
            Json(json!({
                "ok": false,
                "error": message
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_error_response() {
        let error = ApiKeyError::Missing;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let error = ApiKeyError::Invalid;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
