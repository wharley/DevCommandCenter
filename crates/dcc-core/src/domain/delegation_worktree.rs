use std::fmt;

use chrono::DateTime;
use serde::{Deserialize, Serialize};

use super::{
    delegation::DelegationId, guarded_undo::PhysicalRootId, session::SessionId,
    workspace::WorkspaceId,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DelegationWorktreeOperationId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationWorktreeOperationState {
    Preparing,
    Prepared,
    Bound,
    ReviewPending,
    Applying,
    Applied,
    Removing,
    Removed,
    CleanupRequired,
}

impl DelegationWorktreeOperationState {
    /// A removed operation no longer owns a live filesystem path or Git ref.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Removed)
    }

    /// Defines the durable state-machine edges accepted by repository CAS.
    /// Failure from any live state may be recorded as `CleanupRequired`; a
    /// recovery worker must then enter removal because partial filesystem/Git
    /// state may exist. Pre-mutation apply failures return to `ReviewPending`.
    pub fn can_transition_to(&self, next: &Self) -> bool {
        use DelegationWorktreeOperationState as State;

        matches!(
            (self, next),
            (State::Preparing, State::Prepared)
                | (State::Prepared, State::Bound)
                | (State::Prepared, State::ReviewPending)
                | (State::Prepared, State::Removing)
                | (State::Bound, State::ReviewPending)
                | (State::Bound, State::Removing)
                | (State::ReviewPending, State::Applying)
                | (State::ReviewPending, State::Removing)
                | (State::Applying, State::ReviewPending)
                | (State::Applying, State::Applied)
                | (State::Applied, State::Removing)
                | (State::Removing, State::Removed)
                | (State::CleanupRequired, State::Removing)
        ) || (!self.is_terminal() && matches!(next, State::CleanupRequired))
    }
}

/// Durable ownership and recovery journal for one delegation worktree.
///
/// Paths and Git identities are captured before commands mutate or remove the
/// worktree. Related workspace/session rows are intentionally logical links:
/// this record must survive a partially completed parent deletion.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationWorktreeOperation {
    pub operation_id: DelegationWorktreeOperationId,
    pub delegation_key: Option<String>,
    pub delegation_id: Option<DelegationId>,
    pub workspace_id: WorkspaceId,
    pub parent_session_id: Option<SessionId>,
    pub child_session_id: Option<SessionId>,
    pub source_root: String,
    pub worktree_path: String,
    pub branch: String,
    pub base_commit: String,
    pub expected_branch_oid: Option<String>,
    pub source_root_id: Option<PhysicalRootId>,
    pub worktree_root_id: Option<PhysicalRootId>,
    pub common_dir_id: Option<PhysicalRootId>,
    pub state: DelegationWorktreeOperationState,
    pub last_error: Option<String>,
    pub recovery_owner: Option<String>,
    pub recovery_lease_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl fmt::Debug for DelegationWorktreeOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DelegationWorktreeOperation")
            .field("operation_id", &self.operation_id)
            .field("delegation_key", &self.delegation_key)
            .field("delegation_id", &self.delegation_id)
            .field("workspace_id", &self.workspace_id)
            .field("parent_session_id", &self.parent_session_id)
            .field("child_session_id", &self.child_session_id)
            .field("source_root", &self.source_root)
            .field("worktree_path", &self.worktree_path)
            .field("branch", &self.branch)
            .field("base_commit", &self.base_commit)
            .field("expected_branch_oid", &self.expected_branch_oid)
            .field("source_root_id", &self.source_root_id)
            .field("worktree_root_id", &self.worktree_root_id)
            .field("common_dir_id", &self.common_dir_id)
            .field("state", &self.state)
            .field("last_error", &self.last_error)
            .field(
                "recovery_owner",
                &self.recovery_owner.as_ref().map(|_| "[redacted]"),
            )
            .field("recovery_lease_until", &self.recovery_lease_until)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

impl DelegationWorktreeOperation {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("operation_id", self.operation_id.0.as_str()),
            ("workspace_id", self.workspace_id.0.as_str()),
            ("source_root", self.source_root.as_str()),
            ("worktree_path", self.worktree_path.as_str()),
            ("branch", self.branch.as_str()),
            ("base_commit", self.base_commit.as_str()),
            ("created_at", self.created_at.as_str()),
            ("updated_at", self.updated_at.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("delegation worktree {name} cannot be empty"));
            }
        }
        for (name, value) in [
            ("delegation_key", self.delegation_key.as_deref()),
            (
                "delegation_id",
                self.delegation_id.as_ref().map(|id| id.0.as_str()),
            ),
            (
                "parent_session_id",
                self.parent_session_id.as_ref().map(|id| id.0.as_str()),
            ),
            (
                "child_session_id",
                self.child_session_id.as_ref().map(|id| id.0.as_str()),
            ),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(format!("delegation worktree {name} cannot be empty"));
            }
        }
        if let Some(oid) = self.expected_branch_oid.as_deref() {
            let oid = oid.trim();
            if !matches!(oid.len(), 40 | 64) || !oid.as_bytes().iter().all(u8::is_ascii_hexdigit) {
                return Err("delegation worktree expected_branch_oid is invalid".to_string());
            }
        }
        for root_id in [
            self.source_root_id.as_ref(),
            self.worktree_root_id.as_ref(),
            self.common_dir_id.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            root_id
                .validate()
                .map_err(|_| "delegation worktree physical identity is invalid".to_string())?;
        }
        if self
            .last_error
            .as_deref()
            .is_some_and(|error| error.trim().is_empty())
        {
            return Err("delegation worktree last_error cannot be empty".to_string());
        }
        if matches!(
            self.state,
            DelegationWorktreeOperationState::CleanupRequired
        ) && self.last_error.is_none()
        {
            return Err(
                "cleanup-required delegation worktree must retain its last error".to_string(),
            );
        }
        match (
            self.recovery_owner.as_deref(),
            self.recovery_lease_until.as_deref(),
        ) {
            (Some(owner), Some(lease_until)) => {
                if owner.trim().is_empty() || owner.len() > 256 {
                    return Err("delegation worktree recovery owner is invalid".to_string());
                }
                DateTime::parse_from_rfc3339(lease_until)
                    .map_err(|_| "delegation worktree recovery lease is not RFC3339".to_string())?;
                if self.state != DelegationWorktreeOperationState::Removing {
                    return Err(
                        "delegation worktree recovery authority requires removing state"
                            .to_string(),
                    );
                }
            }
            (None, None) => {
                if self.state == DelegationWorktreeOperationState::Removing {
                    return Err(
                        "removing delegation worktree requires recovery authority".to_string()
                    );
                }
            }
            _ => {
                return Err(
                    "delegation worktree recovery owner and lease must be stored together"
                        .to_string(),
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::DelegationWorktreeOperationState as State;

    #[test]
    fn state_machine_rejects_skipped_destructive_edges() {
        assert!(State::Preparing.can_transition_to(&State::Prepared));
        assert!(State::ReviewPending.can_transition_to(&State::Applying));
        assert!(State::Applying.can_transition_to(&State::ReviewPending));
        assert!(State::Applying.can_transition_to(&State::CleanupRequired));
        assert!(State::CleanupRequired.can_transition_to(&State::Removing));
        assert!(State::Removing.can_transition_to(&State::Removed));

        assert!(!State::Preparing.can_transition_to(&State::Applied));
        assert!(!State::Prepared.can_transition_to(&State::Removed));
        assert!(!State::Removed.can_transition_to(&State::CleanupRequired));
        assert!(!State::Removed.can_transition_to(&State::Preparing));
    }
}
