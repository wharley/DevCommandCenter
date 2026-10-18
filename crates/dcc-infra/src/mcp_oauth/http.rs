use std::{net::IpAddr, time::Duration};

use reqwest::{header::HeaderMap, Method, StatusCode};
use serde_json::Value;
use url::Url;

use super::{bounded_text, McpOauthError, McpOauthResult};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// HTTP client for discovery, registration, and token calls. Redirects are
/// never followed, and every endpoint must be HTTPS unless it is loopback
/// (local development servers and the test fixture).
#[derive(Clone, Debug)]
pub struct McpOauthHttp {
    client: reqwest::Client,
}

pub(super) struct HttpReply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl HttpReply {
    pub fn json(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }
}

impl McpOauthHttp {
    pub fn new() -> McpOauthResult<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(REQUEST_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("DevCommandCenter/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| McpOauthError::Transient("Could not start the HTTP client".to_string()))?;
        Ok(Self { client })
    }

    pub(super) async fn send(
        &self,
        method: Method,
        url: &Url,
        headers: HeaderMap,
        body: Option<RequestBody<'_>>,
    ) -> McpOauthResult<HttpReply> {
        require_secure_endpoint(url)?;
        let mut request = self.client.request(method, url.clone()).headers(headers);
        match body {
            Some(RequestBody::Json(value)) => request = request.json(value),
            Some(RequestBody::Form(fields)) => request = request.form(fields),
            None => {}
        }
        let mut response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                McpOauthError::Transient(format!("{} did not answer in time", host(url)))
            } else {
                McpOauthError::Transient(format!("Could not reach {}", host(url)))
            }
        })?;
        let status = response.status();
        let headers = response.headers().clone();
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| McpOauthError::Transient(format!("{} closed the connection", host(url))))?
        {
            if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(McpOauthError::Transient(format!(
                    "{} sent an oversized response",
                    host(url)
                )));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpReply {
            status,
            headers,
            body,
        })
    }

    pub(super) async fn get_json(&self, url: &Url) -> McpOauthResult<Option<Value>> {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let reply = self.send(Method::GET, url, headers, None).await?;
        if reply.status.is_server_error() {
            return Err(McpOauthError::Transient(format!(
                "{} is unavailable ({})",
                host(url),
                reply.status.as_u16()
            )));
        }
        if !reply.status.is_success() {
            return Ok(None);
        }
        Ok(reply.json())
    }
}

pub(super) enum RequestBody<'a> {
    Json(&'a Value),
    Form(&'a [(&'a str, &'a str)]),
}

pub(super) fn host(url: &Url) -> String {
    url.host_str().unwrap_or("the server").to_string()
}

pub(super) fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(url::Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        None => false,
    }
}

pub(super) fn require_secure_endpoint(url: &Url) -> McpOauthResult<()> {
    let allowed = match url.scheme() {
        "https" => true,
        "http" => is_loopback(url),
        _ => false,
    };
    if !allowed || url.username() != "" || url.password().is_some() || url.fragment().is_some() {
        return Err(McpOauthError::Discovery(format!(
            "{} must use HTTPS",
            bounded_text(url.as_str())
        )));
    }
    Ok(())
}

/// Parses a URL advertised by a server and applies the endpoint policy.
pub(super) fn advertised_url(value: &str) -> McpOauthResult<Url> {
    let url = Url::parse(value).map_err(|_| {
        McpOauthError::Discovery(format!(
            "The server advertised an invalid URL: {}",
            bounded_text(value)
        ))
    })?;
    require_secure_endpoint(&url)?;
    Ok(url)
}

/// Reads an OAuth error body (`error`, `error_description`) for messages.
pub(super) fn oauth_error_text(reply: &HttpReply) -> String {
    let Some(body) = reply.json() else {
        return format!("HTTP {}", reply.status.as_u16());
    };
    let code = body.get("error").and_then(Value::as_str).unwrap_or("");
    let description = body
        .get("error_description")
        .and_then(Value::as_str)
        .unwrap_or("");
    match (code.is_empty(), description.is_empty()) {
        (true, true) => format!("HTTP {}", reply.status.as_u16()),
        (false, true) => bounded_text(code),
        (true, false) => bounded_text(description),
        (false, false) => bounded_text(&format!("{code}: {description}")),
    }
}

pub(super) fn oauth_error_code(reply: &HttpReply) -> Option<String> {
    reply
        .json()?
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string)
}
