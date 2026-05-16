use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::http_config::{HttpAuthMode, HttpConfig};
use crate::pairing::{self, SignedRequestHeaders};

const MAX_SIGNED_BODY_BYTES: usize = 32 * 1024 * 1024;

/// Middleware to validate HTTP auth.
///
/// Three accepted credentials, in this priority order:
///   1. Signed request (X-Device-Id + X-Timestamp + X-Signature) — mobile clients
///   2. X-API-Key — desktop / Local mode
///   3. Authorization: Bearer — remote tunnel clients
///
/// When signed-request headers are present, we commit to verifying them: failure
/// returns 401 without falling back. This prevents an attacker from confusing
/// us with bogus signed headers + a valid API key.
pub async fn auth_middleware(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, AuthError> {
    if has_signed_request_headers(request.headers()) {
        return validate_signed_request(config, request, next).await;
    }

    let headers = request.headers().clone();
    let config = config.read().await;
    validate_auth(&config, &headers)?;
    Ok(next.run(request).await)
}

fn has_signed_request_headers(headers: &HeaderMap) -> bool {
    headers.contains_key("X-Device-Id")
        && headers.contains_key("X-Timestamp")
        && headers.contains_key("X-Signature")
}

async fn validate_signed_request(
    config: Arc<RwLock<HttpConfig>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, AuthError> {
    let db_path: PathBuf = { config.read().await.db_path.clone() };

    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();
    let device_id = header_str(request.headers(), "X-Device-Id")
        .ok_or(AuthError::SignedRequestMissingHeader)?;
    let timestamp = header_str(request.headers(), "X-Timestamp")
        .ok_or(AuthError::SignedRequestMissingHeader)?;
    let signature = header_str(request.headers(), "X-Signature")
        .ok_or(AuthError::SignedRequestMissingHeader)?;
    let client_ip = header_str(request.headers(), "x-forwarded-for")
        .and_then(|v| v.split(',').next().map(|s| s.trim().to_string()));

    let (parts, body) = request.into_parts();
    let body_bytes = to_bytes(body, MAX_SIGNED_BODY_BYTES)
        .await
        .map_err(|_| AuthError::SignedRequestBodyTooLarge)?
        .to_vec();

    let verify_method = method.clone();
    let verify_path = path.clone();
    let verify_body = body_bytes.clone();
    let verify_ip = client_ip.clone();

    let result = tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| pairing::PairingError::Database(e.to_string()))?;
        pairing::verify_signed_request(
            &conn,
            SignedRequestHeaders {
                device_id: &device_id,
                timestamp: &timestamp,
                signature_b64: &signature,
            },
            &verify_method,
            &verify_path,
            &verify_body,
            verify_ip.as_deref(),
        )
    })
    .await
    .map_err(|_| AuthError::SignedRequestInternal)?;

    match result {
        Ok(_) => {
            let new_request = Request::from_parts(parts, Body::from(body_bytes));
            Ok(next.run(new_request).await)
        }
        Err(pairing::PairingError::UnknownDevice) => Err(AuthError::SignedRequestUnknownDevice),
        Err(pairing::PairingError::DeviceRevoked) => Err(AuthError::SignedRequestRevokedDevice),
        Err(pairing::PairingError::SessionExpired) => Err(AuthError::SignedRequestSessionExpired),
        Err(pairing::PairingError::TimestampOutOfWindow)
        | Err(pairing::PairingError::InvalidTimestamp) => {
            Err(AuthError::SignedRequestStaleTimestamp)
        }
        Err(_) => Err(AuthError::SignedRequestInvalidSignature),
    }
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
    SignedRequestMissingHeader,
    SignedRequestBodyTooLarge,
    SignedRequestInternal,
    SignedRequestUnknownDevice,
    SignedRequestRevokedDevice,
    SignedRequestStaleTimestamp,
    SignedRequestInvalidSignature,
    SignedRequestSessionExpired,
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
            AuthError::SignedRequestMissingHeader => (
                StatusCode::BAD_REQUEST,
                "Signed request missing X-Device-Id / X-Timestamp / X-Signature",
                None,
            ),
            AuthError::SignedRequestBodyTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "Signed request body exceeds maximum size",
                None,
            ),
            AuthError::SignedRequestInternal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Signed request verification failed (internal)",
                None,
            ),
            AuthError::SignedRequestUnknownDevice => (
                StatusCode::UNAUTHORIZED,
                "Device not paired",
                None,
            ),
            AuthError::SignedRequestRevokedDevice => (
                StatusCode::FORBIDDEN,
                "Device has been revoked",
                None,
            ),
            AuthError::SignedRequestStaleTimestamp => (
                StatusCode::FORBIDDEN,
                "Timestamp outside the replay window",
                None,
            ),
            AuthError::SignedRequestInvalidSignature => (
                StatusCode::FORBIDDEN,
                "Signature verification failed",
                None,
            ),
            AuthError::SignedRequestSessionExpired => (
                StatusCode::FORBIDDEN,
                "Device session expired; please re-pair",
                None,
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

    // ----- signed-request middleware (end-to-end through an axum router) -----

    use axum::{
        body::Body,
        http::{Request as AxumRequest, StatusCode as AxumStatus},
        middleware as axum_middleware,
        routing::{get, post},
        Router,
    };
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use p256::ecdsa::{signature::Signer, Signature, SigningKey};
    use p256::pkcs8::EncodePublicKey;
    use rand::rngs::OsRng;
    use tempfile::NamedTempFile;
    use tower::util::ServiceExt;

    use crate::pairing;

    struct SignedTestSetup {
        _tmp: NamedTempFile,
        config: Arc<RwLock<HttpConfig>>,
        signing: SigningKey,
        device_id: String,
    }

    fn build_signed_setup() -> SignedTestSetup {
        let tmp = NamedTempFile::new().expect("tempfile");
        let db_path = tmp.path().to_path_buf();

        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        conn.execute_batch(include_str!("../sql/schema.sql"))
            .expect("schema");

        let challenge = pairing::create_pairing_nonce(&conn).expect("nonce");
        let signing = SigningKey::random(&mut OsRng);
        let pubkey_der = signing
            .verifying_key()
            .to_public_key_der()
            .expect("spki");
        let pubkey_b64 = BASE64.encode(pubkey_der.as_bytes());

        let device_id = pairing::complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &pubkey_b64,
            "test-device",
            None,
            None,
        )
        .expect("complete_pairing");

        let mut config = HttpConfig::default();
        config.db_path = db_path;
        let config = Arc::new(RwLock::new(config));

        SignedTestSetup {
            _tmp: tmp,
            config,
            signing,
            device_id,
        }
    }

    fn make_router(config: Arc<RwLock<HttpConfig>>) -> Router {
        Router::new()
            .route("/protected", get(|| async { "ok" }))
            .route("/echo", post(|body: String| async move { body }))
            .route_layer(axum_middleware::from_fn_with_state(config.clone(), auth_middleware))
            .with_state(config)
    }

    fn sign_for(
        signing: &SigningKey,
        method: &str,
        path: &str,
        timestamp: &str,
        body: &[u8],
    ) -> String {
        let canonical = pairing::canonical_request_bytes(method, path, timestamp, body);
        let sig: Signature = signing.sign(&canonical);
        BASE64.encode(sig.to_der().as_bytes())
    }

    #[tokio::test]
    async fn signed_request_passes_through_middleware() {
        let setup = build_signed_setup();
        let app = make_router(setup.config.clone());

        let timestamp = chrono::Utc::now().to_rfc3339();
        let signature = sign_for(&setup.signing, "GET", "/protected", &timestamp, b"");

        let response = app
            .oneshot(
                AxumRequest::builder()
                    .method("GET")
                    .uri("/protected")
                    .header("X-Device-Id", &setup.device_id)
                    .header("X-Timestamp", &timestamp)
                    .header("X-Signature", &signature)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), AxumStatus::OK);
    }

    #[tokio::test]
    async fn signed_request_preserves_body_for_handler() {
        let setup = build_signed_setup();
        let app = make_router(setup.config.clone());

        let body = b"hello mobile";
        let timestamp = chrono::Utc::now().to_rfc3339();
        let signature = sign_for(&setup.signing, "POST", "/echo", &timestamp, body);

        let response = app
            .oneshot(
                AxumRequest::builder()
                    .method("POST")
                    .uri("/echo")
                    .header("X-Device-Id", &setup.device_id)
                    .header("X-Timestamp", &timestamp)
                    .header("X-Signature", &signature)
                    .body(Body::from(body.as_slice()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), AxumStatus::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&bytes[..], body);
    }

    #[tokio::test]
    async fn signed_request_rejects_revoked_device() {
        let setup = build_signed_setup();
        {
            let path = setup.config.read().await.db_path.clone();
            let conn = rusqlite::Connection::open(&path).unwrap();
            pairing::revoke_device(&conn, &setup.device_id, None).unwrap();
        }

        let app = make_router(setup.config.clone());
        let timestamp = chrono::Utc::now().to_rfc3339();
        let signature = sign_for(&setup.signing, "GET", "/protected", &timestamp, b"");

        let response = app
            .oneshot(
                AxumRequest::builder()
                    .method("GET")
                    .uri("/protected")
                    .header("X-Device-Id", &setup.device_id)
                    .header("X-Timestamp", &timestamp)
                    .header("X-Signature", &signature)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), AxumStatus::FORBIDDEN);
    }

    #[tokio::test]
    async fn signed_request_rejects_tampered_body() {
        let setup = build_signed_setup();
        let app = make_router(setup.config.clone());

        let timestamp = chrono::Utc::now().to_rfc3339();
        let signed_body = b"signed body";
        let tampered_body = b"tampered body";
        let signature = sign_for(&setup.signing, "POST", "/echo", &timestamp, signed_body);

        let response = app
            .oneshot(
                AxumRequest::builder()
                    .method("POST")
                    .uri("/echo")
                    .header("X-Device-Id", &setup.device_id)
                    .header("X-Timestamp", &timestamp)
                    .header("X-Signature", &signature)
                    .body(Body::from(tampered_body.as_slice()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), AxumStatus::FORBIDDEN);
    }

    #[tokio::test]
    async fn signed_headers_skip_api_key_fallback() {
        // Even if a valid API key is present, an invalid signature must NOT fall back.
        let setup = build_signed_setup();
        {
            let mut cfg = setup.config.write().await;
            cfg.api_key = "valid-key".to_string();
        }
        let app = make_router(setup.config.clone());

        let timestamp = chrono::Utc::now().to_rfc3339();

        let response = app
            .oneshot(
                AxumRequest::builder()
                    .method("GET")
                    .uri("/protected")
                    .header("X-Device-Id", &setup.device_id)
                    .header("X-Timestamp", &timestamp)
                    .header("X-Signature", "INVALIDSIG")
                    .header("X-API-Key", "valid-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(response.status(), AxumStatus::OK);
    }
}
