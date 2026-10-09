use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header::AUTHORIZATION, Extensions, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::http_config::{HttpAuthMode, HttpConfig};

/// The paired device a request comes from. The relay connector
/// (`remote_connector`) sets it on requests it decrypted from a phone's Noise
/// channel. Request extensions never come from the network, so its presence
/// on arrival means an in-process caller already authenticated the device.
#[derive(Clone, Debug)]
pub struct PairedDeviceIdentity(pub String);

/// Middleware to validate HTTP auth.
///
/// Accepted credentials:
///   - a `PairedDeviceIdentity` extension — set in process by the relay
///     connector, never by the network
///   - X-API-Key — desktop / Local mode
///   - Authorization: Bearer <config-token> — the operator-configured shared
///     bearer of a headless `dccd-http` (Remote / Mixed modes)
pub async fn auth_middleware(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, AuthError> {
    if request.extensions().get::<PairedDeviceIdentity>().is_some() {
        return Ok(next.run(request).await);
    }

    {
        let config = config.read().await;
        validate_auth(&config, request.headers())?;
    }
    Ok(next.run(request).await)
}

/// Address of the client, for the pairing audit trail.
///
/// The socket peer is the source of truth. `X-Forwarded-For` is only honoured
/// when the peer is loopback, i.e. a proxy on this machine (Tailscale serve,
/// cloudflared) forwarded the request; from anywhere else it is spoofable.
/// `None` when the server was not started with connect info (tests).
pub fn client_ip(headers: &HeaderMap, extensions: &Extensions) -> Option<String> {
    let peer = extensions.get::<ConnectInfo<SocketAddr>>()?.0.ip();
    if peer.is_loopback() {
        if let Some(forwarded) = header_str(headers, "x-forwarded-for")
            .and_then(|v| v.split(',').next().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
        {
            return Some(forwarded);
        }
    }
    Some(peer.to_string())
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
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

    #[test]
    fn network_exposed_server_rejects_api_key() {
        let config = HttpConfig {
            host: "0.0.0.0".to_string(),
            ..config_with_mode(HttpAuthMode::Local)
        };
        let mut headers = HeaderMap::new();
        headers.insert("X-API-Key", HeaderValue::from_static("test-key"));

        assert!(validate_auth(&config, &headers).is_err());
    }

    fn extensions_from(peer: &str) -> Extensions {
        let mut extensions = Extensions::new();
        extensions.insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        extensions
    }

    #[test]
    fn client_ip_ignores_forwarded_for_from_the_network() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("10.9.9.9"));

        assert_eq!(
            client_ip(&headers, &extensions_from("192.168.1.50:51000")).as_deref(),
            Some("192.168.1.50")
        );
    }

    #[test]
    fn client_ip_trusts_forwarded_for_from_a_local_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("100.64.0.7, 127.0.0.1"),
        );

        assert_eq!(
            client_ip(&headers, &extensions_from("127.0.0.1:51000")).as_deref(),
            Some("100.64.0.7")
        );
        assert_eq!(
            client_ip(&HeaderMap::new(), &extensions_from("127.0.0.1:51000")).as_deref(),
            Some("127.0.0.1")
        );
    }

    #[test]
    fn client_ip_is_unknown_without_connect_info() {
        assert_eq!(client_ip(&HeaderMap::new(), &Extensions::new()), None);
    }

    // ----- middleware (end-to-end through an axum router) -----

    use axum::{
        http::{Request as AxumRequest, StatusCode as AxumStatus},
        middleware as axum_middleware,
        routing::get,
        Router,
    };
    use tower::util::ServiceExt;

    fn make_router(config: HttpConfig) -> Router {
        let config = Arc::new(RwLock::new(config));
        Router::new()
            .route("/protected", get(|| async { "ok" }))
            .route_layer(axum_middleware::from_fn_with_state(
                config.clone(),
                auth_middleware,
            ))
            .with_state(config)
    }

    async fn status_of(app: &Router, request: AxumRequest<Body>) -> AxumStatus {
        app.clone().oneshot(request).await.unwrap().status()
    }

    #[tokio::test]
    async fn relay_identity_passes_without_other_credentials() {
        let app = make_router(config_with_mode(HttpAuthMode::Local));
        let mut request = AxumRequest::builder()
            .uri("/protected")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(PairedDeviceIdentity("device".to_string()));

        assert_eq!(status_of(&app, request).await, AxumStatus::OK);
    }

    #[tokio::test]
    async fn local_mode_needs_the_api_key() {
        let app = make_router(config_with_mode(HttpAuthMode::Local));
        let with_key = AxumRequest::builder()
            .uri("/protected")
            .header("X-API-Key", "test-key")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(&app, with_key).await, AxumStatus::OK);

        let without = AxumRequest::builder()
            .uri("/protected")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(&app, without).await, AxumStatus::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn device_headers_are_not_a_credential() {
        // The retired signed-request scheme must not authorize anything now.
        let app = make_router(config_with_mode(HttpAuthMode::Mixed));
        let request = AxumRequest::builder()
            .uri("/protected")
            .header("X-Device-Id", "device")
            .header("X-Timestamp", chrono::Utc::now().to_rfc3339())
            .header("X-Signature", "c2ln")
            .body(Body::empty())
            .unwrap();

        assert_ne!(status_of(&app, request).await, AxumStatus::OK);
    }

    #[tokio::test]
    async fn remote_mode_accepts_only_the_operator_bearer() {
        let app = make_router(config_with_mode(HttpAuthMode::Remote));
        let operator = AxumRequest::builder()
            .uri("/protected")
            .header(AUTHORIZATION, "Bearer bearer-current")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(&app, operator).await, AxumStatus::OK);

        let unknown = AxumRequest::builder()
            .uri("/protected")
            .header(AUTHORIZATION, "Bearer some-old-session-token")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(&app, unknown).await, AxumStatus::FORBIDDEN);

        let api_key = AxumRequest::builder()
            .uri("/protected")
            .header("X-API-Key", "test-key")
            .body(Body::empty())
            .unwrap();
        assert_ne!(status_of(&app, api_key).await, AxumStatus::OK);
    }
}
