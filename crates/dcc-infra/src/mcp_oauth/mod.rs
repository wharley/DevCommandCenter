//! DCC-owned OAuth client for remote MCP connectors.
//!
//! DCC signs in once per connector, keeps the grant in the OS credential
//! store, and later serves the connector to every provider through its own
//! loopback gateway. Provider processes never run this flow and never see the
//! vendor token.
//!
//! The flow follows the MCP authorization spec (2025-11-25 plus the
//! 2026-07-28 revision): an unauthenticated probe, protected-resource and
//! authorization-server discovery, client identification (metadata document,
//! dynamic registration, or a user-provided app), PKCE S256 with `state` and
//! `resource`, and an `iss` check before the code is redeemed.

mod callback;
mod credential;
mod discovery;
mod flow;
mod http;
mod registration;

use std::time::Duration;

use thiserror::Error;

pub use callback::{
    bind_callback_listener, callback_page, parse_callback_request, serve_callback_connection,
    BoundCallbackListener, CallbackPage,
};
pub use credential::{
    ClientAuthMethod, ClientRegistrationKind, McpOauthClient, McpOauthCredential, McpOauthTokens,
};
pub use discovery::{
    parse_www_authenticate, AuthChallenge, AuthorizationServerMetadata, ProtectedResourceMetadata,
    ServerAuthRequirement,
};
pub use flow::{
    begin_authorization, complete_authorization, refresh_credential, revoke_credential,
    AuthorizationStart, BeginOptions, CallbackParams, PendingAuthorization,
};
pub use http::McpOauthHttp;
pub use registration::UserProvidedClient;

/// Fixed loopback port for sign-in callbacks. It is published in the client
/// metadata document, so changing it breaks every metadata-document client.
pub const DCC_OAUTH_CALLBACK_PORT: u16 = 46327;
pub const DCC_OAUTH_CALLBACK_PATH: &str = "/oauth/mcp/callback";
/// Client ID Metadata Document hosted on the DCC site. The URL is the
/// client_id, so it must be served without redirects.
pub const DCC_CLIENT_METADATA_URL: &str = "https://www.devcommandcenter.com/oauth/mcp-client.json";
/// The document above is live (site commit b9d27f4). Turning this off makes
/// DCC register dynamically even with servers that accept metadata
/// documents.
pub const DCC_CLIENT_METADATA_PUBLISHED: bool = true;
pub const DCC_CLIENT_NAME: &str = "Dev Command Center";
pub const DCC_CLIENT_URI: &str = "https://www.devcommandcenter.com";
/// A sign-in the user abandons in the browser expires after this long.
pub const AUTHORIZATION_TTL: Duration = Duration::from_secs(10 * 60);
/// Access tokens are refreshed this long before they expire.
pub const TOKEN_REFRESH_LEEWAY: Duration = Duration::from_secs(60);

pub fn callback_redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}{DCC_OAUTH_CALLBACK_PATH}")
}

/// Redirect for an app the user registers with the vendor (Slack). Slack
/// treats `http://localhost:<port>` as a desktop redirect once PKCE is on and
/// documents no `127.0.0.1` form, so user apps get the `localhost` spelling;
/// the callback listener answers on both loopback addresses.
pub fn user_app_redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}{DCC_OAUTH_CALLBACK_PATH}")
}

/// Errors carry short, user-safe text. They never include tokens, codes, or
/// response bodies beyond a bounded OAuth `error_description`.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum McpOauthError {
    #[error("No MCP server answered at this URL")]
    NotMcpServer,
    #[error("{0}")]
    Discovery(String),
    #[error("This server only signs in apps registered with it")]
    ClientRegistrationRequired,
    #[error("This server refused to register DCC: {0}")]
    ClientRegistrationRejected(String),
    #[error("The sign-in was declined: {0}")]
    AuthorizationDenied(String),
    #[error("The sign-in response did not match this request")]
    StateMismatch,
    #[error("The sign-in response came from an unexpected issuer")]
    IssuerMismatch,
    #[error("The server did not issue a token: {0}")]
    TokenExchange(String),
    /// The grant is gone (revoked, expired refresh token, deleted client).
    /// The connector needs a new sign-in.
    #[error("The sign-in expired. Reconnect to keep using it")]
    ReauthorizationRequired,
    /// Network failure, timeout, or a 5xx. Retrying later may work.
    #[error("{0}")]
    Transient(String),
    #[error("The sign-in timed out")]
    Timeout,
    #[error("The sign-in was cancelled")]
    Cancelled,
}

impl McpOauthError {
    pub fn needs_reauthorization(&self) -> bool {
        matches!(self, Self::ReauthorizationRequired)
    }
}

pub type McpOauthResult<T> = Result<T, McpOauthError>;

/// Keeps vendor-supplied text short and on one line before it reaches the UI.
fn bounded_text(value: &str) -> String {
    const MAX_CHARS: usize = 200;
    let single_line = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let trimmed = single_line.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        trimmed.to_string()
    } else {
        let mut shortened = trimmed.chars().take(MAX_CHARS).collect::<String>();
        shortened.push('…');
        shortened
    }
}
