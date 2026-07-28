use async_trait::async_trait;
use tauri::{AppHandle, Emitter};

use dcc_core::{ports::events::CoreEvent, CoreError, Result};

pub const PHASE_0A_EVENT_PREFIX: &str = "dcc/phase-0a";
pub const WORKSPACE_EVENT_PREFIX: &str = "dcc/workspace";
pub const SESSION_EVENT_PREFIX: &str = "dcc/session";

pub(crate) fn core_event_name(event: &CoreEvent) -> String {
    match event {
        CoreEvent::WorkspacePrepared { .. } => format!("{WORKSPACE_EVENT_PREFIX}/prepared"),
        CoreEvent::WorkspaceReady { .. } => format!("{WORKSPACE_EVENT_PREFIX}/ready"),
        CoreEvent::SessionStarted { .. } => format!("{SESSION_EVENT_PREFIX}/started"),
        CoreEvent::SessionCompleted { .. } => format!("{SESSION_EVENT_PREFIX}/completed"),
        CoreEvent::SessionAborted { .. } => format!("{SESSION_EVENT_PREFIX}/aborted"),
        CoreEvent::SessionResumed { .. } => format!("{SESSION_EVENT_PREFIX}/resumed"),
        CoreEvent::SessionMcpRuntimeStatusChanged { .. } => {
            format!("{SESSION_EVENT_PREFIX}/mcp/runtime-status")
        }
        CoreEvent::SessionTurnStarted { .. } => format!("{SESSION_EVENT_PREFIX}/turn/started"),
        CoreEvent::SessionTurnDelta { .. } => format!("{SESSION_EVENT_PREFIX}/turn/delta"),
        CoreEvent::SessionTurnReasoningStarted { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/reasoning/started")
        }
        CoreEvent::SessionTurnReasoningDelta { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/reasoning/delta")
        }
        CoreEvent::SessionTurnReasoningCompleted { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/reasoning/completed")
        }
        CoreEvent::SessionTurnToolCallStarted { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/tool-call/started")
        }
        CoreEvent::SessionTurnToolCallDelta { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/tool-call/delta")
        }
        CoreEvent::SessionTurnToolCallCompleted { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/tool-call/completed")
        }
        CoreEvent::SessionTurnToolCallFailed { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/tool-call/failed")
        }
        CoreEvent::SessionTurnUserInputRequested { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/user-input/requested")
        }
        CoreEvent::SessionTurnUserInputResolved { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/user-input/resolved")
        }
        CoreEvent::SessionTurnPermissionRequested { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/permission/requested")
        }
        CoreEvent::SessionTurnPermissionResolved { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/permission/resolved")
        }
        CoreEvent::SessionTurnCompleted { .. } => {
            format!("{SESSION_EVENT_PREFIX}/turn/completed")
        }
        CoreEvent::SessionTurnAborted { .. } => format!("{SESSION_EVENT_PREFIX}/turn/aborted"),
        CoreEvent::SessionCheckpointCreated { .. } => {
            format!("{SESSION_EVENT_PREFIX}/checkpoint/created")
        }
        CoreEvent::SessionPlanApproved { .. } => {
            format!("{SESSION_EVENT_PREFIX}/plan/approved")
        }
        CoreEvent::SessionPlanHandedOff { .. } => {
            format!("{SESSION_EVENT_PREFIX}/plan/handed-off")
        }
        CoreEvent::SessionDelegationRequested { .. } => {
            format!("{SESSION_EVENT_PREFIX}/delegation/requested")
        }
        CoreEvent::SessionDelegationStarted { .. } => {
            format!("{SESSION_EVENT_PREFIX}/delegation/started")
        }
        CoreEvent::SessionDelegationDelta { .. } => {
            format!("{SESSION_EVENT_PREFIX}/delegation/delta")
        }
        CoreEvent::SessionDelegationCompleted { .. } => {
            format!("{SESSION_EVENT_PREFIX}/delegation/completed")
        }
        CoreEvent::SessionDelegationFailed { .. } => {
            format!("{SESSION_EVENT_PREFIX}/delegation/failed")
        }
        CoreEvent::SessionDelegationCancelled { .. } => {
            format!("{SESSION_EVENT_PREFIX}/delegation/cancelled")
        }
    }
}

#[derive(Clone)]
pub struct TauriEventBus {
    app: AppHandle,
}

impl TauriEventBus {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl dcc_core::ports::EventBus for TauriEventBus {
    async fn publish(&self, event: CoreEvent) -> Result<()> {
        let event_name = core_event_name(&event);

        self.app
            .emit(&event_name, &event)
            .map_err(|error| CoreError::EventBus(error.to_string()))?;

        self.app
            .emit("dcc:core-event", &event)
            .map_err(|error| CoreError::EventBus(error.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_event_names_do_not_use_dots() {
        let event = CoreEvent::WorkspacePrepared {
            workspace_id: "workspace-1".to_string(),
            project_id: "project-1".to_string(),
            worktree_path: "/tmp/project".to_string(),
        };

        let name = core_event_name(&event);

        assert_eq!(name, "dcc/workspace/prepared");
        assert!(!name.contains('.'));
    }

    #[test]
    fn mcp_runtime_status_uses_a_session_scoped_event_name() {
        let event = CoreEvent::SessionMcpRuntimeStatusChanged {
            session_id: "session-1".to_string(),
            statuses: Vec::new(),
        };

        assert_eq!(core_event_name(&event), "dcc/session/mcp/runtime-status");
    }
}
