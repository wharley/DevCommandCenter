//! A delegation as the conversation's cards and the Session tab read it
//! from the stored record.

use dcc_core::domain::delegation::{Delegation, DelegationKind, DelegationStatus};

/// What a delegation card reads from the stored record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationRecord {
    pub id: String,
    pub status: DelegationStatus,
    /// `review` | `implement` | `explain` | `test` | `research`
    pub mode: String,
    pub created_at: String,
    /// The list's order (`updated_at DESC, created_at DESC`).
    pub updated_at: String,
    pub started_at: Option<String>,
    pub is_task: bool,
    pub title: Option<String>,
    pub instruction: String,
    pub result_summary: Option<String>,
    pub diff_summary: Option<String>,
    pub validation_summary: Option<String>,
    /// The turn that asked for it: delegations of one turn are compared.
    pub parent_turn_id: Option<String>,
    pub prompt: String,
    pub touched_files: Vec<String>,
    pub provider_id: String,
    pub model_id: Option<String>,
    pub child_session_id: Option<String>,
    /// `mode === "implement"`: it edits files.
    pub implement: bool,
    pub parent_session_id: String,
    /// The child's own folder (`workingDirectoryOverride`): an isolated
    /// worktree waiting for review, when it has one.
    pub worktree: Option<String>,
}

impl Default for DelegationRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            status: DelegationStatus::Draft,
            mode: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            started_at: None,
            is_task: false,
            title: None,
            instruction: String::new(),
            result_summary: None,
            diff_summary: None,
            validation_summary: None,
            parent_turn_id: None,
            prompt: String::new(),
            touched_files: Vec::new(),
            provider_id: String::new(),
            model_id: None,
            child_session_id: None,
            implement: false,
            parent_session_id: String::new(),
            worktree: None,
        }
    }
}

impl DelegationRecord {
    /// An implementation whose worktree waits for the person (`review_pending`).
    pub fn awaits_review(&self) -> bool {
        self.implement && self.status == DelegationStatus::ReviewPending
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChildSession {
    pub title: String,
    pub busy: bool,
    pub waiting: bool,
}

/// `delegationInstruction`: the task as written, without DCC's wrapping.
fn delegation_instruction(delegation: &Delegation) -> String {
    if let Some(stored) = delegation
        .instruction
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return stored.to_string();
    }
    const MARKER: &str = "\nInstruction:\n";
    let Some(start) = delegation.prompt.find(MARKER) else {
        return delegation.prompt.trim().to_string();
    };
    let rest = &delegation.prompt[start + MARKER.len()..];
    let end = [
        "Git context",
        "Mission spec",
        "Recent parent session context",
    ]
    .iter()
    .filter_map(|name| rest.find(&format!("\n\n{name}:\n")))
    .min();
    end.map_or(rest, |end| &rest[..end]).trim().to_string()
}

impl DelegationRecord {
    pub(super) fn from_delegation(delegation: &Delegation) -> Self {
        let id_of = |value: serde_json::Result<serde_json::Value>| {
            value
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default()
        };
        Self {
            id: delegation.id.0.clone(),
            status: delegation.status.clone(),
            mode: id_of(serde_json::to_value(&delegation.mode)),
            created_at: delegation.created_at.clone(),
            updated_at: delegation.updated_at.clone(),
            started_at: delegation.started_at.clone(),
            is_task: delegation.kind == DelegationKind::Task,
            title: delegation.title.clone(),
            instruction: delegation_instruction(delegation),
            result_summary: delegation.result_summary.clone(),
            diff_summary: delegation.diff_summary.clone(),
            validation_summary: delegation.validation_summary.clone(),
            parent_turn_id: delegation
                .parent_turn_id
                .as_ref()
                .map(|turn| turn.0.clone()),
            prompt: delegation.prompt.clone(),
            touched_files: delegation.touched_files.clone(),
            provider_id: delegation.target_provider_id.0.clone(),
            model_id: delegation.target_model_id.clone(),
            child_session_id: delegation.child_session_id.as_ref().map(|id| id.0.clone()),
            implement: delegation.mode == dcc_core::domain::delegation::DelegationMode::Implement,
            parent_session_id: delegation.parent_session_id.0.clone(),
            worktree: None,
        }
    }
}
