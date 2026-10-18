use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, WWW_AUTHENTICATE},
    Method, StatusCode,
};
use serde_json::{json, Value};
use url::Url;

use super::{
    bounded_text,
    http::{advertised_url, host, require_secure_endpoint, McpOauthHttp, RequestBody},
    McpOauthError, McpOauthResult,
};

const PROBE_PROTOCOL_VERSION: &str = "2025-11-25";

/// What an unauthenticated `initialize` tells us about a server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerAuthRequirement {
    /// The server answered `initialize` without a token.
    None,
    OAuth(AuthChallenge),
}

/// Parameters of a `WWW-Authenticate: Bearer` challenge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthChallenge {
    pub resource_metadata: Option<String>,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: Url,
    pub token_endpoint: Url,
    pub registration_endpoint: Option<Url>,
    pub revocation_endpoint: Option<Url>,
    pub scopes_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
    pub client_id_metadata_document_supported: bool,
    pub authorization_response_iss_parameter_supported: bool,
}

/// Sends `initialize` without credentials. A complete result means the server
/// needs no sign-in; a 401 starts OAuth.
pub(super) async fn probe_server(
    http: &McpOauthHttp,
    server_url: &Url,
) -> McpOauthResult<ServerAuthRequirement> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROBE_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "dcc-connector-probe", "version": env!("CARGO_PKG_VERSION") }
        }
    });
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    headers.insert(
        "mcp-protocol-version",
        HeaderValue::from_static(PROBE_PROTOCOL_VERSION),
    );
    let reply = http
        .send(
            Method::POST,
            server_url,
            headers,
            Some(RequestBody::Json(&body)),
        )
        .await?;

    match reply.status {
        StatusCode::UNAUTHORIZED => Ok(ServerAuthRequirement::OAuth(
            reply
                .headers
                .get_all(WWW_AUTHENTICATE)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .find_map(parse_www_authenticate)
                .unwrap_or_default(),
        )),
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED => Err(McpOauthError::NotMcpServer),
        status if status.is_server_error() => Err(McpOauthError::Transient(format!(
            "{} is unavailable ({})",
            host(server_url),
            status.as_u16()
        ))),
        status if status.is_success() => {
            let content_type = reply
                .headers
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            let message = if content_type.starts_with("text/event-stream") {
                first_sse_message(&reply.body)
            } else {
                reply.json()
            };
            if let Some(session_id) = reply
                .headers
                .get("mcp-session-id")
                .and_then(|value| value.to_str().ok())
            {
                close_probe_session(http, server_url, session_id).await;
            }
            if message.as_ref().is_some_and(is_initialize_result) {
                Ok(ServerAuthRequirement::None)
            } else {
                Err(McpOauthError::Discovery(
                    "The server did not return a valid MCP initialize response".to_string(),
                ))
            }
        }
        status => Err(McpOauthError::Discovery(format!(
            "{} answered HTTP {} to an MCP request",
            host(server_url),
            status.as_u16()
        ))),
    }
}

async fn close_probe_session(http: &McpOauthHttp, server_url: &Url, session_id: &str) {
    let Ok(value) = HeaderValue::from_str(session_id) else {
        return;
    };
    let mut headers = HeaderMap::new();
    headers.insert("mcp-session-id", value);
    let _ = http.send(Method::DELETE, server_url, headers, None).await;
}

fn first_sse_message(body: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(body).ok()?;
    let mut data = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data.push_str(rest.trim_start());
        } else if line.is_empty() && !data.is_empty() {
            break;
        }
    }
    serde_json::from_str(&data).ok()
}

fn is_initialize_result(message: &Value) -> bool {
    let Some(result) = message.get("result") else {
        return false;
    };
    result
        .get("protocolVersion")
        .and_then(Value::as_str)
        .is_some()
        && result.get("capabilities").is_some_and(Value::is_object)
        && result
            .get("serverInfo")
            .and_then(|info| info.get("name"))
            .and_then(Value::as_str)
            .is_some()
}

/// Parses one `WWW-Authenticate` header value and returns the Bearer
/// challenge's `resource_metadata` and `scope` parameters.
pub fn parse_www_authenticate(header: &str) -> Option<AuthChallenge> {
    let trimmed = header.trim_start();
    let (scheme, rest) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let mut challenge = AuthChallenge::default();
    let mut remaining = rest.trim();
    while !remaining.is_empty() {
        let Some((name, after_name)) = remaining.split_once('=') else {
            break;
        };
        let name = name.trim().trim_start_matches(',').trim();
        let after_name = after_name.trim_start();
        let (value, after_value) = if let Some(quoted) = after_name.strip_prefix('"') {
            let mut value = String::new();
            let mut escaped = false;
            let mut end = quoted.len();
            for (index, character) in quoted.char_indices() {
                if escaped {
                    value.push(character);
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    end = index + 1;
                    break;
                } else {
                    value.push(character);
                }
            }
            (value, &quoted[end.min(quoted.len())..])
        } else {
            let end = after_name.find(',').unwrap_or(after_name.len());
            (after_name[..end].trim().to_string(), &after_name[end..])
        };
        match name.to_ascii_lowercase().as_str() {
            "resource_metadata" if !value.is_empty() => challenge.resource_metadata = Some(value),
            "scope" if !value.is_empty() => challenge.scope = Some(value),
            _ => {}
        }
        remaining = after_value
            .trim_start()
            .trim_start_matches(',')
            .trim_start();
    }
    Some(challenge)
}

/// RFC 9728 discovery: the challenge's `resource_metadata` first, then the
/// well-known path for the server's path, then the origin root.
pub(super) async fn fetch_protected_resource_metadata(
    http: &McpOauthHttp,
    server_url: &Url,
    challenge: &AuthChallenge,
) -> McpOauthResult<ProtectedResourceMetadata> {
    let mut candidates = Vec::new();
    if let Some(advertised) = &challenge.resource_metadata {
        candidates.push(advertised_url(advertised)?);
    }
    let path = server_url.path().trim_end_matches('/');
    if !path.is_empty() {
        candidates.push(well_known(
            server_url,
            "oauth-protected-resource",
            path,
            false,
        ));
    }
    candidates.push(well_known(
        server_url,
        "oauth-protected-resource",
        "",
        false,
    ));

    for candidate in candidates {
        let Some(document) = http.get_json(&candidate).await? else {
            continue;
        };
        let resource = document
            .get("resource")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| server_url.as_str().to_string());
        if !resource_covers_server(&resource, server_url) {
            return Err(McpOauthError::Discovery(format!(
                "{} described a different resource",
                host(server_url)
            )));
        }
        let authorization_servers = string_list(&document, "authorization_servers");
        if authorization_servers.is_empty() {
            return Err(McpOauthError::Discovery(format!(
                "{} did not name an authorization server",
                host(server_url)
            )));
        }
        return Ok(ProtectedResourceMetadata {
            resource,
            authorization_servers,
            scopes_supported: string_list(&document, "scopes_supported"),
        });
    }
    Err(McpOauthError::Discovery(format!(
        "{} asks for a sign-in but publishes no OAuth metadata",
        host(server_url)
    )))
}

/// RFC 8414 and OpenID discovery with path insertion, then OpenID with the
/// path appended.
pub(super) async fn fetch_authorization_server_metadata(
    http: &McpOauthHttp,
    issuer: &str,
) -> McpOauthResult<AuthorizationServerMetadata> {
    let issuer_url = advertised_url(issuer)?;
    let path = issuer_url.path().trim_end_matches('/');
    let candidates = if path.is_empty() {
        vec![
            well_known(&issuer_url, "oauth-authorization-server", "", false),
            well_known(&issuer_url, "openid-configuration", "", false),
        ]
    } else {
        vec![
            well_known(&issuer_url, "oauth-authorization-server", path, false),
            well_known(&issuer_url, "openid-configuration", path, false),
            well_known(&issuer_url, "openid-configuration", path, true),
        ]
    };

    for candidate in candidates {
        let Some(document) = http.get_json(&candidate).await? else {
            continue;
        };
        return parse_authorization_server_metadata(&document, issuer);
    }
    Err(McpOauthError::Discovery(format!(
        "{} publishes no authorization server metadata",
        host(&issuer_url)
    )))
}

fn parse_authorization_server_metadata(
    document: &Value,
    expected_issuer: &str,
) -> McpOauthResult<AuthorizationServerMetadata> {
    let issuer = document
        .get("issuer")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if issuer.trim_end_matches('/') != expected_issuer.trim_end_matches('/') {
        return Err(McpOauthError::IssuerMismatch);
    }
    let endpoint = |name: &str| -> McpOauthResult<Option<Url>> {
        document
            .get(name)
            .and_then(Value::as_str)
            .map(advertised_url)
            .transpose()
    };
    let authorization_endpoint = endpoint("authorization_endpoint")?.ok_or_else(|| {
        McpOauthError::Discovery("The authorization server has no authorization endpoint".into())
    })?;
    let token_endpoint = endpoint("token_endpoint")?.ok_or_else(|| {
        McpOauthError::Discovery("The authorization server has no token endpoint".into())
    })?;
    // PKCE is mandatory. A server that does not advertise S256 support cannot
    // be assumed to enforce it.
    if !string_list(document, "code_challenge_methods_supported")
        .iter()
        .any(|method| method == "S256")
    {
        return Err(McpOauthError::Discovery(
            "The authorization server does not support PKCE (S256)".to_string(),
        ));
    }
    Ok(AuthorizationServerMetadata {
        issuer: issuer.to_string(),
        authorization_endpoint,
        token_endpoint,
        registration_endpoint: endpoint("registration_endpoint")?,
        revocation_endpoint: endpoint("revocation_endpoint")?,
        scopes_supported: string_list(document, "scopes_supported"),
        token_endpoint_auth_methods_supported: string_list(
            document,
            "token_endpoint_auth_methods_supported",
        ),
        client_id_metadata_document_supported: document
            .get("client_id_metadata_document_supported")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        authorization_response_iss_parameter_supported: document
            .get("authorization_response_iss_parameter_supported")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn well_known(base: &Url, name: &str, path: &str, append: bool) -> Url {
    let mut url = base.clone();
    url.set_query(None);
    url.set_fragment(None);
    if append {
        url.set_path(&format!("{path}/.well-known/{name}"));
    } else {
        url.set_path(&format!("/.well-known/{name}{path}"));
    }
    url
}

/// The metadata's resource must be the server URL or a path prefix of it on
/// the same origin, so a server cannot obtain tokens for someone else.
fn resource_covers_server(resource: &str, server_url: &Url) -> bool {
    let Ok(resource_url) = Url::parse(resource) else {
        return false;
    };
    if require_secure_endpoint(&resource_url).is_err()
        || resource_url.origin() != server_url.origin()
    {
        return false;
    }
    let resource_path = resource_url.path().trim_end_matches('/');
    let server_path = server_url.path().trim_end_matches('/');
    server_path == resource_path
        || resource_path.is_empty()
        || server_path.starts_with(&format!("{resource_path}/"))
}

fn string_list(document: &Value, name: &str) -> Vec<String> {
    document
        .get(name)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(bounded_text)
                .filter(|value| !value.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bearer_challenge_parameters() {
        let challenge = parse_www_authenticate(
            r#"Bearer realm="mcp", resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource", scope="read write""#,
        )
        .expect("bearer challenge");
        assert_eq!(
            challenge.resource_metadata.as_deref(),
            Some("https://mcp.example.com/.well-known/oauth-protected-resource")
        );
        assert_eq!(challenge.scope.as_deref(), Some("read write"));
    }

    #[test]
    fn parses_unquoted_values_and_ignores_other_schemes() {
        let challenge =
            parse_www_authenticate("Bearer error=invalid_token, scope=read").expect("bearer");
        assert_eq!(challenge.scope.as_deref(), Some("read"));
        assert!(challenge.resource_metadata.is_none());
        assert!(parse_www_authenticate(r#"Basic realm="x""#).is_none());
        assert_eq!(
            parse_www_authenticate("Bearer"),
            Some(AuthChallenge::default())
        );
    }

    #[test]
    fn builds_well_known_urls_with_path_insertion() {
        let server = Url::parse("https://mcp.atlassian.com/v1/mcp").unwrap();
        assert_eq!(
            well_known(&server, "oauth-protected-resource", "/v1/mcp", false).as_str(),
            "https://mcp.atlassian.com/.well-known/oauth-protected-resource/v1/mcp"
        );
        let issuer = Url::parse("https://auth.example.com/tenant").unwrap();
        assert_eq!(
            well_known(&issuer, "openid-configuration", "/tenant", true).as_str(),
            "https://auth.example.com/tenant/.well-known/openid-configuration"
        );
    }

    #[test]
    fn resource_must_cover_the_server_on_the_same_origin() {
        let server = Url::parse("https://mcp.slack.com/mcp").unwrap();
        assert!(resource_covers_server("https://mcp.slack.com", &server));
        assert!(resource_covers_server("https://mcp.slack.com/mcp", &server));
        assert!(!resource_covers_server(
            "https://mcp.slack.com/other",
            &server
        ));
        assert!(!resource_covers_server("https://evil.example/mcp", &server));
        assert!(!resource_covers_server("http://mcp.slack.com/mcp", &server));
    }

    #[test]
    fn rejects_metadata_without_s256_or_with_a_different_issuer() {
        let mut document = json!({
            "issuer": "https://auth.example.com",
            "authorization_endpoint": "https://auth.example.com/authorize",
            "token_endpoint": "https://auth.example.com/token",
            "code_challenge_methods_supported": ["plain"]
        });
        assert!(matches!(
            parse_authorization_server_metadata(&document, "https://auth.example.com"),
            Err(McpOauthError::Discovery(_))
        ));
        document["code_challenge_methods_supported"] = json!(["S256"]);
        assert!(
            parse_authorization_server_metadata(&document, "https://auth.example.com/").is_ok()
        );
        assert_eq!(
            parse_authorization_server_metadata(&document, "https://other.example.com"),
            Err(McpOauthError::IssuerMismatch)
        );
    }

    #[test]
    fn reads_the_first_sse_message() {
        let body = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"serverInfo\":{\"name\":\"x\",\"version\":\"1\"}}}\n\n";
        let message = first_sse_message(body).expect("message");
        assert!(is_initialize_result(&message));
    }
}
