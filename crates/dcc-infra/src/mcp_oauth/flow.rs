use base64::{
    engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD, Engine as _,
};
use rand::RngCore;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
    Method,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

use super::{
    bounded_text,
    credential::{ClientAuthMethod, McpOauthClient, McpOauthCredential, McpOauthTokens},
    discovery::{
        fetch_authorization_server_metadata, fetch_protected_resource_metadata, probe_server,
        AuthorizationServerMetadata, ServerAuthRequirement,
    },
    http::{
        oauth_error_code, oauth_error_text, require_secure_endpoint, HttpReply, McpOauthHttp,
        RequestBody,
    },
    registration::{resolve_client, UserProvidedClient},
    McpOauthError, McpOauthResult,
};

#[derive(Clone, Debug, Default)]
pub struct BeginOptions {
    pub redirect_uri: String,
    /// Only the fixed callback port matches the redirect published in the
    /// metadata document; a fallback port must use registration instead.
    pub metadata_document_allowed: bool,
    pub user_client: Option<UserProvidedClient>,
    /// Client from an earlier sign-in to the same connector, reused when its
    /// redirect still matches so reconnecting does not register again.
    pub previous_client: Option<McpOauthClient>,
    /// Scopes the user chose for this connector; they replace the server's
    /// suggestion (like `codex mcp login --scopes`).
    pub scopes: Option<String>,
}

#[derive(Debug)]
pub enum AuthorizationStart {
    NoAuthRequired,
    Authorize(Box<PendingAuthorization>),
}

/// A sign-in waiting for its browser callback. Holds the PKCE verifier, so it
/// stays in backend memory and is never serialized.
pub struct PendingAuthorization {
    pub state: String,
    pub authorization_url: Url,
    pub redirect_uri: String,
    server_url: String,
    resource: String,
    metadata: AuthorizationServerMetadata,
    client: McpOauthClient,
    code_verifier: String,
}

impl std::fmt::Debug for PendingAuthorization {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingAuthorization")
            .field("server_url", &self.server_url)
            .field("issuer", &self.metadata.issuer)
            .field("client", &self.client)
            .finish()
    }
}

/// Query parameters of the redirect back to DCC.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub iss: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// Probes the server and, when it asks for OAuth, discovers its authorization
/// server, identifies DCC, and builds the authorization URL.
pub async fn begin_authorization(
    http: &McpOauthHttp,
    server_url: &str,
    options: BeginOptions,
) -> McpOauthResult<AuthorizationStart> {
    let server_url = Url::parse(server_url).map_err(|_| McpOauthError::NotMcpServer)?;
    require_secure_endpoint(&server_url)?;
    let challenge = match probe_server(http, &server_url).await? {
        ServerAuthRequirement::None => return Ok(AuthorizationStart::NoAuthRequired),
        ServerAuthRequirement::OAuth(challenge) => challenge,
    };
    let resource_metadata =
        fetch_protected_resource_metadata(http, &server_url, &challenge).await?;
    // The first listed authorization server is the default per RFC 9728.
    let issuer = &resource_metadata.authorization_servers[0];
    let metadata = fetch_authorization_server_metadata(http, issuer).await?;
    let client = resolve_client(
        http,
        &metadata,
        &options.redirect_uri,
        options.metadata_document_allowed,
        options.user_client.as_ref(),
        options.previous_client.as_ref(),
    )
    .await?;

    let scope = options
        .scopes
        .clone()
        .or_else(|| challenge.scope.clone())
        .or_else(|| {
            (!resource_metadata.scopes_supported.is_empty())
                .then(|| resource_metadata.scopes_supported.join(" "))
        });
    let state = random_token();
    let code_verifier = random_token();
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));

    let mut authorization_url = metadata.authorization_endpoint.clone();
    {
        let mut query = authorization_url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &client.client_id)
            .append_pair("redirect_uri", &options.redirect_uri)
            .append_pair("code_challenge", &code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("resource", &resource_metadata.resource);
        if let Some(scope) = &scope {
            query.append_pair("scope", scope);
        }
    }

    Ok(AuthorizationStart::Authorize(Box::new(
        PendingAuthorization {
            state,
            authorization_url,
            redirect_uri: options.redirect_uri,
            server_url: server_url.to_string(),
            resource: resource_metadata.resource,
            metadata,
            client,
            code_verifier,
        },
    )))
}

/// Validates the browser callback and redeems the code.
pub async fn complete_authorization(
    http: &McpOauthHttp,
    pending: PendingAuthorization,
    params: CallbackParams,
    now_ms: i64,
) -> McpOauthResult<McpOauthCredential> {
    if params.state.as_deref() != Some(pending.state.as_str()) {
        return Err(McpOauthError::StateMismatch);
    }
    // RFC 9207: an `iss` that is present must match; a server that promises
    // it must send it. Checked before the code is redeemed.
    match params.iss.as_deref() {
        Some(iss) if iss.trim_end_matches('/') != pending.metadata.issuer.trim_end_matches('/') => {
            return Err(McpOauthError::IssuerMismatch)
        }
        None if pending
            .metadata
            .authorization_response_iss_parameter_supported =>
        {
            return Err(McpOauthError::IssuerMismatch)
        }
        _ => {}
    }
    if let Some(error) = params.error {
        let text = params
            .error_description
            .filter(|description| !description.trim().is_empty())
            .unwrap_or(error);
        return Err(McpOauthError::AuthorizationDenied(bounded_text(&text)));
    }
    let code = params
        .code
        .filter(|code| !code.is_empty())
        .ok_or_else(|| McpOauthError::TokenExchange("no authorization code was returned".into()))?;

    let reply = token_request(
        http,
        &pending.metadata.token_endpoint,
        &pending.client,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &pending.redirect_uri),
            ("code_verifier", &pending.code_verifier),
            ("resource", &pending.resource),
        ],
    )
    .await?;
    if reply.status.is_server_error() {
        return Err(McpOauthError::Transient(format!(
            "The token endpoint is unavailable ({})",
            reply.status.as_u16()
        )));
    }
    if !reply.status.is_success() {
        return Err(McpOauthError::TokenExchange(oauth_error_text(&reply)));
    }
    let tokens = parse_token_response(&reply, now_ms, None)
        .ok_or_else(|| McpOauthError::TokenExchange("the token response was invalid".into()))?;

    Ok(McpOauthCredential::new(
        pending.server_url,
        pending.resource,
        pending.metadata.issuer,
        pending.metadata.token_endpoint.to_string(),
        pending
            .metadata
            .revocation_endpoint
            .map(|endpoint| endpoint.to_string()),
        pending.client,
        tokens,
    ))
}

/// Exchanges the refresh token. Only a definitive rejection
/// (`invalid_grant`, `invalid_client`, `unauthorized_client`) means the user
/// must sign in again; anything else is transient.
pub async fn refresh_credential(
    http: &McpOauthHttp,
    credential: &McpOauthCredential,
    now_ms: i64,
) -> McpOauthResult<McpOauthCredential> {
    let Some(refresh_token) = credential.tokens.refresh_token.as_deref() else {
        return Err(McpOauthError::ReauthorizationRequired);
    };
    let endpoint = Url::parse(&credential.token_endpoint)
        .map_err(|_| McpOauthError::ReauthorizationRequired)?;
    let reply = token_request(
        http,
        &endpoint,
        &credential.client,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("resource", &credential.resource),
        ],
    )
    .await?;
    if !reply.status.is_success() {
        return Err(match oauth_error_code(&reply).as_deref() {
            Some("invalid_grant" | "invalid_client" | "unauthorized_client") => {
                McpOauthError::ReauthorizationRequired
            }
            _ => McpOauthError::Transient(format!(
                "Could not refresh the sign-in ({})",
                oauth_error_text(&reply)
            )),
        });
    }
    let tokens = parse_token_response(&reply, now_ms, Some(refresh_token))
        .ok_or_else(|| McpOauthError::Transient("the refresh response was invalid".to_string()))?;
    let mut refreshed = credential.clone();
    refreshed.tokens = tokens;
    Ok(refreshed)
}

/// RFC 7009, best effort: the refresh token first (it can mint new access
/// tokens), then the access token. Failures are ignored; the local grant is
/// deleted either way.
pub async fn revoke_credential(http: &McpOauthHttp, credential: &McpOauthCredential) {
    let Some(endpoint) = credential
        .revocation_endpoint
        .as_deref()
        .and_then(|endpoint| Url::parse(endpoint).ok())
    else {
        return;
    };
    if let Some(refresh_token) = credential.tokens.refresh_token.as_deref() {
        let _ = token_request(
            http,
            &endpoint,
            &credential.client,
            &[
                ("token", refresh_token),
                ("token_type_hint", "refresh_token"),
            ],
        )
        .await;
    }
    let _ = token_request(
        http,
        &endpoint,
        &credential.client,
        &[
            ("token", &credential.tokens.access_token),
            ("token_type_hint", "access_token"),
        ],
    )
    .await;
}

async fn token_request(
    http: &McpOauthHttp,
    endpoint: &Url,
    client: &McpOauthClient,
    fields: &[(&str, &str)],
) -> McpOauthResult<HttpReply> {
    let mut form = fields.to_vec();
    let mut headers = HeaderMap::new();
    match (client.auth_method, client.client_secret.as_deref()) {
        (ClientAuthMethod::ClientSecretBasic, Some(secret)) => {
            let encode = |value: &str| {
                url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
            };
            let credentials =
                STANDARD.encode(format!("{}:{}", encode(&client.client_id), encode(secret)));
            let value = HeaderValue::from_str(&format!("Basic {credentials}"))
                .map_err(|_| McpOauthError::TokenExchange("invalid client credentials".into()))?;
            headers.insert(AUTHORIZATION, value);
        }
        (ClientAuthMethod::ClientSecretPost, Some(secret)) => {
            form.push(("client_id", &client.client_id));
            form.push(("client_secret", secret));
        }
        _ => form.push(("client_id", &client.client_id)),
    }
    http.send(
        Method::POST,
        endpoint,
        headers,
        Some(RequestBody::Form(&form)),
    )
    .await
}

fn parse_token_response(
    reply: &HttpReply,
    now_ms: i64,
    previous_refresh_token: Option<&str>,
) -> Option<McpOauthTokens> {
    let body = reply.json()?;
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())?;
    let token_type = body
        .get("token_type")
        .and_then(Value::as_str)
        .unwrap_or("Bearer");
    if !token_type.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let expires_at_ms = body
        .get("expires_in")
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
        .filter(|seconds| *seconds > 0)
        .map(|seconds| now_ms.saturating_add(seconds.saturating_mul(1000)));
    // Servers may rotate refresh tokens; keep the old one when they don't.
    let refresh_token = body
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .or_else(|| previous_refresh_token.map(str::to_string));
    Some(McpOauthTokens {
        access_token: access_token.to_string(),
        refresh_token,
        scope: body.get("scope").and_then(Value::as_str).map(bounded_text),
        expires_at_ms,
    })
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::*;

    fn reply(body: &str) -> HttpReply {
        HttpReply {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn token_response_keeps_the_previous_refresh_token() {
        let tokens = parse_token_response(
            &reply(r#"{"access_token":"a","token_type":"bearer","expires_in":3600}"#),
            1_000,
            Some("old-refresh"),
        )
        .expect("tokens");
        assert_eq!(tokens.refresh_token.as_deref(), Some("old-refresh"));
        assert_eq!(tokens.expires_at_ms, Some(3_601_000));
    }

    #[test]
    fn token_response_rejects_non_bearer_tokens() {
        assert!(parse_token_response(
            &reply(r#"{"access_token":"a","token_type":"mac"}"#),
            0,
            None
        )
        .is_none());
    }

    #[test]
    fn random_tokens_are_43_url_safe_characters() {
        let token = random_token();
        assert_eq!(token.len(), 43);
        assert!(token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'));
        assert_ne!(token, random_token());
    }
}
