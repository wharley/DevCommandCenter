use std::fmt;

use chrono::DateTime;
use serde::{Deserialize, Serialize};

use super::{
    delegation::DelegationId, delegation_worktree::DelegationWorktreeOperationId,
    workspace::WorkspaceId,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DelegationApplyTransactionId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationApplyTransactionState {
    Preparing,
    Prepared,
    Applying,
    Applied,
    RolledBack,
    RecoveryRequired,
}

impl DelegationApplyTransactionState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Applied | Self::RolledBack)
    }

    pub fn can_pre_apply_transition_to(&self, next: &Self) -> bool {
        matches!(
            (self, next),
            (Self::Preparing, Self::Prepared)
                | (Self::Preparing, Self::RolledBack)
                | (Self::Prepared, Self::RolledBack)
        )
    }
}

/// Durable cross-process journal for one delegation apply attempt.
///
/// Filesystem artifacts are referenced by the manifest digest and accounting
/// fields but intentionally live outside SQLite. The worktree operation link
/// is logical so this recovery authority survives partially completed cleanup.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationApplyTransaction {
    pub transaction_id: DelegationApplyTransactionId,
    pub operation_id: DelegationWorktreeOperationId,
    pub delegation_id: DelegationId,
    pub workspace_id: WorkspaceId,
    pub source_head_oid: Option<String>,
    pub destination_head_oid: Option<String>,
    pub destination_ref: Option<String>,
    pub destination_index_tree_oid: Option<String>,
    pub manifest_digest: Option<String>,
    pub file_count: u32,
    pub artifact_bytes: u64,
    pub state: DelegationApplyTransactionState,
    pub recovery_owner: Option<String>,
    pub recovery_lease_until: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl fmt::Debug for DelegationApplyTransaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DelegationApplyTransaction")
            .field("transaction_id", &self.transaction_id)
            .field("operation_id", &self.operation_id)
            .field("delegation_id", &self.delegation_id)
            .field("workspace_id", &self.workspace_id)
            .field("state", &self.state)
            .field("file_count", &self.file_count)
            .field("artifact_bytes", &self.artifact_bytes)
            .field(
                "recovery_owner",
                &self.recovery_owner.as_ref().map(|_| "[redacted]"),
            )
            .field("recovery_lease_until", &self.recovery_lease_until)
            .field(
                "last_error",
                &self.last_error.as_ref().map(|_| "[redacted]"),
            )
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

impl DelegationApplyTransaction {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("transaction_id", self.transaction_id.0.as_str()),
            ("operation_id", self.operation_id.0.as_str()),
            ("delegation_id", self.delegation_id.0.as_str()),
            ("workspace_id", self.workspace_id.0.as_str()),
            ("created_at", self.created_at.as_str()),
            ("updated_at", self.updated_at.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("delegation apply {name} cannot be empty"));
            }
        }
        for (name, oid) in [
            ("source_head_oid", self.source_head_oid.as_deref()),
            ("destination_head_oid", self.destination_head_oid.as_deref()),
            (
                "destination_index_tree_oid",
                self.destination_index_tree_oid.as_deref(),
            ),
        ] {
            if oid.is_some_and(|oid| !valid_oid(oid)) {
                return Err(format!("delegation apply {name} is invalid"));
            }
        }
        if self.destination_ref.as_deref().is_some_and(|value| {
            value.trim().is_empty() || value.len() > 1_024 || value.contains('\0')
        }) {
            return Err("delegation apply destination_ref is invalid".to_string());
        }
        if let Some(digest) = self.manifest_digest.as_deref() {
            if digest.len() != 64 || !digest.as_bytes().iter().all(u8::is_ascii_hexdigit) {
                return Err("delegation apply manifest_digest is invalid".to_string());
            }
        }
        for (name, timestamp) in [
            ("created_at", self.created_at.as_str()),
            ("updated_at", self.updated_at.as_str()),
        ] {
            DateTime::parse_from_rfc3339(timestamp)
                .map_err(|_| format!("delegation apply {name} is not RFC3339"))?;
        }
        if self
            .last_error
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err("delegation apply last_error cannot be empty".to_string());
        }
        match (
            self.recovery_owner.as_deref(),
            self.recovery_lease_until.as_deref(),
        ) {
            (Some(owner), Some(lease)) => {
                if owner.trim().is_empty() || owner.len() > 256 {
                    return Err("delegation apply recovery owner is invalid".to_string());
                }
                DateTime::parse_from_rfc3339(lease)
                    .map_err(|_| "delegation apply recovery lease is not RFC3339".to_string())?;
                if self.state != DelegationApplyTransactionState::Applying {
                    return Err(
                        "delegation apply recovery authority requires applying state".to_string(),
                    );
                }
            }
            (None, None) => {
                if self.state == DelegationApplyTransactionState::Applying {
                    return Err(
                        "applying delegation transaction requires recovery authority".to_string(),
                    );
                }
            }
            _ => {
                return Err(
                    "delegation apply recovery owner and lease must be stored together".to_string(),
                )
            }
        }
        match self.state {
            DelegationApplyTransactionState::Preparing => {
                if self.source_head_oid.is_some()
                    || self.destination_head_oid.is_some()
                    || self.destination_ref.is_some()
                    || self.destination_index_tree_oid.is_some()
                    || self.manifest_digest.is_some()
                    || self.file_count != 0
                    || self.artifact_bytes != 0
                {
                    return Err(
                        "preparing delegation apply cannot publish manifest accounting".to_string(),
                    );
                }
            }
            DelegationApplyTransactionState::Prepared
            | DelegationApplyTransactionState::Applying
            | DelegationApplyTransactionState::Applied => {
                if self.source_head_oid.is_none()
                    || self.destination_head_oid.is_none()
                    || self.destination_index_tree_oid.is_none()
                    || self.manifest_digest.is_none()
                    || self.file_count == 0
                {
                    return Err(
                        "prepared delegation apply requires a non-empty manifest".to_string()
                    );
                }
                if self.last_error.is_some() {
                    return Err(
                        "active/successful delegation apply cannot retain an error".to_string()
                    );
                }
            }
            DelegationApplyTransactionState::RecoveryRequired => {
                if self.source_head_oid.is_none()
                    || self.destination_head_oid.is_none()
                    || self.destination_index_tree_oid.is_none()
                    || self.manifest_digest.is_none()
                    || self.file_count == 0
                    || self.last_error.is_none()
                {
                    return Err(
                        "recovery-required delegation apply needs manifest and error".to_string(),
                    );
                }
            }
            DelegationApplyTransactionState::RolledBack => {}
        }
        Ok(())
    }
}

fn valid_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.as_bytes().iter().all(u8::is_ascii_hexdigit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transaction() -> DelegationApplyTransaction {
        DelegationApplyTransaction {
            transaction_id: DelegationApplyTransactionId("tx-1".into()),
            operation_id: DelegationWorktreeOperationId("op-1".into()),
            delegation_id: DelegationId("delegation-1".into()),
            workspace_id: WorkspaceId("workspace-1".into()),
            source_head_oid: None,
            destination_head_oid: None,
            destination_ref: None,
            destination_index_tree_oid: None,
            manifest_digest: None,
            file_count: 0,
            artifact_bytes: 0,
            state: DelegationApplyTransactionState::Preparing,
            recovery_owner: None,
            recovery_lease_until: None,
            last_error: None,
            created_at: "2026-08-28T00:00:00Z".into(),
            updated_at: "2026-08-28T00:00:00Z".into(),
        }
    }

    #[test]
    fn validates_manifest_and_lease_state() {
        let mut value = transaction();
        assert!(value.validate().is_ok());
        value.state = DelegationApplyTransactionState::Prepared;
        assert!(value.validate().is_err());
        value.source_head_oid = Some("a".repeat(40));
        value.destination_head_oid = Some("b".repeat(40));
        value.destination_ref = Some("refs/heads/main".into());
        value.destination_index_tree_oid = Some("c".repeat(40));
        value.manifest_digest = Some("d".repeat(64));
        value.file_count = 1;
        assert!(value.validate().is_ok());
        value.state = DelegationApplyTransactionState::Applying;
        assert!(value.validate().is_err());
        value.recovery_owner = Some("worker".into());
        value.recovery_lease_until = Some("2026-08-28T00:01:00Z".into());
        assert!(value.validate().is_ok());
    }
}
