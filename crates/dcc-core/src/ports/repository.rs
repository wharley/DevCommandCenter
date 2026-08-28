use async_trait::async_trait;

use crate::{
    domain::{
        delegation::{Delegation, DelegationId, DelegationStatus},
        delegation_worktree::{
            DelegationWorktreeOperation, DelegationWorktreeOperationId,
            DelegationWorktreeOperationState,
        },
        mcp::{
            McpBinding, McpBindingId, McpDefinition, McpDefinitionId, McpOauthGrant, McpToolPolicy,
        },
        project::{Project, ProjectId},
        repository::{Repository, RepositoryId},
        session::{Session, SessionEventKind, SessionEventRecord, SessionId, TurnId},
        thread::{Thread, ThreadId},
        usage::{ModelTokenUsage, UsageDashboard, UsageDashboardInput},
        workspace::{Workspace, WorkspaceId},
        workspace_bundle::{
            WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleMember, WorkspaceBundleState,
            WorkspaceBundleSummary,
        },
    },
    Result,
};

/// Result of an idempotent durable event append.
///
/// `Existing` returns the canonical record already stored by another caller.
/// Repositories may therefore safely retry an append after a transport or
/// process failure without creating a second durable event.
#[derive(Clone, Debug)]
pub enum AppendEventOutcome {
    Inserted(SessionEventRecord),
    Existing(SessionEventRecord),
}

#[async_trait]
pub trait WorkspaceRepo: Send + Sync {
    async fn save_workspace(&self, workspace: &Workspace) -> Result<()>;
    async fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>>;
    async fn list_workspaces(&self) -> Result<Vec<Workspace>>;
    async fn delete_workspace(&self, id: &WorkspaceId) -> Result<()>;
}

#[async_trait]
pub trait WorkspaceBundleRepo: Send + Sync {
    async fn save_workspace_bundle(
        &self,
        bundle: &WorkspaceBundle,
        members: &[WorkspaceBundleMember],
    ) -> Result<()>;
    async fn get_workspace_bundle(
        &self,
        id: &WorkspaceBundleId,
    ) -> Result<Option<WorkspaceBundleSummary>>;
    async fn get_workspace_bundle_for_workspace(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Option<WorkspaceBundleSummary>>;
    async fn list_workspace_bundles(&self) -> Result<Vec<WorkspaceBundleSummary>>;
    async fn set_workspace_bundle_state(
        &self,
        id: &WorkspaceBundleId,
        state: WorkspaceBundleState,
        updated_at: String,
    ) -> Result<Option<WorkspaceBundleSummary>>;
    async fn delete_workspace_bundle(&self, id: &WorkspaceBundleId) -> Result<()>;
}

#[async_trait]
pub trait RepositoryRepo: Send + Sync {
    async fn save_repository(&self, repository: &Repository) -> Result<()>;
    async fn get_repository(&self, id: &RepositoryId) -> Result<Option<Repository>>;
    async fn list_repositories(&self) -> Result<Vec<Repository>>;
    async fn delete_repository(&self, id: &RepositoryId) -> Result<()>;
}

#[async_trait]
pub trait ProjectRepo: Send + Sync {
    async fn save_project(&self, project: &Project) -> Result<()>;
    async fn get_project(&self, id: &ProjectId) -> Result<Option<Project>>;
}

#[async_trait]
pub trait McpRepo: Send + Sync {
    async fn save_mcp_definition(&self, definition: &McpDefinition) -> Result<()>;
    async fn get_mcp_definition(&self, id: &McpDefinitionId) -> Result<Option<McpDefinition>>;
    async fn list_mcp_definitions(&self) -> Result<Vec<McpDefinition>>;
    async fn delete_mcp_definition(&self, id: &McpDefinitionId) -> Result<()>;

    async fn save_mcp_binding(&self, binding: &McpBinding) -> Result<()>;
    async fn get_mcp_binding(&self, id: &McpBindingId) -> Result<Option<McpBinding>>;
    async fn list_mcp_bindings(
        &self,
        definition_id: Option<&McpDefinitionId>,
    ) -> Result<Vec<McpBinding>>;
    async fn delete_mcp_binding(&self, id: &McpBindingId) -> Result<()>;

    async fn save_mcp_tool_policy(&self, policy: &McpToolPolicy) -> Result<()>;
    async fn list_mcp_tool_policies(
        &self,
        definition_id: Option<&McpDefinitionId>,
    ) -> Result<Vec<McpToolPolicy>>;
    async fn delete_mcp_tool_policy(
        &self,
        definition_id: &McpDefinitionId,
        tool_name: &str,
    ) -> Result<()>;

    async fn save_mcp_oauth_grant(&self, grant: &McpOauthGrant) -> Result<()> {
        let _ = grant;
        Err(crate::CoreError::Repository(
            "MCP OAuth grant persistence is unavailable".to_string(),
        ))
    }

    async fn get_mcp_oauth_grant(
        &self,
        definition_id: &McpDefinitionId,
        provider_id: &crate::domain::provider::ProviderId,
    ) -> Result<Option<McpOauthGrant>> {
        let _ = (definition_id, provider_id);
        Ok(None)
    }

    async fn list_mcp_oauth_grants(
        &self,
        definition_id: Option<&McpDefinitionId>,
    ) -> Result<Vec<McpOauthGrant>> {
        let _ = definition_id;
        Ok(Vec::new())
    }

    async fn delete_mcp_oauth_grant(
        &self,
        definition_id: &McpDefinitionId,
        provider_id: &crate::domain::provider::ProviderId,
    ) -> Result<()> {
        let _ = (definition_id, provider_id);
        Ok(())
    }
}

#[async_trait]
pub trait SessionRepo: Send + Sync {
    async fn save_session(&self, session: &Session) -> Result<()>;
    async fn get_session(&self, id: &SessionId) -> Result<Option<Session>>;
    async fn delete_session(&self, id: &SessionId) -> Result<()>;
}

#[async_trait]
pub trait SessionEventRepo: Send + Sync {
    async fn append_event(&self, event: &SessionEventRecord) -> Result<AppendEventOutcome>;
    /// Returns the canonical durable terminal event for one turn.
    ///
    /// The default keeps lightweight mocks source-compatible. Production
    /// repositories should override it with an indexed, metadata-validating
    /// lookup. Multiple terminal records or records attributed to another
    /// session fail closed.
    async fn find_terminal_event(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
    ) -> Result<Option<SessionEventRecord>> {
        let events = self.list_events_by_session(session_id).await?;
        let mut terminal = None;
        for event in events {
            if event.session_id != *session_id {
                return Err(crate::CoreError::Repository(
                    "terminal event session attribution is inconsistent".to_string(),
                ));
            }
            let event_turn_id = match &event.kind {
                SessionEventKind::TurnCompleted { turn_id }
                | SessionEventKind::TurnAborted { turn_id, .. } => turn_id,
                _ => continue,
            };
            if event_turn_id != turn_id {
                continue;
            }
            if terminal.is_some() {
                return Err(crate::CoreError::Repository(
                    "multiple terminal events exist for one turn".to_string(),
                ));
            }
            terminal = Some(event);
        }
        Ok(terminal)
    }
    async fn list_events_by_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>>;
    async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()>;
}

#[async_trait]
pub trait UsageRepo: Send + Sync {
    async fn replace_turn_usage(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        recorded_at: &str,
        models: &[ModelTokenUsage],
    ) -> Result<()>;
    async fn usage_dashboard(&self, input: &UsageDashboardInput) -> Result<UsageDashboard>;
}

#[async_trait]
pub trait DelegationRepo: Send + Sync {
    async fn save_delegation(&self, delegation: &Delegation) -> Result<()>;
    async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>>;
    async fn list_delegations(
        &self,
        workspace_id: Option<&WorkspaceId>,
        parent_session_id: Option<&SessionId>,
    ) -> Result<Vec<Delegation>>;
    async fn update_delegation_status(
        &self,
        id: &DelegationId,
        status: DelegationStatus,
        updated_at: String,
    ) -> Result<Option<Delegation>>;
}

/// Durable lifecycle journal for delegation worktree creation, binding,
/// application, and cleanup. CAS is the only update operation so stale
/// workers cannot overwrite a successor state.
#[async_trait]
pub trait DelegationWorktreeOperationRepo: Send + Sync {
    async fn create_delegation_worktree_operation(
        &self,
        operation: &DelegationWorktreeOperation,
    ) -> Result<()>;
    async fn get_delegation_worktree_operation(
        &self,
        id: &DelegationWorktreeOperationId,
    ) -> Result<Option<DelegationWorktreeOperation>>;
    async fn get_delegation_worktree_operation_by_delegation_id(
        &self,
        delegation_id: &DelegationId,
    ) -> Result<Option<DelegationWorktreeOperation>>;
    async fn list_delegation_worktree_operations_by_workspace(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<DelegationWorktreeOperation>>;
    async fn compare_and_swap_delegation_worktree_operation(
        &self,
        expected_state: DelegationWorktreeOperationState,
        operation: &DelegationWorktreeOperation,
    ) -> Result<bool>;
    async fn list_delegation_worktree_operations_requiring_recovery(
        &self,
    ) -> Result<Vec<DelegationWorktreeOperation>>;
    async fn claim_delegation_worktree_removal(
        &self,
        id: &DelegationWorktreeOperationId,
        recovery_owner: &str,
        now: &str,
        lease_until: &str,
    ) -> Result<Option<DelegationWorktreeOperation>>;
    async fn finalize_delegation_worktree_removal(
        &self,
        id: &DelegationWorktreeOperationId,
        recovery_owner: &str,
        final_state: DelegationWorktreeOperationState,
        last_error: Option<String>,
        updated_at: &str,
    ) -> Result<Option<DelegationWorktreeOperation>>;
    /// Purges only a terminal `Removed` journal row. Returns false for a
    /// missing or still-live operation.
    async fn delete_removed_delegation_worktree_operation(
        &self,
        id: &DelegationWorktreeOperationId,
    ) -> Result<bool>;
}

#[async_trait]
pub trait ThreadRepo: Send + Sync {
    async fn save_thread(&self, thread: &Thread) -> Result<()>;
    async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>>;
    async fn find_thread_by_session_id(&self, session_id: &SessionId) -> Result<Option<Thread>>;
    async fn delete_thread(&self, id: &ThreadId) -> Result<()>;
}
