//! Connector sign-in owned by DCC.
//!
//! One coordinator per app. It keeps the sign-ins waiting for their browser
//! callback, runs the loopback callback listener only while one is pending,
//! stores finished grants in the credential store (SQLite keeps only the
//! reference, under the `dcc` owner), and refreshes them one at a time per
//! connector because some vendors issue single-use refresh tokens.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex as StdMutex},
    time::Instant,
};

use chrono::Utc;
use dcc_core::{
    domain::{
        mcp::{
            mcp_oauth_resource_fingerprint, McpConnectionCheck, McpConnectionState, McpDefinition,
            McpDefinitionId, McpOauthGrant, McpSecretReferenceId, McpTransport,
        },
        provider::ProviderId,
    },
    ports::{CredentialStore, McpRepo},
};
use dcc_infra::mcp_oauth::{
    begin_authorization, bind_callback_listener, callback_redirect_uri, complete_authorization,
    refresh_credential, revoke_credential, serve_callback_connection, AuthorizationStart,
    BeginOptions, CallbackPage, CallbackParams, McpOauthCredential, McpOauthError, McpOauthHttp,
    McpOauthResult, PendingAuthorization, UserProvidedClient, AUTHORIZATION_TTL,
    DCC_CLIENT_METADATA_PUBLISHED, DCC_OAUTH_CALLBACK_PORT, TOKEN_REFRESH_LEEWAY,
};
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use uuid::Uuid;

/// Grant owner for sign-ins DCC performs itself. Grants under any other
/// owner come from the retired provider login and are deleted at startup.
pub const DCC_OAUTH_GRANT_OWNER: &str = "dcc";
const MAX_PENDING_SIGN_INS: usize = 16;

/// Repository and credential store used to persist grants.
#[derive(Clone)]
pub struct ConnectStores {
    pub repo: Arc<dyn McpRepo>,
    pub credentials: Arc<dyn CredentialStore>,
}

#[derive(Debug)]
pub enum ConnectStart {
    /// The server answered without a sign-in; nothing to store.
    NoAuthRequired,
    Authorize {
        login_id: String,
        authorization_url: String,
    },
}

struct PendingSignIn {
    login_id: String,
    definition_id: McpDefinitionId,
    authorization: PendingAuthorization,
    stores: ConnectStores,
    started: Instant,
    done: oneshot::Sender<McpOauthResult<McpDefinition>>,
}

struct RunningListener {
    port: u16,
    fixed_port: bool,
    shutdown: oneshot::Sender<()>,
}

#[derive(Default)]
struct Inner {
    /// Keyed by OAuth `state`.
    pending: HashMap<String, PendingSignIn>,
    waiters: HashMap<String, oneshot::Receiver<McpOauthResult<McpDefinition>>>,
    listener: Option<RunningListener>,
}

#[derive(Clone)]
pub struct McpConnectCoordinator {
    http: McpOauthHttp,
    preferred_port: u16,
    inner: Arc<StdMutex<Inner>>,
    refresh_locks: Arc<StdMutex<HashMap<McpDefinitionId, Arc<AsyncMutex<()>>>>>,
}

impl McpConnectCoordinator {
    pub fn new() -> McpOauthResult<Self> {
        Self::with_callback_port(DCC_OAUTH_CALLBACK_PORT)
    }

    /// Tests pass port 0 so parallel runs never fight over the fixed port.
    pub fn with_callback_port(preferred_port: u16) -> McpOauthResult<Self> {
        Ok(Self {
            http: McpOauthHttp::new()?,
            preferred_port,
            inner: Arc::new(StdMutex::new(Inner::default())),
            refresh_locks: Arc::new(StdMutex::new(HashMap::new())),
        })
    }

    /// Starts a sign-in. The caller opens `authorization_url` in the browser
    /// and then awaits [`Self::wait`].
    pub async fn start(
        &self,
        stores: ConnectStores,
        definition: &McpDefinition,
        user_client: Option<UserProvidedClient>,
    ) -> McpOauthResult<ConnectStart> {
        let McpTransport::Http { url } = &definition.transport else {
            return Err(McpOauthError::NotMcpServer);
        };
        self.cancel_definition(&definition.id);
        if self.lock().pending.len() >= MAX_PENDING_SIGN_INS {
            return Err(McpOauthError::Transient(
                "Too many sign-ins are waiting. Finish or cancel one first".to_string(),
            ));
        }
        let (port, fixed_port) = self.ensure_listener().await?;
        let previous_client = load_credential(&stores, &definition.id)
            .await
            .ok()
            .flatten()
            .map(|credential| credential.client);
        let scopes = stores
            .repo
            .list_mcp_connector_details()
            .await
            .ok()
            .and_then(|details| {
                details
                    .into_iter()
                    .find(|details| details.definition_id == definition.id)
            })
            .and_then(|details| details.scopes);
        let started = begin_authorization(
            &self.http,
            url,
            BeginOptions {
                redirect_uri: callback_redirect_uri(port),
                metadata_document_allowed: fixed_port && DCC_CLIENT_METADATA_PUBLISHED,
                user_client,
                previous_client,
                scopes,
            },
        )
        .await;
        let authorization = match started {
            Ok(AuthorizationStart::Authorize(authorization)) => *authorization,
            Ok(AuthorizationStart::NoAuthRequired) => {
                self.stop_listener_if_idle();
                return Ok(ConnectStart::NoAuthRequired);
            }
            Err(error) => {
                self.stop_listener_if_idle();
                return Err(error);
            }
        };

        let login_id = Uuid::new_v4().to_string();
        let authorization_url = authorization.authorization_url.to_string();
        let (done, receiver) = oneshot::channel();
        {
            let mut inner = self.lock();
            inner.waiters.insert(login_id.clone(), receiver);
            inner.pending.insert(
                authorization.state.clone(),
                PendingSignIn {
                    login_id: login_id.clone(),
                    definition_id: definition.id.clone(),
                    authorization,
                    stores,
                    started: Instant::now(),
                    done,
                },
            );
        }
        self.expire_after_ttl(login_id.clone());
        Ok(ConnectStart::Authorize {
            login_id,
            authorization_url,
        })
    }

    /// Waits until the browser comes back, the sign-in is cancelled, or it
    /// times out.
    /// Returns the connected definition once its grant is stored.
    pub async fn wait(&self, login_id: &str) -> McpOauthResult<McpDefinition> {
        let Some(receiver) = self.lock().waiters.remove(login_id) else {
            return Err(McpOauthError::Cancelled);
        };
        match tokio::time::timeout(AUTHORIZATION_TTL, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(McpOauthError::Cancelled),
            Err(_) => {
                self.cancel(login_id);
                Err(McpOauthError::Timeout)
            }
        }
    }

    pub fn cancel(&self, login_id: &str) -> bool {
        let removed = {
            let mut inner = self.lock();
            let state = inner
                .pending
                .iter()
                .find(|(_, pending)| pending.login_id == login_id)
                .map(|(state, _)| state.clone());
            state.and_then(|state| inner.pending.remove(&state))
        };
        let cancelled = removed.is_some();
        if let Some(pending) = removed {
            let _ = pending.done.send(Err(McpOauthError::Cancelled));
        }
        self.stop_listener_if_idle();
        cancelled
    }

    fn cancel_definition(&self, definition_id: &McpDefinitionId) {
        let login_id = self
            .lock()
            .pending
            .values()
            .find(|pending| &pending.definition_id == definition_id)
            .map(|pending| pending.login_id.clone());
        if let Some(login_id) = login_id {
            self.cancel(&login_id);
        }
    }

    /// Revokes the grant with the vendor when possible and deletes it here.
    pub async fn disconnect(
        &self,
        stores: &ConnectStores,
        definition_id: &McpDefinitionId,
    ) -> McpOauthResult<bool> {
        self.cancel_definition(definition_id);
        let lock = self.refresh_lock(definition_id);
        let _guard = lock.lock().await;
        let owner = ProviderId(DCC_OAUTH_GRANT_OWNER.to_string());
        let grant = stores
            .repo
            .get_mcp_oauth_grant(definition_id, &owner)
            .await
            .map_err(storage_error)?;
        let Some(grant) = grant else {
            return Ok(false);
        };
        if let Ok(Some(credential)) = load_credential(stores, definition_id).await {
            revoke_credential(&self.http, &credential).await;
        }
        stores
            .credentials
            .delete_secret(&grant.secret_ref)
            .await
            .map_err(|_| storage_error(()))?;
        stores
            .repo
            .delete_mcp_oauth_grant(definition_id, &owner)
            .await
            .map_err(storage_error)?;
        let _ = stores.repo.delete_mcp_connection_check(definition_id).await;
        Ok(true)
    }

    /// Returns a credential whose access token is valid now, refreshing it
    /// first when it expires within the leeway. `force` refreshes regardless,
    /// for a 401 from the server. Concurrent callers share one refresh.
    pub async fn access_credential(
        &self,
        stores: &ConnectStores,
        definition: &McpDefinition,
        force: bool,
    ) -> McpOauthResult<McpOauthCredential> {
        let lock = self.refresh_lock(&definition.id);
        let _guard = lock.lock().await;
        let credential = load_credential(stores, &definition.id)
            .await?
            .ok_or(McpOauthError::ReauthorizationRequired)?;
        let now = Utc::now().timestamp_millis();
        let leeway = TOKEN_REFRESH_LEEWAY.as_millis() as i64;
        if !force && !credential.needs_refresh(now, leeway) {
            return Ok(credential);
        }
        let refreshed = refresh_credential(&self.http, &credential, now).await?;
        // The vendor may have consumed the old refresh token already, so a
        // failed save still returns the new pair to the caller.
        let _ = save_credential(stores, definition, &refreshed).await;
        Ok(refreshed)
    }

    fn refresh_lock(&self, definition_id: &McpDefinitionId) -> Arc<AsyncMutex<()>> {
        self.refresh_locks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(definition_id.clone())
            .or_default()
            .clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    async fn ensure_listener(&self) -> McpOauthResult<(u16, bool)> {
        if let Some(listener) = &self.lock().listener {
            return Ok((listener.port, listener.fixed_port));
        }
        let bound = bind_callback_listener(self.preferred_port)
            .await
            .map_err(|_| McpOauthError::Transient("Could not open the sign-in callback".into()))?;
        let (port, fixed_port) = (bound.port, bound.fixed_port && self.preferred_port != 0);
        let (shutdown, mut shutdown_signal) = oneshot::channel::<()>();
        {
            let mut inner = self.lock();
            if let Some(existing) = &inner.listener {
                // Another start won the race; drop ours.
                return Ok((existing.port, existing.fixed_port));
            }
            inner.listener = Some(RunningListener {
                port,
                fixed_port,
                shutdown,
            });
        }
        let coordinator = self.clone();
        let listener = bound.listener;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_signal => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { continue };
                        let coordinator = coordinator.clone();
                        tokio::spawn(async move {
                            serve_callback_connection(stream, |params| {
                                coordinator.handle_callback(params)
                            })
                            .await;
                        });
                    }
                }
            }
        });
        Ok((port, fixed_port))
    }

    fn stop_listener_if_idle(&self) {
        let mut inner = self.lock();
        if inner.pending.is_empty() {
            if let Some(listener) = inner.listener.take() {
                let _ = listener.shutdown.send(());
            }
        }
    }

    fn handle_callback(&self, params: CallbackParams) -> CallbackPage {
        let pending = params
            .state
            .as_ref()
            .and_then(|state| self.lock().pending.remove(state));
        let Some(pending) = pending else {
            // Unknown or already used state: never redeem anything.
            return CallbackPage::Failed;
        };
        let page = if params.error.is_some() {
            CallbackPage::Failed
        } else {
            CallbackPage::Done
        };
        let coordinator = self.clone();
        tokio::spawn(async move {
            let PendingSignIn {
                definition_id,
                authorization,
                stores,
                done,
                ..
            } = pending;
            let now = Utc::now().timestamp_millis();
            let result = async {
                let credential =
                    complete_authorization(&coordinator.http, authorization, params, now).await?;
                let definition = stores
                    .repo
                    .get_mcp_definition(&definition_id)
                    .await
                    .map_err(storage_error)?
                    .ok_or(McpOauthError::Cancelled)?;
                save_credential(&stores, &definition, &credential).await?;
                forget_provider_grants(&stores, &definition.id).await;
                Ok(definition)
            }
            .await;
            let _ = done.send(result);
            coordinator.stop_listener_if_idle();
        });
        page
    }

    fn expire_after_ttl(&self, login_id: String) {
        let coordinator = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(AUTHORIZATION_TTL).await;
            let expired = {
                let mut inner = coordinator.lock();
                let state = inner
                    .pending
                    .iter()
                    .find(|(_, pending)| {
                        pending.login_id == login_id
                            && pending.started.elapsed() >= AUTHORIZATION_TTL
                    })
                    .map(|(state, _)| state.clone());
                state.and_then(|state| inner.pending.remove(&state))
            };
            if let Some(pending) = expired {
                let _ = pending.done.send(Err(McpOauthError::Timeout));
            }
            coordinator.stop_listener_if_idle();
        });
    }

    #[cfg(test)]
    fn listener_port(&self) -> Option<u16> {
        self.lock().listener.as_ref().map(|listener| listener.port)
    }
}

/// Once DCC holds the sign-in, grants a provider adapter kept for the same
/// connector (the old in-turn mcp-remote path) are deleted, so only one
/// sign-in exists per connector. Best effort.
async fn forget_provider_grants(stores: &ConnectStores, definition_id: &McpDefinitionId) {
    let Ok(grants) = stores.repo.list_mcp_oauth_grants(Some(definition_id)).await else {
        return;
    };
    for grant in grants
        .into_iter()
        .filter(|grant| grant.provider_id.0 != DCC_OAUTH_GRANT_OWNER)
    {
        let _ = stores.credentials.delete_secret(&grant.secret_ref).await;
        let _ = stores
            .repo
            .delete_mcp_oauth_grant(definition_id, &grant.provider_id)
            .await;
    }
}

/// Retires the sign-ins the agents' own MCP login (mcp-remote) used to
/// store. Nothing reads them since DCC signs connectors in itself, so a
/// connector that only had one shows Reconnect instead of failing inside the
/// agent. Runs at startup; once those grants are gone it does nothing.
pub async fn retire_provider_grants(stores: &ConnectStores) -> usize {
    let Ok(grants) = stores.repo.list_mcp_oauth_grants(None).await else {
        return 0;
    };
    let signed_in_by_dcc = grants
        .iter()
        .filter(|grant| grant.provider_id.0 == DCC_OAUTH_GRANT_OWNER)
        .map(|grant| grant.definition_id.clone())
        .collect::<HashSet<_>>();
    let mut retired = 0;
    let mut needs_auth = HashSet::new();
    for grant in grants
        .into_iter()
        .filter(|grant| grant.provider_id.0 != DCC_OAUTH_GRANT_OWNER)
    {
        if stores
            .credentials
            .delete_secret(&grant.secret_ref)
            .await
            .is_err()
            || stores
                .repo
                .delete_mcp_oauth_grant(&grant.definition_id, &grant.provider_id)
                .await
                .is_err()
        {
            continue;
        }
        retired += 1;
        if !signed_in_by_dcc.contains(&grant.definition_id) {
            needs_auth.insert(grant.definition_id);
        }
    }
    if needs_auth.is_empty() {
        return retired;
    }
    let previous = stores
        .repo
        .list_mcp_connection_checks()
        .await
        .unwrap_or_default();
    for definition_id in needs_auth {
        let tools = previous
            .iter()
            .find(|check| check.definition_id == definition_id)
            .map(|check| check.tools.clone())
            .unwrap_or_default();
        let _ = stores
            .repo
            .save_mcp_connection_check(&McpConnectionCheck {
                definition_id,
                state: McpConnectionState::NeedsAuth,
                tools,
                message: Some(
                    "The sign-in is missing or expired. Reconnect to keep using it".to_string(),
                ),
                checked_at: Utc::now().to_rfc3339(),
            })
            .await;
    }
    retired
}

pub fn dcc_grant_secret_reference(definition_id: &McpDefinitionId) -> McpSecretReferenceId {
    McpSecretReferenceId(format!(
        "oauth-grant:{DCC_OAUTH_GRANT_OWNER}:{}",
        definition_id.0
    ))
}

pub async fn load_credential(
    stores: &ConnectStores,
    definition_id: &McpDefinitionId,
) -> McpOauthResult<Option<McpOauthCredential>> {
    let owner = ProviderId(DCC_OAUTH_GRANT_OWNER.to_string());
    let Some(grant) = stores
        .repo
        .get_mcp_oauth_grant(definition_id, &owner)
        .await
        .map_err(storage_error)?
    else {
        return Ok(None);
    };
    let Some(secret) = stores
        .credentials
        .resolve_secret(&grant.secret_ref)
        .await
        .map_err(|_| storage_error(()))?
    else {
        return Ok(None);
    };
    McpOauthCredential::from_secret(&secret)
        .map(Some)
        .map_err(|_| McpOauthError::ReauthorizationRequired)
}

async fn save_credential(
    stores: &ConnectStores,
    definition: &McpDefinition,
    credential: &McpOauthCredential,
) -> McpOauthResult<()> {
    let resource_fingerprint =
        mcp_oauth_resource_fingerprint(definition).map_err(|_| McpOauthError::NotMcpServer)?;
    let owner = ProviderId(DCC_OAUTH_GRANT_OWNER.to_string());
    let secret_ref = dcc_grant_secret_reference(&definition.id);
    let secret = credential.to_secret().map_err(|_| storage_error(()))?;
    stores
        .credentials
        .store_secret(&secret_ref, secret)
        .await
        .map_err(|_| storage_error(()))?;
    let now = Utc::now().to_rfc3339();
    let created_at = stores
        .repo
        .get_mcp_oauth_grant(&definition.id, &owner)
        .await
        .map_err(storage_error)?
        .filter(|grant| grant.resource_fingerprint == resource_fingerprint)
        .map(|grant| grant.created_at)
        .unwrap_or_else(|| now.clone());
    stores
        .repo
        .save_mcp_oauth_grant(&McpOauthGrant {
            definition_id: definition.id.clone(),
            provider_id: owner,
            resource_fingerprint,
            secret_ref,
            created_at,
            updated_at: now,
        })
        .await
        .map_err(storage_error)
}

fn storage_error<E>(_: E) -> McpOauthError {
    McpOauthError::Transient("Could not save the sign-in on this computer".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelling_the_last_sign_in_closes_the_listener() {
        let coordinator = McpConnectCoordinator::with_callback_port(0).expect("coordinator");
        let (port, fixed) = coordinator.ensure_listener().await.expect("listener");
        assert!(!fixed, "port 0 never counts as the published fixed port");
        assert_eq!(coordinator.listener_port(), Some(port));
        assert!(!coordinator.cancel("unknown"));
        assert_eq!(coordinator.listener_port(), None);
    }

    #[tokio::test]
    async fn unknown_state_is_never_redeemed() {
        let coordinator = McpConnectCoordinator::with_callback_port(0).expect("coordinator");
        let page = coordinator.handle_callback(CallbackParams {
            code: Some("code".into()),
            state: Some("forged".into()),
            ..CallbackParams::default()
        });
        assert_eq!(page, CallbackPage::Failed);
    }

    #[tokio::test]
    async fn provider_sign_ins_are_retired_and_ask_to_reconnect() {
        use dcc_core::domain::mcp::{
            McpDefinitionOwnership, McpOauthResourceFingerprint, McpToolAnnotations,
            McpToolSummary, McpTrust, McpTrustDecision, McpTrustFingerprint,
        };
        use dcc_core::ports::SecretValue;
        use dcc_infra::{credential_store::InMemoryCredentialStore, mcp_db::SqliteMcpRepo};

        let repo = SqliteMcpRepo::from_connection(Arc::new(StdMutex::new(
            rusqlite::Connection::open_in_memory().expect("open sqlite"),
        )))
        .expect("MCP repo");
        let credentials = InMemoryCredentialStore::default();
        for id in ["legacy", "moved"] {
            let mut definition = McpDefinition {
                id: McpDefinitionId(id.to_string()),
                display_name: id.to_string(),
                transport: McpTransport::Http {
                    url: format!("https://{id}.example.com/mcp"),
                },
                secret_refs: vec![],
                enabled: true,
                ownership: McpDefinitionOwnership::DccManaged,
                trust: McpTrust {
                    current_fingerprint: McpTrustFingerprint("0".repeat(64)),
                    decision: McpTrustDecision::Untrusted,
                },
                created_at: "2026-10-20T00:00:00Z".to_string(),
                updated_at: "2026-10-20T00:00:00Z".to_string(),
            };
            definition.synchronize_trust_fingerprint();
            repo.save_mcp_definition(&definition)
                .await
                .expect("save definition");
        }
        let grant = |definition: &str, owner: &str| McpOauthGrant {
            definition_id: McpDefinitionId(definition.to_string()),
            provider_id: ProviderId(owner.to_string()),
            resource_fingerprint: McpOauthResourceFingerprint("a".repeat(64)),
            secret_ref: McpSecretReferenceId(format!("oauth-grant:{owner}:{definition}")),
            created_at: "2026-10-20T00:00:00Z".to_string(),
            updated_at: "2026-10-20T00:00:00Z".to_string(),
        };
        for grant in [
            grant("legacy", "claude_code"),
            grant("moved", "codex"),
            grant("moved", DCC_OAUTH_GRANT_OWNER),
        ] {
            credentials
                .store_secret(
                    &grant.secret_ref,
                    SecretValue::new(b"state".to_vec()).expect("secret"),
                )
                .await
                .expect("store secret");
            repo.save_mcp_oauth_grant(&grant).await.expect("save grant");
        }
        let tool = McpToolSummary {
            name: "get_task".to_string(),
            annotations: McpToolAnnotations::default(),
        };
        repo.save_mcp_connection_check(&McpConnectionCheck {
            definition_id: McpDefinitionId("legacy".to_string()),
            state: McpConnectionState::Connected,
            tools: vec![tool.clone()],
            message: None,
            checked_at: "2026-10-20T00:00:00Z".to_string(),
        })
        .await
        .expect("save check");
        let stores = ConnectStores {
            repo: Arc::new(repo),
            credentials: Arc::new(credentials.clone()),
        };

        assert_eq!(retire_provider_grants(&stores).await, 2);
        let left = stores
            .repo
            .list_mcp_oauth_grants(None)
            .await
            .expect("grants");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].provider_id.0, DCC_OAUTH_GRANT_OWNER);
        assert!(credentials
            .resolve_secret(&grant("legacy", "claude_code").secret_ref)
            .await
            .expect("resolve")
            .is_none());
        let checks = stores
            .repo
            .list_mcp_connection_checks()
            .await
            .expect("checks");
        assert_eq!(checks.len(), 1, "a connector DCC holds keeps its state");
        assert_eq!(checks[0].definition_id.0, "legacy");
        assert_eq!(checks[0].state, McpConnectionState::NeedsAuth);
        assert_eq!(checks[0].tools, vec![tool]);

        assert_eq!(retire_provider_grants(&stores).await, 0);
    }
}
