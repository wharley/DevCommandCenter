use std::{convert::Infallible, net::IpAddr, time::Duration};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{
        header::{ACCEPT, CONTENT_TYPE, ORIGIN},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Router,
};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use url::Url;

use crate::{FixtureServer, SUPPORTED_PROTOCOL_VERSIONS};

const MAX_HTTP_MESSAGE_BYTES: usize = 64 * 1024;
const MCP_PROTOCOL_VERSION: &str = "mcp-protocol-version";

#[derive(Debug, Error)]
pub enum HttpFixtureError {
    #[error("HTTP fixture must bind to a loopback address")]
    NonLoopbackBind,
    #[error("HTTP fixture I/O failed")]
    Io(#[from] std::io::Error),
}

pub fn build_router(server: FixtureServer) -> Router {
    Router::new()
        .route("/mcp", get(get_messages).post(post_message))
        .layer(DefaultBodyLimit::max(MAX_HTTP_MESSAGE_BYTES))
        .with_state(server)
}

pub async fn serve_http(
    listener: TcpListener,
    server: FixtureServer,
) -> Result<(), HttpFixtureError> {
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(HttpFixtureError::NonLoopbackBind);
    }
    axum::serve(listener, build_router(server)).await?;
    Ok(())
}

async fn post_message(
    State(server): State<FixtureServer>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !origin_is_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !accepts(&headers, "application/json") || !accepts(&headers, "text/event-stream") {
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    if !content_type_is_json(&headers) {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    let message = match serde_json::from_slice::<Value>(&body) {
        Ok(message) => message,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": "Parse error" }
                }),
            )
        }
    };

    let is_initialize = message.get("method").and_then(Value::as_str) == Some("initialize");
    if !is_initialize && !supported_protocol_header(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match server.handle_message(message).await {
        Some(response) => json_response(StatusCode::OK, response),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

async fn get_messages(State(server): State<FixtureServer>, headers: HeaderMap) -> Response {
    if !origin_is_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !accepts(&headers, "text/event-stream") {
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    if !supported_protocol_header(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let stream = BroadcastStream::new(server.subscribe()).filter_map(|message| match message {
        Ok(message) => Some(Ok::<_, Infallible>(
            Event::default().data(message.to_string()),
        )),
        Err(_) => None,
    });
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
}

fn json_response(status: StatusCode, body: Value) -> Response {
    (
        status,
        [(CONTENT_TYPE, HeaderValue::from_static("application/json"))],
        body.to_string(),
    )
        .into_response()
}

fn accepts(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get_all(ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|value| value.trim().split(';').next().unwrap_or_default())
        .any(|value| value == expected || value == "*/*")
}

fn content_type_is_json(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
}

fn supported_protocol_header(headers: &HeaderMap) -> bool {
    headers
        .get(MCP_PROTOCOL_VERSION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|version| SUPPORTED_PROTOCOL_VERSIONS.contains(&version))
}

fn origin_is_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(origin) = Url::parse(origin) else {
        return false;
    };
    if !matches!(origin.scheme(), "http" | "https") {
        return false;
    }
    match origin.host_str() {
        Some("localhost") => true,
        Some(host) => host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;

    use super::*;

    fn post_request(body: Value) -> Request<Body> {
        Request::post("/mcp")
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("build request")
    }

    #[tokio::test]
    async fn http_initialize_returns_json_without_requiring_version_header() {
        let response = build_router(FixtureServer::new())
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "test", "version": "1" }
                }
            })))
            .await
            .expect("HTTP response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
        let body = to_bytes(response.into_body(), MAX_HTTP_MESSAGE_BYTES)
            .await
            .expect("response body");
        let body: Value = serde_json::from_slice(&body).expect("JSON response");
        assert_eq!(body["result"]["protocolVersion"], "2025-11-25");
    }

    #[tokio::test]
    async fn http_requires_protocol_version_after_initialization() {
        let response = build_router(FixtureServer::new())
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            })))
            .await
            .expect("HTTP response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn http_accepts_notifications_with_202() {
        let mut request = post_request(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }));
        request
            .headers_mut()
            .insert(MCP_PROTOCOL_VERSION, HeaderValue::from_static("2025-11-25"));
        let response = build_router(FixtureServer::new())
            .oneshot(request)
            .await
            .expect("HTTP response");

        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn http_rejects_non_loopback_browser_origins() {
        let mut request = post_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }));
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static("https://evil.example"));
        let response = build_router(FixtureServer::new())
            .oneshot(request)
            .await
            .expect("HTTP response");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn http_get_opens_an_sse_notification_stream() {
        let request = Request::get("/mcp")
            .header(ACCEPT, "text/event-stream")
            .header(MCP_PROTOCOL_VERSION, "2025-11-25")
            .body(Body::empty())
            .expect("build request");
        let response = build_router(FixtureServer::new())
            .oneshot(request)
            .await
            .expect("HTTP response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream")));
    }
}
