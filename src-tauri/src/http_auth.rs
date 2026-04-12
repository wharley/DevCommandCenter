use axum::{
    body::Body,
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::http_config::{HttpAuthMode, HttpConfig};

/// Middleware to validate HTTP auth according to the configured mode.
pub async fn auth_middleware(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, AuthError> {
    let headers = request.headers().clone();
    let config = config.read().await;
    validate_auth(&config, &headers)?;
    Ok(next.run(request).await)
}

/// Validates either API key or bearer token, depending on the active auth mode.
pub fn validate_auth(config: &HttpConfig, headers: &HeaderMap) -> Result<(), AuthError> {
    match config.effective_auth_mode() {
        HttpAuthMode::Local => validate_api_key(config, headers),
        HttpAuthMode::Remote => validate_bearer_token(config, headers),
        HttpAuthMode::Mixed => {
            if validate_api_key(config, headers).is_ok() {
                return Ok(());
            }
            validate_bearer_token(config, headers)
        }
    }
}

fn validate_api_key(config: &HttpConfig, headers: &HeaderMap) -> Result<(), AuthError> {
    let api_key = headers
        .get("X-API-Key")
        .and_then(|value| value.to_str().ok())
        .ok_or(AuthError::MissingApiKey)?;

    if api_key != config.api_key {
        return Err(AuthError::InvalidApiKey);
    }

    Ok(())
}

fn validate_bearer_token(config: &HttpConfig, headers: &HeaderMap) -> Result<(), AuthError> {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(AuthError::MissingBearerToken)?;

    let token = authorization
        .strip_prefix("Bearer ")
        .or_else(|| authorization.strip_prefix("bearer "))
        .ok_or(AuthError::InvalidBearerScheme)?;

    if config
        .bearer_token
        .as_deref()
        .filter(|configured| !configured.is_empty() && token == *configured)
        .is_some()
        && config.bearer_token_is_active()
    {
        return Ok(());
    }

    if config
        .bearer_token_previous
        .as_deref()
        .filter(|configured| !configured.is_empty() && token == *configured)
        .is_some()
        && config.bearer_token_previous_is_active()
    {
        return Ok(());
    }

    Err(AuthError::InvalidBearerToken)
}

/// Errors that can occur during HTTP auth
#[derive(Debug)]
pub enum AuthError {
    MissingApiKey,
    InvalidApiKey,
    MissingBearerToken,
    InvalidBearerScheme,
    InvalidBearerToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message, scheme) = match self {
            AuthError::MissingApiKey => {
                (StatusCode::UNAUTHORIZED, "Missing API key", Some("ApiKey"))
            }
            AuthError::InvalidApiKey => (StatusCode::FORBIDDEN, "Invalid API key", Some("ApiKey")),
            AuthError::MissingBearerToken => (
                StatusCode::UNAUTHORIZED,
                "Missing bearer token",
                Some("Bearer"),
            ),
            AuthError::InvalidBearerScheme => (
                StatusCode::UNAUTHORIZED,
                "Invalid bearer token scheme",
                Some("Bearer"),
            ),
            AuthError::InvalidBearerToken => (
                StatusCode::FORBIDDEN,
                "Invalid or expired bearer token",
                Some("Bearer"),
            ),
        };

        let mut response = (
            status,
            Json(json!({
                "ok": false,
                "error": message
            })),
        )
            .into_response();

        if let Some(scheme) = scheme {
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_str(scheme)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("Bearer")),
            );
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_config::{HttpAuthMode, HttpConfig};
    use axum::http::HeaderValue;
    use chrono::{Duration as ChronoDuration, Utc};
    use std::path::PathBuf;

    fn config_with_mode(mode: HttpAuthMode) -> HttpConfig {
        HttpConfig {
            auth_mode: Some(mode),
            api_key: "test-key".to_string(),
            bearer_token: Some("bearer-current".to_string()),
            bearer_token_previous: Some("bearer-previous".to_string()),
            bearer_token_expires_at: Some(Utc::now() + ChronoDuration::seconds(60)),
            bearer_token_previous_expires_at: Some(Utc::now() + ChronoDuration::seconds(60)),
            db_path: PathBuf::from("dcc.db"),
            ..HttpConfig::default()
        }
    }

    #[test]
    fn test_local_mode_accepts_api_key() {
        let config = config_with_mode(HttpAuthMode::Local);
        let mut headers = HeaderMap::new();
        headers.insert("X-API-Key", HeaderValue::from_static("test-key"));

        assert!(validate_auth(&config, &headers).is_ok());
    }

    #[test]
    fn test_remote_mode_accepts_bearer_token() {
        let config = config_with_mode(HttpAuthMode::Remote);
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer bearer-current"),
        );

        assert!(validate_auth(&config, &headers).is_ok());
    }

    #[test]
    fn test_mixed_mode_accepts_either_scheme() {
        let config = config_with_mode(HttpAuthMode::Mixed);

        let mut api_key_headers = HeaderMap::new();
        api_key_headers.insert("X-API-Key", HeaderValue::from_static("test-key"));
        assert!(validate_auth(&config, &api_key_headers).is_ok());

        let mut bearer_headers = HeaderMap::new();
        bearer_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer bearer-current"),
        );
        assert!(validate_auth(&config, &bearer_headers).is_ok());
    }
}
