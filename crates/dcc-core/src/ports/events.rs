use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Result;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CoreEvent {
    WorkspacePrepared {
        workspace_id: String,
        project_id: String,
        worktree_path: String,
    },
    WorkspaceReady {
        workspace_id: String,
        project_id: String,
        worktree_path: String,
    },
    SessionStarted {
        session_id: String,
        workspace_id: String,
        project_id: String,
        provider_id: String,
        model: Option<String>,
    },
    SessionCompleted {
        session_id: String,
    },
    SessionAborted {
        session_id: String,
        reason: Option<String>,
    },
    SessionResumed {
        session_id: String,
    },
    SessionTurnStarted {
        session_id: String,
        turn_id: String,
        prompt: String,
        plan_mode: Option<bool>,
    },
    SessionTurnDelta {
        session_id: String,
        turn_id: String,
        content: String,
    },
    SessionTurnReasoningStarted {
        session_id: String,
        turn_id: String,
        reasoning_id: String,
        label: Option<String>,
    },
    SessionTurnReasoningDelta {
        session_id: String,
        turn_id: String,
        reasoning_id: String,
        content: String,
    },
    SessionTurnReasoningCompleted {
        session_id: String,
        turn_id: String,
        reasoning_id: String,
    },
    SessionTurnToolCallStarted {
        session_id: String,
        turn_id: String,
        tool_call_id: String,
        action: String,
        command: Option<String>,
        file: Option<String>,
    },
    SessionTurnToolCallDelta {
        session_id: String,
        turn_id: String,
        tool_call_id: String,
        content: String,
    },
    SessionTurnToolCallCompleted {
        session_id: String,
        turn_id: String,
        tool_call_id: String,
    },
    SessionTurnToolCallFailed {
        session_id: String,
        turn_id: String,
        tool_call_id: String,
        reason: Option<String>,
    },
    SessionTurnCompleted {
        session_id: String,
        turn_id: String,
    },
    SessionTurnAborted {
        session_id: String,
        turn_id: String,
        reason: Option<String>,
    },
    SessionCheckpointCreated {
        session_id: String,
        checkpoint_id: String,
        label: String,
    },
}

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: CoreEvent) -> Result<()>;
}
