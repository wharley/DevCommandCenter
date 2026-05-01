use async_trait::async_trait;
use tauri::{AppHandle, Emitter};

use dcc_core::{ports::events::CoreEvent, CoreError, Result};

pub const PHASE_0A_EVENT_PREFIX: &str = "dcc.phase-0a";
pub const WORKSPACE_EVENT_PREFIX: &str = "dcc.workspace";
pub const SESSION_EVENT_PREFIX: &str = "dcc.session";

pub(crate) fn core_event_name(event: &CoreEvent) -> String {
	match event {
		CoreEvent::WorkspacePrepared { .. } => format!("{WORKSPACE_EVENT_PREFIX}.prepared"),
		CoreEvent::WorkspaceReady { .. } => format!("{WORKSPACE_EVENT_PREFIX}.ready"),
		CoreEvent::SessionStarted { .. } => format!("{SESSION_EVENT_PREFIX}.started"),
		CoreEvent::SessionCompleted { .. } => format!("{SESSION_EVENT_PREFIX}.completed"),
		CoreEvent::SessionAborted { .. } => format!("{SESSION_EVENT_PREFIX}.aborted"),
		CoreEvent::SessionResumed { .. } => format!("{SESSION_EVENT_PREFIX}.resumed"),
		CoreEvent::SessionTurnStarted { .. } => format!("{SESSION_EVENT_PREFIX}.turn.started"),
		CoreEvent::SessionTurnCompleted { .. } => {
			format!("{SESSION_EVENT_PREFIX}.turn.completed")
		}
		CoreEvent::SessionTurnAborted { .. } => format!("{SESSION_EVENT_PREFIX}.turn.aborted"),
		CoreEvent::SessionCheckpointCreated { .. } => {
			format!("{SESSION_EVENT_PREFIX}.checkpoint.created")
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
