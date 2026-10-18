use std::fmt;

use dcc_core::ports::{CredentialStoreError, SecretValue};
use serde::{Deserialize, Serialize};

const CREDENTIAL_VERSION: u8 = 1;

/// Everything DCC needs to keep a connector signed in. Serialized whole into
/// one credential-store entry; SQLite only keeps the opaque reference.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOauthCredential {
    pub version: u8,
    pub server_url: String,
    /// RFC 8707 resource the tokens were issued for; refresh reuses it.
    pub resource: String,
    pub issuer: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub revocation_endpoint: Option<String>,
    pub client: McpOauthClient,
    pub tokens: McpOauthTokens,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOauthClient {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub auth_method: ClientAuthMethod,
    pub registration: ClientRegistrationKind,
    pub redirect_uri: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientAuthMethod {
    None,
    ClientSecretPost,
    ClientSecretBasic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientRegistrationKind {
    MetadataDocument,
    Dynamic,
    UserProvided,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOauthTokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    /// Unix milliseconds. `None` when the server did not say.
    #[serde(default)]
    pub expires_at_ms: Option<i64>,
}

impl McpOauthCredential {
    pub fn new(
        server_url: String,
        resource: String,
        issuer: String,
        token_endpoint: String,
        revocation_endpoint: Option<String>,
        client: McpOauthClient,
        tokens: McpOauthTokens,
    ) -> Self {
        Self {
            version: CREDENTIAL_VERSION,
            server_url,
            resource,
            issuer,
            token_endpoint,
            revocation_endpoint,
            client,
            tokens,
        }
    }

    /// True when the access token is expired or will be within `leeway_ms`.
    pub fn needs_refresh(&self, now_ms: i64, leeway_ms: i64) -> bool {
        self.tokens
            .expires_at_ms
            .is_some_and(|expires_at| expires_at - leeway_ms <= now_ms)
    }

    pub fn can_refresh(&self) -> bool {
        self.tokens.refresh_token.is_some()
    }

    pub fn authorization_header(&self) -> String {
        format!("Bearer {}", self.tokens.access_token)
    }

    pub fn to_secret(&self) -> Result<SecretValue, CredentialStoreError> {
        let bytes = serde_json::to_vec(self).map_err(|_| CredentialStoreError::OperationFailed)?;
        SecretValue::new(bytes)
    }

    pub fn from_secret(secret: &SecretValue) -> Result<Self, CredentialStoreError> {
        let credential: Self = serde_json::from_slice(secret.expose_secret())
            .map_err(|_| CredentialStoreError::CorruptEntry)?;
        if credential.version != CREDENTIAL_VERSION {
            return Err(CredentialStoreError::CorruptEntry);
        }
        Ok(credential)
    }
}

impl fmt::Debug for McpOauthCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpOauthCredential")
            .field("server_url", &self.server_url)
            .field("issuer", &self.issuer)
            .field("client", &self.client)
            .field("tokens", &self.tokens)
            .finish()
    }
}

impl fmt::Debug for McpOauthClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpOauthClient")
            .field("registration", &self.registration)
            .field("auth_method", &self.auth_method)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

impl fmt::Debug for McpOauthTokens {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpOauthTokens")
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(expires_at_ms: Option<i64>) -> McpOauthCredential {
        McpOauthCredential::new(
            "https://mcp.example.com/mcp".into(),
            "https://mcp.example.com/mcp".into(),
            "https://auth.example.com".into(),
            "https://auth.example.com/token".into(),
            None,
            McpOauthClient {
                client_id: "client".into(),
                client_secret: Some("client-secret-value".into()),
                auth_method: ClientAuthMethod::ClientSecretPost,
                registration: ClientRegistrationKind::UserProvided,
                redirect_uri: "http://127.0.0.1:46327/oauth/mcp/callback".into(),
            },
            McpOauthTokens {
                access_token: "access-token-value".into(),
                refresh_token: Some("refresh-token-value".into()),
                scope: None,
                expires_at_ms,
            },
        )
    }

    #[test]
    fn round_trips_through_a_secret() {
        let original = credential(Some(1_000));
        let secret = original.to_secret().expect("serialize");
        assert_eq!(
            McpOauthCredential::from_secret(&secret).expect("parse"),
            original
        );
    }

    #[test]
    fn debug_output_never_contains_secrets() {
        let rendered = format!("{:?}", credential(None));
        for secret in [
            "access-token-value",
            "refresh-token-value",
            "client-secret-value",
        ] {
            assert!(!rendered.contains(secret), "{rendered}");
        }
    }

    #[test]
    fn refreshes_inside_the_leeway_only() {
        assert!(credential(Some(10_000)).needs_refresh(9_500, 1_000));
        assert!(!credential(Some(10_000)).needs_refresh(8_000, 1_000));
        assert!(!credential(None).needs_refresh(i64::MAX - 1, 1_000));
    }
}
