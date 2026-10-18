//! End-to-end connector sign-in against the offline OAuth fixture: the
//! browser is simulated by following the authorization redirect by hand.

use std::sync::Arc;

use dcc_core::{
    domain::mcp::{
        McpDefinition, McpDefinitionId, McpDefinitionOwnership, McpTransport, McpTrust,
        McpTrustDecision, McpTrustFingerprint,
    },
    domain::provider::ProviderId,
    ports::McpRepo,
};
use dcc_infra::{
    credential_store::InMemoryCredentialStore, mcp_db::SqliteMcpRepo, mcp_oauth::McpOauthError,
};
use dcc_mcp_fixture::oauth::{OAuthFixture, OAuthFixtureConfig};
use dcc_tauri::mcp_connect::{
    load_credential, ConnectStart, ConnectStores, McpConnectCoordinator, DCC_OAUTH_GRANT_OWNER,
};

struct Harness {
    _dir: tempfile::TempDir,
    stores: ConnectStores,
    definition: McpDefinition,
    coordinator: McpConnectCoordinator,
    fixture: OAuthFixture,
}

async fn harness(config: OAuthFixtureConfig) -> Harness {
    let fixture = OAuthFixture::spawn(config).await.expect("fixture");
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = SqliteMcpRepo::open(dir.path().join("dcc.db")).expect("repo");
    let now = "2026-10-18T00:00:00Z".to_string();
    let mut definition = McpDefinition {
        id: McpDefinitionId("mcp-fixture".into()),
        display_name: "Fixture".into(),
        transport: McpTransport::Http {
            url: fixture.server_url(),
        },
        secret_refs: vec![],
        enabled: true,
        ownership: McpDefinitionOwnership::DccManaged,
        trust: McpTrust {
            current_fingerprint: McpTrustFingerprint("0".repeat(64)),
            decision: McpTrustDecision::Untrusted,
        },
        created_at: now.clone(),
        updated_at: now,
    };
    definition.synchronize_trust_fingerprint();
    repo.save_mcp_definition(&definition).await.expect("save");
    Harness {
        _dir: dir,
        stores: ConnectStores {
            repo: Arc::new(repo),
            credentials: Arc::new(InMemoryCredentialStore::default()),
        },
        definition,
        coordinator: McpConnectCoordinator::with_callback_port(0).expect("coordinator"),
        fixture,
    }
}

/// Plays the browser: the fixture auto-approves and redirects to DCC's
/// loopback callback, which answers with the result page.
async fn approve_in_browser(authorization_url: &str) -> String {
    let browser = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let redirect = browser
        .get(authorization_url)
        .send()
        .await
        .expect("authorize");
    assert_eq!(redirect.status(), 302, "authorization request was rejected");
    let location = redirect.headers()["location"].to_str().unwrap().to_string();
    browser
        .get(location)
        .send()
        .await
        .expect("callback")
        .text()
        .await
        .unwrap()
}

async fn connect(harness: &Harness) -> Result<(), McpOauthError> {
    let start = harness
        .coordinator
        .start(harness.stores.clone(), &harness.definition, None)
        .await?;
    let ConnectStart::Authorize {
        login_id,
        authorization_url,
    } = start
    else {
        panic!("fixture requires a sign-in");
    };
    approve_in_browser(&authorization_url).await;
    harness.coordinator.wait(&login_id).await
}

#[tokio::test]
async fn signs_in_stores_the_grant_and_refreshes_single_use_tokens() {
    let harness = harness(OAuthFixtureConfig::default()).await;
    connect(&harness).await.expect("connected");

    let credential = load_credential(&harness.stores, &harness.definition.id)
        .await
        .unwrap()
        .expect("grant stored");
    assert!(harness
        .fixture
        .accepts_access_token(&credential.tokens.access_token));
    assert_eq!(
        harness.fixture.last_resource(),
        Some(harness.fixture.server_url())
    );
    let grant = harness
        .stores
        .repo
        .get_mcp_oauth_grant(
            &harness.definition.id,
            &ProviderId(DCC_OAUTH_GRANT_OWNER.into()),
        )
        .await
        .unwrap()
        .expect("grant row");
    assert!(grant.secret_ref.0.starts_with("oauth-grant:dcc:"));

    // A fresh token is returned as is; a forced refresh rotates it twice in
    // a row, proving the new single-use refresh token was saved.
    let unchanged = harness
        .coordinator
        .access_credential(&harness.stores, &harness.definition, false)
        .await
        .unwrap();
    assert_eq!(
        unchanged.tokens.access_token,
        credential.tokens.access_token
    );
    for expected in 1..=2 {
        let refreshed = harness
            .coordinator
            .access_credential(&harness.stores, &harness.definition, true)
            .await
            .expect("refresh");
        assert!(harness
            .fixture
            .accepts_access_token(&refreshed.tokens.access_token));
        assert_eq!(harness.fixture.refreshes(), expected);
    }
}

#[tokio::test]
async fn reconnecting_on_the_fixed_port_reuses_the_registered_client() {
    let free_port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut harness = harness(OAuthFixtureConfig::default()).await;
    harness.coordinator = McpConnectCoordinator::with_callback_port(free_port).unwrap();
    connect(&harness).await.expect("connected");
    connect(&harness).await.expect("reconnected");
    assert_eq!(harness.fixture.registrations(), 1);
}

#[tokio::test]
async fn concurrent_refreshes_share_one_exchange() {
    let harness = harness(OAuthFixtureConfig::default()).await;
    connect(&harness).await.expect("connected");
    // The second caller waits for the first one's refresh and then finds a
    // fresh token, so the single-use refresh token is spent once.
    let (first, second) = tokio::join!(
        harness
            .coordinator
            .access_credential(&harness.stores, &harness.definition, true),
        harness
            .coordinator
            .access_credential(&harness.stores, &harness.definition, false),
    );
    assert!(first.is_ok() && second.is_ok(), "{first:?} {second:?}");
    assert_eq!(harness.fixture.refreshes(), 1);
}

#[tokio::test]
async fn finds_metadata_without_resource_metadata_in_the_challenge() {
    let harness = harness(OAuthFixtureConfig {
        challenge_has_resource_metadata: false,
        ..OAuthFixtureConfig::default()
    })
    .await;
    connect(&harness)
        .await
        .expect("connected via well-known fallback");
}

#[tokio::test]
async fn rejects_a_callback_from_another_issuer() {
    let harness = harness(OAuthFixtureConfig {
        wrong_iss: true,
        ..OAuthFixtureConfig::default()
    })
    .await;
    assert_eq!(connect(&harness).await, Err(McpOauthError::IssuerMismatch));
    assert!(load_credential(&harness.stores, &harness.definition.id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn rejects_a_missing_iss_when_the_server_promised_one() {
    let harness = harness(OAuthFixtureConfig {
        send_iss: false,
        ..OAuthFixtureConfig::default()
    })
    .await;
    assert_eq!(connect(&harness).await, Err(McpOauthError::IssuerMismatch));
}

#[tokio::test]
async fn asks_for_the_users_app_when_registration_is_unavailable() {
    let harness = harness(OAuthFixtureConfig {
        dynamic_registration: false,
        ..OAuthFixtureConfig::default()
    })
    .await;
    assert_eq!(
        connect(&harness).await,
        Err(McpOauthError::ClientRegistrationRequired)
    );

    let forbidden = self::harness(OAuthFixtureConfig {
        registration_forbidden: true,
        ..OAuthFixtureConfig::default()
    })
    .await;
    assert!(matches!(
        connect(&forbidden).await,
        Err(McpOauthError::ClientRegistrationRejected(_))
    ));
}

#[tokio::test]
async fn disconnect_revokes_and_forgets_the_grant() {
    let harness = harness(OAuthFixtureConfig::default()).await;
    connect(&harness).await.expect("connected");
    let credential = load_credential(&harness.stores, &harness.definition.id)
        .await
        .unwrap()
        .unwrap();

    assert!(harness
        .coordinator
        .disconnect(&harness.stores, &harness.definition.id)
        .await
        .unwrap());
    let revoked = harness.fixture.revoked();
    assert_eq!(
        revoked,
        vec![
            credential.tokens.refresh_token.clone().unwrap(),
            credential.tokens.access_token.clone()
        ]
    );
    assert!(!harness
        .fixture
        .accepts_access_token(&credential.tokens.access_token));
    assert!(load_credential(&harness.stores, &harness.definition.id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        harness
            .coordinator
            .access_credential(&harness.stores, &harness.definition, false)
            .await
            .unwrap_err(),
        McpOauthError::ReauthorizationRequired
    );
}

#[tokio::test]
async fn a_cancelled_sign_in_stores_nothing() {
    let harness = harness(OAuthFixtureConfig::default()).await;
    let ConnectStart::Authorize { login_id, .. } = harness
        .coordinator
        .start(harness.stores.clone(), &harness.definition, None)
        .await
        .unwrap()
    else {
        panic!("sign-in expected");
    };
    assert!(harness.coordinator.cancel(&login_id));
    assert_eq!(
        harness.coordinator.wait(&login_id).await,
        Err(McpOauthError::Cancelled)
    );
}

mod gateway {
    use std::sync::Mutex;

    use dcc_tauri::connector_gateway::{ConnectorCall, ConnectorGateway, ConnectorResponse};
    use serde_json::json;

    use super::*;

    fn gateway(harness: &Harness) -> (ConnectorGateway, Arc<Mutex<Vec<McpDefinitionId>>>) {
        let needs_auth = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&needs_auth);
        let gateway = ConnectorGateway::new(
            harness.coordinator.clone(),
            harness.stores.clone(),
            Arc::new(move |definition_id: &McpDefinitionId| {
                recorded.lock().unwrap().push(definition_id.clone());
            }),
        )
        .expect("gateway");
        (gateway, needs_auth)
    }

    fn tools_list() -> ConnectorCall {
        ConnectorCall {
            session_id: None,
            protocol_version: Some("2025-11-25".into()),
            body: json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        }
    }

    fn error_message(response: &ConnectorResponse) -> String {
        let ConnectorResponse::Json { body, .. } = response else {
            panic!("expected a JSON-RPC body, got {response:?}");
        };
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    #[tokio::test]
    async fn forwards_with_the_vendor_token() {
        let harness = harness(OAuthFixtureConfig::default()).await;
        connect(&harness).await.expect("connected");
        let (gateway, needs_auth) = gateway(&harness);
        let response = gateway.handle(&harness.definition.id, tools_list()).await;
        let ConnectorResponse::Json { body, .. } = response else {
            panic!("expected a reply");
        };
        assert_eq!(body["result"]["serverInfo"]["name"], "dcc-oauth-fixture");
        assert!(needs_auth.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn refreshes_once_when_the_server_rejects_the_token() {
        let harness = harness(OAuthFixtureConfig::default()).await;
        connect(&harness).await.expect("connected");
        harness.fixture.expire_access_tokens();
        let (gateway, needs_auth) = gateway(&harness);
        let response = gateway.handle(&harness.definition.id, tools_list()).await;
        assert!(
            matches!(response, ConnectorResponse::Json { ref body, .. } if body.get("result").is_some())
        );
        assert_eq!(harness.fixture.refreshes(), 1);
        assert!(needs_auth.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_revoked_sign_in_asks_for_reconnection_instead_of_401() {
        let harness = harness(OAuthFixtureConfig::default()).await;
        connect(&harness).await.expect("connected");
        harness.fixture.revoke_all_tokens();
        let (gateway, needs_auth) = gateway(&harness);
        let response = gateway.handle(&harness.definition.id, tools_list()).await;
        assert!(error_message(&response).contains("needs to be reconnected"));
        assert_eq!(
            *needs_auth.lock().unwrap(),
            vec![harness.definition.id.clone()]
        );
    }

    #[tokio::test]
    async fn disconnecting_cuts_running_sessions_off() {
        let harness = harness(OAuthFixtureConfig::default()).await;
        connect(&harness).await.expect("connected");
        harness
            .coordinator
            .disconnect(&harness.stores, &harness.definition.id)
            .await
            .unwrap();
        let (gateway, _) = gateway(&harness);
        let response = gateway.handle(&harness.definition.id, tools_list()).await;
        assert!(error_message(&response).contains("needs to be reconnected"));
    }

    #[tokio::test]
    async fn notifications_are_accepted_even_when_signed_out() {
        let harness = harness(OAuthFixtureConfig::default()).await;
        let (gateway, _) = gateway(&harness);
        let response = gateway
            .handle(
                &harness.definition.id,
                ConnectorCall {
                    session_id: None,
                    protocol_version: None,
                    body: json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
                },
            )
            .await;
        assert_eq!(response, ConnectorResponse::Accepted { session_id: None });
    }
}
