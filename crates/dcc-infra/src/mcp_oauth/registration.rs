use reqwest::{header::HeaderMap, Method, StatusCode};
use serde_json::{json, Value};

use super::{
    bounded_text,
    credential::{ClientAuthMethod, ClientRegistrationKind, McpOauthClient},
    discovery::AuthorizationServerMetadata,
    http::{oauth_error_text, McpOauthHttp, RequestBody},
    McpOauthError, McpOauthResult, DCC_CLIENT_METADATA_URL, DCC_CLIENT_NAME, DCC_CLIENT_URI,
};

/// An OAuth app the user registered with the vendor (e.g. a Slack app),
/// for servers that accept neither metadata documents nor registration.
#[derive(Clone, PartialEq, Eq)]
pub struct UserProvidedClient {
    pub client_id: String,
    pub client_secret: Option<String>,
}

impl std::fmt::Debug for UserProvidedClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserProvidedClient")
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// Picks how DCC identifies itself: the user's own app, then the hosted
/// metadata document, then dynamic registration. A previously registered
/// dynamic client is reused while its redirect URI still matches.
pub(super) async fn resolve_client(
    http: &McpOauthHttp,
    metadata: &AuthorizationServerMetadata,
    redirect_uri: &str,
    metadata_document_allowed: bool,
    user_client: Option<&UserProvidedClient>,
    previous: Option<&McpOauthClient>,
) -> McpOauthResult<McpOauthClient> {
    if let Some(user_client) = user_client {
        return Ok(user_provided(metadata, redirect_uri, user_client));
    }
    if let Some(previous) = previous.filter(|client| {
        client.redirect_uri == redirect_uri
            && client.registration != ClientRegistrationKind::MetadataDocument
    }) {
        return Ok(previous.clone());
    }
    if metadata_document_allowed && metadata.client_id_metadata_document_supported {
        return Ok(McpOauthClient {
            client_id: DCC_CLIENT_METADATA_URL.to_string(),
            client_secret: None,
            auth_method: ClientAuthMethod::None,
            registration: ClientRegistrationKind::MetadataDocument,
            redirect_uri: redirect_uri.to_string(),
        });
    }
    if metadata.registration_endpoint.is_some() {
        return register_dynamic_client(http, metadata, redirect_uri).await;
    }
    Err(McpOauthError::ClientRegistrationRequired)
}

fn user_provided(
    metadata: &AuthorizationServerMetadata,
    redirect_uri: &str,
    user_client: &UserProvidedClient,
) -> McpOauthClient {
    let supported = &metadata.token_endpoint_auth_methods_supported;
    let auth_method = match user_client.client_secret {
        None => ClientAuthMethod::None,
        // RFC 8414 defaults to client_secret_basic when the list is absent.
        Some(_)
            if supported
                .iter()
                .any(|method| method == "client_secret_post")
                && !supported
                    .iter()
                    .any(|method| method == "client_secret_basic") =>
        {
            ClientAuthMethod::ClientSecretPost
        }
        Some(_) => ClientAuthMethod::ClientSecretBasic,
    };
    McpOauthClient {
        client_id: user_client.client_id.clone(),
        client_secret: user_client.client_secret.clone(),
        auth_method,
        registration: ClientRegistrationKind::UserProvided,
        redirect_uri: redirect_uri.to_string(),
    }
}

async fn register_dynamic_client(
    http: &McpOauthHttp,
    metadata: &AuthorizationServerMetadata,
    redirect_uri: &str,
) -> McpOauthResult<McpOauthClient> {
    let Some(endpoint) = &metadata.registration_endpoint else {
        return Err(McpOauthError::ClientRegistrationRequired);
    };
    let request = json!({
        "client_name": DCC_CLIENT_NAME,
        "client_uri": DCC_CLIENT_URI,
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        // SEP-837: lets servers accept loopback redirects for native apps.
        "application_type": "native",
    });
    let reply = http
        .send(
            Method::POST,
            endpoint,
            HeaderMap::new(),
            Some(RequestBody::Json(&request)),
        )
        .await?;
    if reply.status.is_server_error() {
        return Err(McpOauthError::Transient(format!(
            "Client registration is unavailable ({})",
            reply.status.as_u16()
        )));
    }
    if matches!(
        reply.status,
        StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED
    ) {
        return Err(McpOauthError::ClientRegistrationRejected(
            "it may only accept apps it has approved".to_string(),
        ));
    }
    if !reply.status.is_success() {
        return Err(McpOauthError::ClientRegistrationRejected(oauth_error_text(
            &reply,
        )));
    }
    let body = reply.json().unwrap_or(Value::Null);
    let client_id = body
        .get("client_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            McpOauthError::ClientRegistrationRejected("no client_id was returned".to_string())
        })?;
    let client_secret = body
        .get("client_secret")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let auth_method = match (
        client_secret.is_some(),
        body.get("token_endpoint_auth_method")
            .and_then(Value::as_str),
    ) {
        (false, _) => ClientAuthMethod::None,
        (true, Some("client_secret_post")) => ClientAuthMethod::ClientSecretPost,
        (true, _) => ClientAuthMethod::ClientSecretBasic,
    };
    Ok(McpOauthClient {
        client_id: bounded_text(client_id),
        client_secret,
        auth_method,
        registration: ClientRegistrationKind::Dynamic,
        redirect_uri: redirect_uri.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    fn metadata(
        cimd: bool,
        registration: bool,
        auth_methods: &[&str],
    ) -> AuthorizationServerMetadata {
        AuthorizationServerMetadata {
            issuer: "https://auth.example.com".into(),
            authorization_endpoint: Url::parse("https://auth.example.com/authorize").unwrap(),
            token_endpoint: Url::parse("https://auth.example.com/token").unwrap(),
            registration_endpoint: registration
                .then(|| Url::parse("https://auth.example.com/register").unwrap()),
            revocation_endpoint: None,
            scopes_supported: vec![],
            token_endpoint_auth_methods_supported: auth_methods
                .iter()
                .map(|m| m.to_string())
                .collect(),
            client_id_metadata_document_supported: cimd,
            authorization_response_iss_parameter_supported: false,
        }
    }

    const REDIRECT: &str = "http://127.0.0.1:46327/oauth/mcp/callback";

    #[tokio::test]
    async fn prefers_the_metadata_document_when_allowed() {
        let http = McpOauthHttp::new().unwrap();
        let client = resolve_client(
            &http,
            &metadata(true, true, &[]),
            REDIRECT,
            true,
            None,
            None,
        )
        .await
        .expect("client");
        assert_eq!(
            client.registration,
            ClientRegistrationKind::MetadataDocument
        );
        assert_eq!(client.client_id, DCC_CLIENT_METADATA_URL);
    }

    #[tokio::test]
    async fn needs_a_user_app_without_cimd_or_registration() {
        let http = McpOauthHttp::new().unwrap();
        let error = resolve_client(
            &http,
            &metadata(true, false, &[]),
            REDIRECT,
            false,
            None,
            None,
        )
        .await
        .expect_err("no way to identify DCC");
        assert_eq!(error, McpOauthError::ClientRegistrationRequired);
    }

    #[tokio::test]
    async fn reuses_a_dynamic_client_with_the_same_redirect() {
        let http = McpOauthHttp::new().unwrap();
        let previous = McpOauthClient {
            client_id: "registered".into(),
            client_secret: None,
            auth_method: ClientAuthMethod::None,
            registration: ClientRegistrationKind::Dynamic,
            redirect_uri: REDIRECT.into(),
        };
        let client = resolve_client(
            &http,
            &metadata(false, true, &[]),
            REDIRECT,
            true,
            None,
            Some(&previous),
        )
        .await
        .expect("client");
        assert_eq!(client.client_id, "registered");
    }

    #[test]
    fn slack_style_servers_get_client_secret_post() {
        let user = UserProvidedClient {
            client_id: "123.456".into(),
            client_secret: Some("secret".into()),
        };
        let client = user_provided(
            &metadata(false, false, &["client_secret_post"]),
            REDIRECT,
            &user,
        );
        assert_eq!(client.auth_method, ClientAuthMethod::ClientSecretPost);
        let client = user_provided(&metadata(false, false, &[]), REDIRECT, &user);
        assert_eq!(client.auth_method, ClientAuthMethod::ClientSecretBasic);
    }
}
