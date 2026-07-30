use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use dcc_core::{
    application::{
        prepare_session_for_turn, resolve_session_mcp_servers,
        send_turn_selection_differs_from_session, ResolveSessionMcpInput, SendTurnInput,
        StartThreadInput,
    },
    domain::{
        delegation::{Delegation, DelegationId, DelegationStatus},
        mcp::{
            mcp_oauth_resource_fingerprint, McpDefinitionId, McpErrorCategory, McpOauthGrant,
            McpRuntimeError, McpRuntimeState, McpRuntimeStatus, McpSecretReferenceId, McpTransport,
        },
        project::{Project, ProjectId},
        provider::{McpOauthSupport, ProviderEvent, ProviderId, SessionHandle},
        repository::{Repository, RepositoryId},
        session::{
            Session, SessionEventKind, SessionEventRecord, SessionId, SessionSearchResult, TurnId,
            WorkspaceSessionSummary,
        },
        thread::{Thread, ThreadId},
        workspace::{Workspace, WorkspaceId},
        workspace_bundle::WorkspaceBundleState,
    },
    ports::{
        CredentialStore, DelegationRepo, EventBus, Input, McpRepo, ProjectRepo, Provider,
        ProviderMcpOauthStart, ProviderMcpServerConfig, ProviderRuntimeConfig, RepositoryRepo,
        SessionConfig, SessionEventRepo, SessionRepo, ThreadRepo, WorkspaceBundleRepo,
        WorkspaceRepo,
    },
    Result,
};
use dcc_infra::{
    credential_store::SystemCredentialStore,
    db::{SqliteSessionRepo, SqliteWorkspaceRepo},
    mcp_db::SqliteMcpRepo,
};

use crate::delivery_failure::{
    WorkspaceDeliveryFailureOperation, WorkspaceDeliveryFailureSnapshot,
};
use crate::events::TauriEventBus;
use dcc_providers::provider_runtime;

const DELIVERY_FAILURE_WORKSPACE_LIMIT: usize = 64;

type DeliveryFailureStore =
    HashMap<String, HashMap<WorkspaceDeliveryFailureOperation, WorkspaceDeliveryFailureSnapshot>>;

#[derive(Clone, Debug)]
pub struct WorkspaceCommandState {
    pub db_path: PathBuf,
    delivery_failures: Arc<Mutex<DeliveryFailureStore>>,
}

impl WorkspaceCommandState {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            delivery_failures: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn record_delivery_failure(
        &self,
        snapshot: WorkspaceDeliveryFailureSnapshot,
    ) -> WorkspaceDeliveryFailureSnapshot {
        let Ok(mut store) = self.delivery_failures.lock() else {
            return snapshot;
        };
        let root = snapshot.workspace_root.clone();
        if let Some(existing) = store
            .get(&root)
            .and_then(|operations| operations.get(&snapshot.operation))
        {
            let same_failure = existing.branch == snapshot.branch
                && existing.head_sha == snapshot.head_sha
                && existing.classification == snapshot.classification
                && existing.remote == snapshot.remote
                && existing.operation_target == snapshot.operation_target
                && existing.push_target == snapshot.push_target
                && existing.output == snapshot.output
                && existing.changed_files == snapshot.changed_files
                && existing.external_url == snapshot.external_url
                && existing.available_actions == snapshot.available_actions;
            if same_failure {
                return existing.clone();
            }
        }

        if !store.contains_key(&root) && store.len() >= DELIVERY_FAILURE_WORKSPACE_LIMIT {
            let oldest_root = store
                .iter()
                .filter_map(|(candidate_root, operations)| {
                    operations
                        .values()
                        .map(|failure| failure.created_at.as_str())
                        .max()
                        .map(|latest| (candidate_root.clone(), latest.to_string()))
                })
                .min_by(|left, right| left.1.cmp(&right.1))
                .map(|(candidate_root, _)| candidate_root);
            if let Some(oldest_root) = oldest_root {
                store.remove(&oldest_root);
            }
        }

        store
            .entry(root)
            .or_default()
            .insert(snapshot.operation, snapshot.clone());
        snapshot
    }

    pub(crate) fn clear_delivery_failure(
        &self,
        workspace_root: &str,
        operation: WorkspaceDeliveryFailureOperation,
    ) {
        let Ok(mut store) = self.delivery_failures.lock() else {
            return;
        };
        let root = workspace_root.trim();
        let remove_root = store
            .get_mut(root)
            .map(|operations| {
                operations.remove(&operation);
                operations.is_empty()
            })
            .unwrap_or(false);
        if remove_root {
            store.remove(root);
        }
    }

    pub(crate) fn clear_delivery_failures(&self, workspace_root: &str) {
        let Ok(mut store) = self.delivery_failures.lock() else {
            return;
        };
        store.remove(workspace_root.trim());
    }

    pub(crate) fn has_delivery_failure(&self, workspace_root: &str) -> bool {
        self.delivery_failures
            .lock()
            .ok()
            .and_then(|store| {
                store
                    .get(workspace_root.trim())
                    .map(|operations| !operations.is_empty())
            })
            .unwrap_or(false)
    }

    pub(crate) fn latest_delivery_failure(
        &self,
        workspace_root: &str,
        branch: Option<&str>,
        head_sha: Option<&str>,
    ) -> Option<WorkspaceDeliveryFailureSnapshot> {
        let store = self.delivery_failures.lock().ok()?;
        store
            .get(workspace_root.trim())?
            .values()
            .filter(|failure| {
                failure.branch.as_deref() == branch && failure.head_sha.as_deref() == head_sha
            })
            .max_by(|left, right| left.created_at.cmp(&right.created_at))
            .cloned()
    }
}

#[derive(Clone)]
pub struct SessionCommandState {
    app_data_dir: PathBuf,
    db_path: PathBuf,
    session_repo: SqliteSessionRepo,
    event_bus: Arc<dyn EventBus>,
    store: Arc<Mutex<SessionStore>>,
}

#[derive(Clone, Debug)]
struct ProviderSessionBinding {
    provider_id: String,
    handle: SessionHandle,
    current_turn_id: Arc<AsyncMutex<Option<String>>>,
    projected_mcp_definition_ids: Arc<HashSet<McpDefinitionId>>,
}

#[derive(Default, Debug)]
struct SessionStore {
    provider_sessions: HashMap<SessionId, ProviderSessionBinding>,
    mcp_runtime_statuses: HashMap<SessionId, Vec<McpRuntimeStatus>>,
}

#[derive(Clone, Copy, Debug, Default)]
struct NoopEventBus;

#[async_trait]
impl EventBus for NoopEventBus {
    async fn publish(&self, _event: dcc_core::ports::events::CoreEvent) -> Result<()> {
        Ok(())
    }
}

impl SessionCommandState {
    pub fn new(app: AppHandle, db_path: PathBuf) -> Self {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        Self::from_parts(db_path, app_data_dir, Arc::new(TauriEventBus::new(app)))
    }

    pub fn new_headless(db_path: PathBuf, app_data_dir: PathBuf) -> Self {
        Self::from_parts(db_path, app_data_dir, Arc::new(NoopEventBus))
    }

    pub fn new_with_event_bus(
        db_path: PathBuf,
        app_data_dir: PathBuf,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self::from_parts(db_path, app_data_dir, event_bus)
    }

    fn from_parts(db_path: PathBuf, app_data_dir: PathBuf, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            app_data_dir,
            session_repo: SqliteSessionRepo::open(&db_path)
                .expect("failed to open sqlite session repo"),
            db_path,
            event_bus,
            store: Arc::new(Mutex::new(SessionStore::default())),
        }
    }

    fn lock_store(&self) -> Result<std::sync::MutexGuard<'_, SessionStore>> {
        self.store
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    fn provider_binding(&self, session_id: &SessionId) -> Result<Option<ProviderSessionBinding>> {
        let store = self.lock_store()?;
        Ok(store.provider_sessions.get(session_id).cloned())
    }

    fn provider_runtime_config(
        &self,
        provider_id: &str,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<ProviderRuntimeConfig> {
        let runtime = runtime.cloned().unwrap_or_default();
        if self.is_legacy_managed_provider_home(provider_id, &runtime) {
            return Ok(ProviderRuntimeConfig::default());
        }
        Ok(runtime)
    }

    fn provider_home_root(&self) -> PathBuf {
        self.app_data_dir.join("provider-homes")
    }

    fn is_legacy_managed_provider_home(
        &self,
        provider_id: &str,
        runtime: &ProviderRuntimeConfig,
    ) -> bool {
        if !matches!(provider_id, "claude_code" | "gemini" | "grok") {
            return false;
        }

        if runtime.shadow_home_path.is_some() {
            return false;
        }

        let Some(home_path) = runtime.home_path.as_deref() else {
            return false;
        };

        PathBuf::from(home_path) == self.provider_home_root().join(provider_id)
    }

    pub async fn peek_session(&self, session_id: &SessionId) -> Result<Option<Session>> {
        SessionRepo::get_session(&self.session_repo, session_id).await
    }

    pub fn list_workspace_sessions(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<WorkspaceSessionSummary>> {
        self.session_repo.list_workspace_sessions(workspace_id)
    }

    pub fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSearchResult>> {
        self.session_repo.search_sessions(query, limit)
    }

    async fn append_session_event(
        &self,
        session_id: &SessionId,
        kind: SessionEventKind,
    ) -> Result<SessionEventRecord> {
        let events =
            SessionEventRepo::list_events_by_session(&self.session_repo, session_id).await?;
        let sequence = events.last().map(|event| event.sequence + 1).unwrap_or(1);
        let record = SessionEventRecord {
            event_id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            sequence,
            occurred_at: Utc::now().to_rfc3339(),
            kind,
        };
        SessionEventRepo::append_event(&self.session_repo, &record).await?;
        Ok(record)
    }

    async fn append_and_publish_session_event(
        &self,
        session_id: &SessionId,
        kind: SessionEventKind,
        core_event: dcc_core::ports::events::CoreEvent,
    ) -> Result<()> {
        self.append_session_event(session_id, kind).await?;
        self.publish(core_event).await?;
        Ok(())
    }

    pub async fn emit_turn_aborted(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        reason: Option<String>,
    ) -> Result<()> {
        self.append_and_publish_session_event(
            session_id,
            SessionEventKind::TurnAborted {
                turn_id: turn_id.clone(),
                reason: reason.clone(),
            },
            dcc_core::ports::events::CoreEvent::SessionTurnAborted {
                session_id: session_id.0.clone(),
                turn_id: turn_id.0.clone(),
                reason,
            },
        )
        .await
    }

    pub async fn attach_provider_session(&self, session: &Session) -> Result<()> {
        if self.provider_binding(&session.id)?.is_some() {
            return Ok(());
        }

        let provider = provider_runtime(&session.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                session.provider_id
            ))
        })?;
        let (working_directory, additional_working_directories) = self
            .resolve_session_working_directories(
                session,
                provider.capabilities().supports_multi_root,
            )
            .await?;
        let provider_runtime =
            self.provider_runtime_config(&session.provider_id, session.provider_runtime.as_ref())?;
        let mcp_projection_version = provider.dcc_mcp_projection_version().map(str::to_string);
        let mcp_servers = self
            .resolve_provider_mcp_servers(session, provider.as_ref())
            .await?;
        let projected_definition_ids = mcp_servers
            .iter()
            .map(|server| server.definition_id.clone())
            .collect::<Vec<_>>();

        let handle = match provider
            .prepare_session(SessionConfig {
                workspace_id: session.workspace_id.clone(),
                session_id: session.id.clone(),
                model: session.model.clone(),
                working_directory: Some(working_directory),
                additional_working_directories,
                provider_runtime: Some(provider_runtime),
                mcp_servers,
            })
            .await
        {
            Ok(handle) => handle,
            Err(error) => {
                if let Some(provider_version) = mcp_projection_version.as_deref() {
                    if !projected_definition_ids.is_empty() {
                        let checked_at = Utc::now().to_rfc3339();
                        let statuses = projected_definition_ids
                            .iter()
                            .cloned()
                            .map(|definition_id| McpRuntimeStatus {
                                definition_id,
                                provider_id: ProviderId(session.provider_id.clone()),
                                provider_version: provider_version.to_string(),
                                session_id: session.id.clone(),
                                state: McpRuntimeState::Failed,
                                tools: Vec::new(),
                                checked_at: checked_at.clone(),
                                bounded_error: Some(McpRuntimeError::bounded(
                                    McpErrorCategory::Protocol,
                                    format!(
                                        "MCP bridge contract negotiation failed for {provider_version}"
                                    ),
                                )),
                            })
                            .collect();
                        let _ = self
                            .replace_mcp_runtime_statuses(
                                &session.id,
                                &session.provider_id,
                                provider_version,
                                statuses,
                            )
                            .await;
                    }
                }
                return Err(error);
            }
        };

        let binding = ProviderSessionBinding {
            provider_id: session.provider_id.clone(),
            handle: handle.clone(),
            current_turn_id: Arc::new(AsyncMutex::new(None)),
            projected_mcp_definition_ids: Arc::new(
                projected_definition_ids.iter().cloned().collect(),
            ),
        };

        {
            let mut store = self.lock_store()?;
            store
                .provider_sessions
                .insert(session.id.clone(), binding.clone());
        }

        if let Some(provider_version) = mcp_projection_version {
            let checked_at = Utc::now().to_rfc3339();
            let statuses = projected_definition_ids
                .into_iter()
                .map(|definition_id| McpRuntimeStatus {
                    definition_id,
                    provider_id: ProviderId(session.provider_id.clone()),
                    provider_version: provider_version.clone(),
                    session_id: session.id.clone(),
                    state: McpRuntimeState::AttachingProvider,
                    tools: Vec::new(),
                    checked_at: checked_at.clone(),
                    bounded_error: None,
                })
                .collect();
            let _ = self
                .replace_mcp_runtime_statuses(
                    &session.id,
                    &session.provider_id,
                    &provider_version,
                    statuses,
                )
                .await;
        }

        self.spawn_provider_bridge(session.id.clone(), binding, provider)
            .await;
        Ok(())
    }

    /// Applies provider/model selection and attaches its runtime before a turn
    /// is recorded. This keeps OAuth and MCP startup failures outside durable
    /// user-turn history.
    pub async fn prepare_provider_session_for_turn(
        &self,
        input: &SendTurnInput,
    ) -> Result<Session> {
        let current = self
            .peek_session(&input.session_id)
            .await?
            .ok_or_else(|| dcc_core::CoreError::Repository("session not found".to_string()))?;
        if send_turn_selection_differs_from_session(&current, input) {
            let _ = self.cancel_provider_session(&input.session_id).await;
        }
        let session = prepare_session_for_turn(self, input).await?;
        self.attach_provider_session(&session).await?;
        Ok(session)
    }

    pub fn session_mcp_oauth_support(&self, session_id: &SessionId) -> Result<McpOauthSupport> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        Ok(provider.capabilities().mcp_oauth_support)
    }

    async fn resolve_provider_mcp_servers(
        &self,
        session: &Session,
        provider: &dyn Provider,
    ) -> Result<Vec<ProviderMcpServerConfig>> {
        // Only adapters with an explicit DCC projection path may receive
        // registry definitions. Native provider configuration remains
        // independent for every other provider.
        if provider.dcc_mcp_projection_version().is_none() {
            return Ok(Vec::new());
        }

        let repo = SqliteMcpRepo::open(&self.db_path)?;
        resolve_session_mcp_servers(
            &repo,
            &SystemCredentialStore::default(),
            &ResolveSessionMcpInput {
                provider_id: ProviderId(session.provider_id.clone()),
                project_id: session.project_id.clone(),
                session_id: session.id.clone(),
            },
        )
        .await
    }

    pub fn list_mcp_runtime_statuses(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<McpRuntimeStatus>> {
        Ok(self
            .lock_store()?
            .mcp_runtime_statuses
            .get(session_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn replace_mcp_runtime_statuses(
        &self,
        session_id: &SessionId,
        provider_id: &str,
        provider_version: &str,
        mut statuses: Vec<McpRuntimeStatus>,
    ) -> Result<()> {
        let mut definition_ids = HashSet::with_capacity(statuses.len());
        for status in &statuses {
            status.validate().map_err(|_| {
                dcc_core::CoreError::Provider(
                    "provider returned an invalid MCP runtime status".to_string(),
                )
            })?;
            if &status.session_id != session_id
                || status.provider_id.0 != provider_id
                || status.provider_version != provider_version
                || !definition_ids.insert(status.definition_id.clone())
            {
                return Err(dcc_core::CoreError::Provider(
                    "provider returned an invalid MCP runtime status".to_string(),
                ));
            }
        }
        statuses.sort_unstable_by(|left, right| left.definition_id.0.cmp(&right.definition_id.0));

        {
            let mut store = self.lock_store()?;
            if statuses.is_empty() {
                store.mcp_runtime_statuses.remove(session_id);
            } else {
                store
                    .mcp_runtime_statuses
                    .insert(session_id.clone(), statuses.clone());
            }
        }

        self.publish(
            dcc_core::ports::events::CoreEvent::SessionMcpRuntimeStatusChanged {
                session_id: session_id.0.clone(),
                statuses,
            },
        )
        .await
    }

    async fn clear_mcp_runtime_statuses(&self, session_id: &SessionId) -> Result<()> {
        let removed = self
            .lock_store()?
            .mcp_runtime_statuses
            .remove(session_id)
            .is_some();
        if !removed {
            return Ok(());
        }
        self.publish(
            dcc_core::ports::events::CoreEvent::SessionMcpRuntimeStatusChanged {
                session_id: session_id.0.clone(),
                statuses: Vec::new(),
            },
        )
        .await
    }

    fn mcp_oauth_credential_reference(
        provider_id: &str,
        definition_id: &McpDefinitionId,
    ) -> McpSecretReferenceId {
        let mut digest = Sha256::new();
        digest.update(b"dcc-mcp-oauth-grant-v1\0");
        digest.update(provider_id.as_bytes());
        digest.update(b"\0");
        digest.update(definition_id.0.as_bytes());
        McpSecretReferenceId(format!("oauth-grant:{:x}", digest.finalize()))
    }

    async fn persist_provider_mcp_oauth_updates(
        &self,
        binding: &ProviderSessionBinding,
        provider: &dyn Provider,
        signaled_definition_id: &McpDefinitionId,
    ) -> Result<()> {
        if !binding
            .projected_mcp_definition_ids
            .contains(signaled_definition_id)
        {
            return Err(dcc_core::CoreError::Provider(
                "provider returned OAuth state for an unknown MCP definition".to_string(),
            ));
        }

        let updates = provider.take_mcp_oauth_updates(&binding.handle).await?;
        if updates.is_empty() {
            return Ok(());
        }

        let repo = SqliteMcpRepo::open(&self.db_path)?;
        let credential_store = SystemCredentialStore::default();
        let provider_id = ProviderId(binding.provider_id.clone());
        for update in updates {
            if !binding
                .projected_mcp_definition_ids
                .contains(&update.definition_id)
            {
                return Err(dcc_core::CoreError::Provider(
                    "provider returned OAuth state for an unknown MCP definition".to_string(),
                ));
            }
            let definition = repo
                .get_mcp_definition(&update.definition_id)
                .await?
                .ok_or_else(|| {
                    dcc_core::CoreError::Repository(
                        "OAuth state references a missing MCP definition".to_string(),
                    )
                })?;
            let McpTransport::Http { .. } = &definition.transport else {
                return Err(dcc_core::CoreError::Provider(
                    "provider returned OAuth state for a non-HTTP MCP definition".to_string(),
                ));
            };
            let resource_fingerprint =
                mcp_oauth_resource_fingerprint(&definition).map_err(|_| {
                    dcc_core::CoreError::Provider(
                        "provider returned OAuth state for an invalid MCP resource".to_string(),
                    )
                })?;
            let existing = repo
                .get_mcp_oauth_grant(&update.definition_id, &provider_id)
                .await?;
            let Some(state) = update.state else {
                if let Some(existing) = existing {
                    credential_store
                        .delete_secret(&existing.secret_ref)
                        .await
                        .map_err(|_| {
                            dcc_core::CoreError::Provider(
                                "MCP OAuth credential persistence failed".to_string(),
                            )
                        })?;
                    repo.delete_mcp_oauth_grant(&update.definition_id, &provider_id)
                        .await?;
                }
                continue;
            };
            let now = Utc::now().to_rfc3339();
            let secret_ref =
                Self::mcp_oauth_credential_reference(&binding.provider_id, &update.definition_id);
            let created_at = existing
                .as_ref()
                .filter(|grant| grant.resource_fingerprint == resource_fingerprint)
                .map(|grant| grant.created_at.clone())
                .unwrap_or_else(|| now.clone());

            credential_store
                .store_secret(&secret_ref, state.into_secret())
                .await
                .map_err(|_| {
                    dcc_core::CoreError::Provider(
                        "MCP OAuth credential persistence failed".to_string(),
                    )
                })?;
            repo.save_mcp_oauth_grant(&McpOauthGrant {
                definition_id: update.definition_id,
                provider_id: provider_id.clone(),
                resource_fingerprint,
                secret_ref,
                created_at,
                updated_at: now,
            })
            .await?;
        }
        Ok(())
    }

    pub async fn validate_start_thread_scope(&self, input: &StartThreadInput) -> Result<()> {
        if input.additional_workspace_ids.is_empty() {
            return Ok(());
        }
        let provider = provider_runtime(&input.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                input.provider_id
            ))
        })?;
        let candidate = Session {
            id: SessionId("scope-validation".to_string()),
            project_id: input.project_id.clone(),
            workspace_id: input.workspace_id.clone(),
            additional_workspace_ids: input.additional_workspace_ids.clone(),
            provider_id: input.provider_id.clone(),
            model: input.model.clone(),
            provider_runtime: input.provider_runtime.clone(),
            working_directory_override: input.working_directory_override.clone(),
            state: dcc_core::domain::session::SessionState::Draft,
            created_at: String::new(),
            updated_at: String::new(),
        };
        self.resolve_session_working_directories(
            &candidate,
            provider.capabilities().supports_multi_root,
        )
        .await
        .map(|_| ())
    }

    async fn resolve_session_working_directories(
        &self,
        session: &Session,
        provider_supports_multi_root: bool,
    ) -> Result<(String, Vec<String>)> {
        let workspace_repo = SqliteWorkspaceRepo::open(&self.db_path)?;
        let primary = workspace_repo
            .get_workspace(&session.workspace_id)
            .await?
            .ok_or_else(|| {
                dcc_core::CoreError::Repository(format!(
                    "workspace not found for session {}",
                    session.id.0
                ))
            })?;

        if session.additional_workspace_ids.is_empty() {
            let working_directory = session
                .working_directory_override
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .or_else(|| {
                    primary
                        .worktree_path
                        .as_ref()
                        .filter(|value| !value.trim().is_empty())
                        .cloned()
                })
                .unwrap_or_else(|| primary.root_path.clone());
            return Ok((working_directory, Vec::new()));
        }

        if !provider_supports_multi_root {
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {} does not support isolated multi-workspace sessions yet",
                session.provider_id
            )));
        }
        if session
            .working_directory_override
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(dcc_core::CoreError::InvalidInput(
                "working_directory_override is not allowed for multi-workspace sessions"
                    .to_string(),
            ));
        }

        let bundle = workspace_repo
            .get_workspace_bundle_for_workspace(&session.workspace_id)
            .await?
            .ok_or_else(|| {
                dcc_core::CoreError::InvalidInput(
                    "multi-workspace session must use a DCC workspace bundle".to_string(),
                )
            })?;
        if bundle.bundle.state != WorkspaceBundleState::Ready {
            return Err(dcc_core::CoreError::InvalidInput(
                "multi-workspace bundle must be ready".to_string(),
            ));
        }
        if bundle.bundle.primary_workspace_id != session.workspace_id {
            return Err(dcc_core::CoreError::InvalidInput(
                "session primary workspace must match the bundle primary workspace".to_string(),
            ));
        }

        let expected_workspace_ids = bundle
            .members
            .iter()
            .map(|member| member.workspace_id.clone())
            .collect::<HashSet<_>>();
        let mut requested_workspace_ids = HashSet::from([session.workspace_id.clone()]);
        requested_workspace_ids.extend(session.additional_workspace_ids.iter().cloned());
        if requested_workspace_ids != expected_workspace_ids {
            return Err(dcc_core::CoreError::InvalidInput(
                "session workspace scope must contain every member of its bundle exactly once"
                    .to_string(),
            ));
        }

        let resolve_managed_root = |workspace: &Workspace| -> Result<String> {
            if workspace.state != dcc_core::domain::workspace::WorkspaceState::Ready {
                return Err(dcc_core::CoreError::InvalidInput(format!(
                    "workspace {} must be ready",
                    workspace.id.0
                )));
            }
            let root = workspace
                .worktree_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    dcc_core::CoreError::InvalidInput(format!(
                        "workspace {} has no DCC-managed worktree",
                        workspace.id.0
                    ))
                })?;
            if !PathBuf::from(root).is_absolute() {
                return Err(dcc_core::CoreError::InvalidInput(format!(
                    "workspace {} worktree path must be absolute",
                    workspace.id.0
                )));
            }
            Ok(root.to_string())
        };

        let primary_root = resolve_managed_root(&primary)?;
        let mut seen_roots = HashSet::from([primary_root.clone()]);
        let mut additional_roots = Vec::with_capacity(session.additional_workspace_ids.len());
        for workspace_id in &session.additional_workspace_ids {
            let workspace = workspace_repo
                .get_workspace(workspace_id)
                .await?
                .ok_or_else(|| {
                    dcc_core::CoreError::Repository(format!(
                        "workspace not found for multi-workspace session: {}",
                        workspace_id.0
                    ))
                })?;
            let root = resolve_managed_root(&workspace)?;
            if !seen_roots.insert(root.clone()) {
                return Err(dcc_core::CoreError::InvalidInput(
                    "multi-workspace roots must be distinct".to_string(),
                ));
            }
            additional_roots.push(root);
        }

        Ok((primary_root, additional_roots))
    }

    async fn spawn_provider_bridge(
        &self,
        session_id: SessionId,
        binding: ProviderSessionBinding,
        provider: Arc<dyn Provider>,
    ) {
        let state = self.clone();
        tokio::spawn(async move {
            let mut events = provider.stream_events(&binding.handle);

            while let Some(event) = events.next().await {
                match event {
                    Ok(ProviderEvent::Started { .. }) => {}
                    Ok(ProviderEvent::McpRuntimeStatusSnapshot { statuses }) => {
                        if let Some(provider_version) = provider.dcc_mcp_projection_version() {
                            let _ = state
                                .replace_mcp_runtime_statuses(
                                    &session_id,
                                    &binding.provider_id,
                                    provider_version,
                                    statuses,
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::McpOauthStateChanged { definition_id }) => {
                        if state
                            .persist_provider_mcp_oauth_updates(
                                &binding,
                                provider.as_ref(),
                                &definition_id,
                            )
                            .await
                            .is_err()
                        {
                            if let Some(provider_version) = provider.dcc_mcp_projection_version() {
                                let mut statuses = state
                                    .lock_store()
                                    .ok()
                                    .and_then(|store| {
                                        store.mcp_runtime_statuses.get(&session_id).cloned()
                                    })
                                    .unwrap_or_default();
                                if let Some(status) = statuses
                                    .iter_mut()
                                    .find(|status| status.definition_id == definition_id)
                                {
                                    status.state = McpRuntimeState::Failed;
                                    status.tools.clear();
                                    status.checked_at = Utc::now().to_rfc3339();
                                    status.bounded_error = Some(McpRuntimeError::bounded(
                                        McpErrorCategory::Authentication,
                                        "MCP OAuth credential persistence failed",
                                    ));
                                    let _ = state
                                        .replace_mcp_runtime_statuses(
                                            &session_id,
                                            &binding.provider_id,
                                            provider_version,
                                            statuses,
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                    Ok(ProviderEvent::TextDelta { content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::ReasoningStarted { id, label, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnReasoningStarted {
										turn_id: TurnId(turn_id.clone()),
										reasoning_id: id.clone(),
										label: label.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnReasoningStarted {
										session_id: session_id.0.clone(),
										turn_id,
										reasoning_id: id,
										label,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ReasoningDelta { id, content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnReasoningDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        reasoning_id: id.clone(),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnReasoningDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        reasoning_id: id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::ReasoningCompleted { id, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnReasoningCompleted {
										turn_id: TurnId(turn_id.clone()),
										reasoning_id: id.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnReasoningCompleted {
										session_id: session_id.0.clone(),
										turn_id,
										reasoning_id: id,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallStarted {
                        id,
                        action,
                        command,
                        file,
                        ..
                    }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnToolCallStarted {
										turn_id: TurnId(turn_id.clone()),
										tool_call_id: id.clone(),
										action: action.clone(),
										command: command.clone(),
										file: file.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnToolCallStarted {
										session_id: session_id.0.clone(),
										turn_id,
										tool_call_id: id,
										action,
										command,
										file,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallDelta { id, content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnToolCallDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        tool_call_id: id.clone(),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnToolCallDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        tool_call_id: id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallCompleted { id, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnToolCallCompleted {
										turn_id: TurnId(turn_id.clone()),
										tool_call_id: id.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnToolCallCompleted {
										session_id: session_id.0.clone(),
										turn_id,
										tool_call_id: id,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallFailed { id, reason, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnToolCallFailed {
                                        turn_id: TurnId(turn_id.clone()),
                                        tool_call_id: id.clone(),
                                        reason: reason.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnToolCallFailed {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        tool_call_id: id,
                                        reason,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::UserInputRequested { id, questions, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnUserInputRequested {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: id.clone(),
                                        questions: questions.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnUserInputRequested {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: id,
                                        questions,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::UserInputResolved { id, answers, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnUserInputResolved {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: id.clone(),
                                        answers: answers.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnUserInputResolved {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: id,
                                        answers,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::PermissionRequested { request, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnPermissionRequested {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: request.request_id.clone(),
                                        tool_name: request.tool_name.clone(),
                                        title: request.title.clone(),
                                        description: request.description.clone(),
                                        command: request.command.clone(),
                                        file: request.file.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnPermissionRequested {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: request.request_id,
                                        tool_name: request.tool_name,
                                        title: request.title,
                                        description: request.description,
                                        command: request.command,
                                        file: request.file,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::PermissionResolved { id, behavior, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnPermissionResolved {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: id.clone(),
                                        behavior: behavior.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnPermissionResolved {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: id,
                                        behavior,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::Completed { .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.take();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnCompleted {
                                        turn_id: TurnId(turn_id.clone()),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnCompleted {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::Failed { message, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.take();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnAborted {
                                        turn_id: TurnId(turn_id.clone()),
                                        reason: Some(message.clone()),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnAborted {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        reason: Some(message),
                                    },
                                )
                                .await;
                        }
                    }
                    Err(error) => {
                        let turn_id = binding.current_turn_id.lock().await.take();
                        if let Some(turn_id) = turn_id {
                            let reason = error.to_string();
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnAborted {
                                        turn_id: TurnId(turn_id.clone()),
                                        reason: Some(reason.clone()),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnAborted {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        reason: Some(reason),
                                    },
                                )
                                .await;
                        }
                    }
                }
            }

            if let Ok(mut store) = state.store.lock() {
                store.provider_sessions.remove(&session_id);
            }
            let _ = state.clear_mcp_runtime_statuses(&session_id).await;
        });
    }

    pub async fn set_active_turn(
        &self,
        session_id: &SessionId,
        turn_id: Option<String>,
    ) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        *binding.current_turn_id.lock().await = turn_id;
        Ok(())
    }

    pub async fn send_provider_input(&self, session_id: &SessionId, input: Input) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        let input = match input {
            Input::Turn(mut turn) => {
                let session = SessionRepo::get_session(&self.session_repo, session_id)
                    .await?
                    .ok_or_else(|| {
                        dcc_core::CoreError::Repository(format!(
                            "session not found while preparing provider input: {}",
                            session_id.0
                        ))
                    })?;
                if let Some(scope_instructions) =
                    self.multi_workspace_scope_instructions(&session).await?
                {
                    turn.tool_instructions = Some(match turn.tool_instructions {
                        Some(existing) if !existing.trim().is_empty() => {
                            format!("{scope_instructions}\n\n{existing}")
                        }
                        _ => scope_instructions,
                    });
                }
                Input::Turn(turn)
            }
            other => other,
        };
        provider.send_input(&binding.handle, input).await
    }

    pub async fn start_mcp_oauth(
        &self,
        session_id: &SessionId,
        definition_id: &dcc_core::domain::mcp::McpDefinitionId,
    ) -> Result<ProviderMcpOauthStart> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        provider
            .start_mcp_oauth(&binding.handle, definition_id)
            .await
    }

    async fn multi_workspace_scope_instructions(
        &self,
        session: &Session,
    ) -> Result<Option<String>> {
        if session.additional_workspace_ids.is_empty() {
            return Ok(None);
        }
        let workspace_repo = SqliteWorkspaceRepo::open(&self.db_path)?;
        let bundle = workspace_repo
            .get_workspace_bundle_for_workspace(&session.workspace_id)
            .await?
            .ok_or_else(|| {
                dcc_core::CoreError::InvalidInput(
                    "multi-workspace session bundle is no longer available".to_string(),
                )
            })?;
        let mut lines = vec![
            "DCC authorized multi-workspace scope:".to_string(),
            "Use only the isolated worktree paths listed below for file reads and writes. Never edit the repositories' original checkouts or any unlisted local project. Decide which listed projects need changes, keep producer/consumer contracts consistent, and test the affected projects in the same task context.".to_string(),
        ];
        for member in &bundle.members {
            let workspace = workspace_repo
                .get_workspace(&member.workspace_id)
                .await?
                .ok_or_else(|| {
                    dcc_core::CoreError::Repository(format!(
                        "workspace not found while building session scope: {}",
                        member.workspace_id.0
                    ))
                })?;
            let root = workspace
                .worktree_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    dcc_core::CoreError::InvalidInput(format!(
                        "workspace {} has no isolated worktree",
                        workspace.id.0
                    ))
                })?;
            let role = if workspace.id == session.workspace_id {
                "primary"
            } else {
                "additional"
            };
            lines.push(format!(
                "- {role}: {} | project={} | base={} | worktree={root}",
                workspace.name.as_deref().unwrap_or(&workspace.id.0),
                workspace.project_id.0,
                workspace.base_branch,
            ));
        }
        Ok(Some(lines.join("\n")))
    }

    pub async fn cancel_provider_session(&self, session_id: &SessionId) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        *binding.current_turn_id.lock().await = None;
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        let result = provider.cancel(&binding.handle).await;
        if let Ok(mut store) = self.store.lock() {
            store.provider_sessions.remove(session_id);
        }
        let _ = self.clear_mcp_runtime_statuses(session_id).await;
        result
    }
}

#[async_trait]
impl WorkspaceRepo for SessionCommandState {
    async fn save_workspace(&self, _workspace: &Workspace) -> Result<()> {
        Ok(())
    }

    async fn get_workspace(&self, _id: &WorkspaceId) -> Result<Option<Workspace>> {
        Ok(None)
    }

    async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        Ok(Vec::new())
    }

    async fn delete_workspace(&self, _id: &WorkspaceId) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl RepositoryRepo for SessionCommandState {
    async fn save_repository(&self, _repository: &Repository) -> Result<()> {
        Ok(())
    }

    async fn get_repository(&self, _id: &RepositoryId) -> Result<Option<Repository>> {
        Ok(None)
    }

    async fn list_repositories(&self) -> Result<Vec<Repository>> {
        Ok(Vec::new())
    }

    async fn delete_repository(&self, _id: &RepositoryId) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ProjectRepo for SessionCommandState {
    async fn save_project(&self, _project: &Project) -> Result<()> {
        Ok(())
    }

    async fn get_project(&self, _id: &ProjectId) -> Result<Option<Project>> {
        Ok(None)
    }
}

#[async_trait]
impl SessionRepo for SessionCommandState {
    async fn save_session(&self, session: &Session) -> Result<()> {
        SessionRepo::save_session(&self.session_repo, session).await
    }

    async fn get_session(&self, id: &SessionId) -> Result<Option<Session>> {
        SessionRepo::get_session(&self.session_repo, id).await
    }

    async fn delete_session(&self, id: &SessionId) -> Result<()> {
        let result = SessionRepo::delete_session(&self.session_repo, id).await;
        if result.is_ok() {
            {
                let mut store = self.lock_store()?;
                store.provider_sessions.remove(id);
            }
            let _ = self.clear_mcp_runtime_statuses(id).await;
        }
        result
    }
}

#[async_trait]
impl ThreadRepo for SessionCommandState {
    async fn save_thread(&self, thread: &Thread) -> Result<()> {
        ThreadRepo::save_thread(&self.session_repo, thread).await
    }

    async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        ThreadRepo::get_thread(&self.session_repo, id).await
    }

    async fn find_thread_by_session_id(&self, session_id: &SessionId) -> Result<Option<Thread>> {
        ThreadRepo::find_thread_by_session_id(&self.session_repo, session_id).await
    }

    async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
        ThreadRepo::delete_thread(&self.session_repo, id).await
    }
}

#[async_trait]
impl SessionEventRepo for SessionCommandState {
    async fn append_event(&self, event: &SessionEventRecord) -> Result<()> {
        SessionEventRepo::append_event(&self.session_repo, event).await
    }

    async fn list_events_by_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>> {
        SessionEventRepo::list_events_by_session(&self.session_repo, session_id).await
    }

    async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
        SessionEventRepo::delete_events_by_session(&self.session_repo, session_id).await
    }
}

#[async_trait]
impl DelegationRepo for SessionCommandState {
    async fn save_delegation(&self, delegation: &Delegation) -> Result<()> {
        DelegationRepo::save_delegation(&self.session_repo, delegation).await
    }

    async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        DelegationRepo::get_delegation(&self.session_repo, id).await
    }

    async fn list_delegations(
        &self,
        workspace_id: Option<&WorkspaceId>,
        parent_session_id: Option<&SessionId>,
    ) -> Result<Vec<Delegation>> {
        DelegationRepo::list_delegations(&self.session_repo, workspace_id, parent_session_id).await
    }

    async fn update_delegation_status(
        &self,
        id: &DelegationId,
        status: DelegationStatus,
        updated_at: String,
    ) -> Result<Option<Delegation>> {
        DelegationRepo::update_delegation_status(&self.session_repo, id, status, updated_at).await
    }
}

#[async_trait]
impl EventBus for SessionCommandState {
    async fn publish(&self, event: dcc_core::ports::events::CoreEvent) -> Result<()> {
        self.event_bus.publish(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::{
        mcp::McpDefinitionId,
        session::SessionState,
        workspace::WorkspaceState,
        workspace_bundle::{
            WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleMember, WorkspaceBundleState,
        },
    };

    fn sample_workspace(id: &str, root: &str) -> Workspace {
        Workspace {
            id: WorkspaceId(id.to_string()),
            project_id: ProjectId(format!("project-{id}")),
            name: Some(id.to_string()),
            root_path: format!("/original/{id}"),
            base_branch: "main".to_string(),
            worktree_path: Some(root.to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn mcp_status(
        definition_id: &str,
        session_id: &SessionId,
        state: McpRuntimeState,
    ) -> McpRuntimeStatus {
        McpRuntimeStatus {
            definition_id: McpDefinitionId(definition_id.to_string()),
            provider_id: ProviderId("claude_code".to_string()),
            provider_version: "claude-agent-sdk@test+claude-code@test".to_string(),
            session_id: session_id.clone(),
            state,
            tools: Vec::new(),
            checked_at: "2026-07-28T00:00:00Z".to_string(),
            bounded_error: None,
        }
    }

    #[test]
    fn mcp_runtime_snapshots_are_ephemeral_sorted_and_identity_bound() {
        let db_path = std::env::temp_dir().join(format!("dcc-mcp-{}.sqlite", Uuid::new_v4()));
        let state = SessionCommandState::new_headless(db_path.clone(), std::env::temp_dir());
        let session_id = SessionId("session-1".to_string());
        let provider_version = "claude-agent-sdk@test+claude-code@test";

        futures::executor::block_on(state.replace_mcp_runtime_statuses(
            &session_id,
            "claude_code",
            provider_version,
            vec![
                mcp_status("zeta", &session_id, McpRuntimeState::Connected),
                mcp_status("alpha", &session_id, McpRuntimeState::AttachingProvider),
            ],
        ))
        .expect("replace MCP status snapshot");

        assert_eq!(
            state
                .list_mcp_runtime_statuses(&session_id)
                .expect("list statuses")
                .iter()
                .map(|status| status.definition_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "zeta"]
        );

        let mismatch = futures::executor::block_on(state.replace_mcp_runtime_statuses(
            &session_id,
            "claude_code",
            "different-version",
            vec![mcp_status(
                "replacement",
                &session_id,
                McpRuntimeState::Connected,
            )],
        ));
        assert!(matches!(mismatch, Err(dcc_core::CoreError::Provider(_))));
        assert_eq!(
            state
                .list_mcp_runtime_statuses(&session_id)
                .expect("snapshot remains")
                .len(),
            2
        );

        futures::executor::block_on(state.clear_mcp_runtime_statuses(&session_id))
            .expect("clear snapshot");
        assert!(state
            .list_mcp_runtime_statuses(&session_id)
            .expect("list cleared statuses")
            .is_empty());

        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn multi_workspace_scope_resolves_only_bundle_worktrees_and_gates_provider() {
        let db_path = std::env::temp_dir().join(format!("dcc-scope-{}.sqlite", Uuid::new_v4()));
        let repo = SqliteWorkspaceRepo::open(&db_path).expect("open workspace repo");
        let primary = sample_workspace("primary", "/tmp/dcc-primary-worktree");
        let secondary = sample_workspace("secondary", "/tmp/dcc-secondary-worktree");
        futures::executor::block_on(repo.save_workspace(&primary)).expect("save primary");
        futures::executor::block_on(repo.save_workspace(&secondary)).expect("save secondary");
        let bundle_id = WorkspaceBundleId("bundle-1".to_string());
        futures::executor::block_on(repo.save_workspace_bundle(
            &WorkspaceBundle {
                id: bundle_id.clone(),
                name: "feature".to_string(),
                primary_workspace_id: primary.id.clone(),
                state: WorkspaceBundleState::Ready,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
            &[
                WorkspaceBundleMember {
                    bundle_id: bundle_id.clone(),
                    workspace_id: primary.id.clone(),
                    created_for_bundle: true,
                    position: 0,
                },
                WorkspaceBundleMember {
                    bundle_id,
                    workspace_id: secondary.id.clone(),
                    created_for_bundle: true,
                    position: 1,
                },
            ],
        ))
        .expect("save bundle");

        let state = SessionCommandState::new_headless(db_path.clone(), std::env::temp_dir());
        let session = Session {
            id: SessionId("session-1".to_string()),
            project_id: primary.project_id.clone(),
            workspace_id: primary.id.clone(),
            additional_workspace_ids: vec![secondary.id.clone()],
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let unsupported =
            futures::executor::block_on(state.resolve_session_working_directories(&session, false));
        assert!(matches!(unsupported, Err(dcc_core::CoreError::Provider(_))));

        let (primary_root, additional_roots) =
            futures::executor::block_on(state.resolve_session_working_directories(&session, true))
                .expect("resolve multi-root scope");
        assert_eq!(primary_root, "/tmp/dcc-primary-worktree");
        assert_eq!(additional_roots, vec!["/tmp/dcc-secondary-worktree"]);
        let instructions =
            futures::executor::block_on(state.multi_workspace_scope_instructions(&session))
                .expect("build scope instructions")
                .expect("multi scope instructions");
        assert!(instructions.contains("/tmp/dcc-primary-worktree"));
        assert!(instructions.contains("/tmp/dcc-secondary-worktree"));
        assert!(!instructions.contains("/original/primary"));

        drop(state);
        drop(repo);
        let _ = std::fs::remove_file(db_path);
    }
}
