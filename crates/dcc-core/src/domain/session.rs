use serde::{Deserialize, Serialize};
use specta::Type;

use super::{project::ProjectId, workspace::WorkspaceId};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct SessionId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct TurnId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct CheckpointId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
	Draft,
	Active,
	Completed,
	Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
	Running,
	Completed,
	Aborted,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
	pub id: TurnId,
	pub session_id: SessionId,
	pub role: String,
	pub content: String,
	pub state: TurnState,
	pub created_at: String,
	pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
	pub id: CheckpointId,
	pub session_id: SessionId,
	pub label: String,
	pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Session {
	pub id: SessionId,
	pub project_id: ProjectId,
	pub workspace_id: WorkspaceId,
	pub provider_id: String,
	pub state: SessionState,
	pub created_at: String,
	pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEventKind {
	SessionStarted {
		#[serde(rename = "workspaceId")]
		workspace_id: WorkspaceId,
		#[serde(rename = "projectId")]
		project_id: ProjectId,
		#[serde(rename = "providerId")]
		provider_id: String,
	},
	TurnStarted {
		#[serde(rename = "turnId")]
		turn_id: TurnId,
		prompt: String,
	},
	TurnCompleted {
		#[serde(rename = "turnId")]
		turn_id: TurnId,
	},
	TurnAborted {
		#[serde(rename = "turnId")]
		turn_id: TurnId,
		reason: Option<String>,
	},
	CheckpointCreated {
		#[serde(rename = "checkpointId")]
		checkpoint_id: CheckpointId,
		label: String,
	},
	SessionCompleted,
	SessionAborted {
		reason: Option<String>,
	},
	SessionResumed,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionEventRecord {
	pub event_id: String,
	pub session_id: SessionId,
	pub sequence: u64,
	pub occurred_at: String,
	pub kind: SessionEventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionProjection {
	pub session_id: SessionId,
	pub project_id: ProjectId,
	pub workspace_id: WorkspaceId,
	pub provider_id: String,
	pub state: SessionState,
	pub active_turn_id: Option<TurnId>,
	pub turn_count: u32,
	pub checkpoint_count: u32,
	pub created_at: String,
	pub updated_at: String,
}

impl SessionProjection {
	pub fn new(
		session_id: SessionId,
		project_id: ProjectId,
		workspace_id: WorkspaceId,
		provider_id: String,
		occurred_at: String,
	) -> Self {
		Self {
			session_id,
			project_id,
			workspace_id,
			provider_id,
			state: SessionState::Draft,
			active_turn_id: None,
			turn_count: 0,
			checkpoint_count: 0,
			created_at: occurred_at.clone(),
			updated_at: occurred_at,
		}
	}

	pub fn apply_event(&mut self, event: &SessionEventRecord) {
		self.updated_at = event.occurred_at.clone();

		match &event.kind {
			SessionEventKind::SessionStarted {
				workspace_id,
				project_id,
				provider_id,
			} => {
				self.workspace_id = workspace_id.clone();
				self.project_id = project_id.clone();
				self.provider_id = provider_id.clone();
				self.state = SessionState::Active;
			}
			SessionEventKind::TurnStarted { turn_id, .. } => {
				self.state = SessionState::Active;
				self.active_turn_id = Some(turn_id.clone());
			}
			SessionEventKind::TurnCompleted { turn_id } => {
				self.turn_count = self.turn_count.saturating_add(1);
				if self.active_turn_id.as_ref() == Some(turn_id) {
					self.active_turn_id = None;
				}
			}
			SessionEventKind::TurnAborted { turn_id, .. } => {
				if self.active_turn_id.as_ref() == Some(turn_id) {
					self.active_turn_id = None;
				}
			}
			SessionEventKind::CheckpointCreated { .. } => {
				self.checkpoint_count = self.checkpoint_count.saturating_add(1);
			}
			SessionEventKind::SessionCompleted => {
				self.state = SessionState::Completed;
				self.active_turn_id = None;
			}
			SessionEventKind::SessionAborted { .. } => {
				self.state = SessionState::Aborted;
				self.active_turn_id = None;
			}
			SessionEventKind::SessionResumed => {
				self.state = SessionState::Active;
			}
		}
	}

	pub fn fold(events: &[SessionEventRecord]) -> Option<Self> {
		let first = events.first()?;
		let mut projection = match &first.kind {
			SessionEventKind::SessionStarted {
				workspace_id,
				project_id,
				provider_id,
			} => Self::new(
				first.session_id.clone(),
				project_id.clone(),
				workspace_id.clone(),
				provider_id.clone(),
				first.occurred_at.clone(),
			),
			_ => Self::new(
				first.session_id.clone(),
				ProjectId(String::new()),
				WorkspaceId(String::new()),
				String::new(),
				first.occurred_at.clone(),
			),
		};

		for event in events {
			projection.apply_event(event);
		}

		Some(projection)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn event(sequence: u64, occurred_at: &str, kind: SessionEventKind) -> SessionEventRecord {
		SessionEventRecord {
			event_id: format!("evt-{sequence}"),
			session_id: SessionId("session-1".to_string()),
			sequence,
			occurred_at: occurred_at.to_string(),
			kind,
		}
	}

	#[test]
	fn projection_tracks_turns_checkpoints_and_terminal_state() {
		let events = vec![
			event(
				1,
				"2026-05-01T10:00:00Z",
				SessionEventKind::SessionStarted {
					workspace_id: WorkspaceId("workspace-1".to_string()),
					project_id: ProjectId("project-1".to_string()),
					provider_id: "codex".to_string(),
				},
			),
			event(
				2,
				"2026-05-01T10:00:05Z",
				SessionEventKind::TurnStarted {
					turn_id: TurnId("turn-1".to_string()),
					prompt: "Create shell".to_string(),
				},
			),
			event(
				3,
				"2026-05-01T10:00:10Z",
				SessionEventKind::TurnCompleted {
					turn_id: TurnId("turn-1".to_string()),
				},
			),
			event(
				4,
				"2026-05-01T10:00:12Z",
				SessionEventKind::CheckpointCreated {
					checkpoint_id: CheckpointId("checkpoint-1".to_string()),
					label: "After turn".to_string(),
				},
			),
			event(
				5,
				"2026-05-01T10:00:20Z",
				SessionEventKind::SessionAborted {
					reason: Some("user stopped".to_string()),
				},
			),
			event(6, "2026-05-01T10:00:25Z", SessionEventKind::SessionResumed),
		];

		let projection = SessionProjection::fold(&events).expect("projection should exist");

		assert_eq!(projection.session_id.0, "session-1");
		assert_eq!(projection.project_id.0, "project-1");
		assert_eq!(projection.workspace_id.0, "workspace-1");
		assert_eq!(projection.provider_id, "codex");
		assert_eq!(projection.turn_count, 1);
		assert_eq!(projection.checkpoint_count, 1);
		assert_eq!(projection.active_turn_id, None);
		assert_eq!(projection.state, SessionState::Active);
		assert_eq!(projection.created_at, "2026-05-01T10:00:00Z");
		assert_eq!(projection.updated_at, "2026-05-01T10:00:25Z");
	}

	#[test]
	fn projection_resumes_after_aborted_state() {
		let events = vec![
			event(
				1,
				"2026-05-01T10:00:00Z",
				SessionEventKind::SessionStarted {
					workspace_id: WorkspaceId("workspace-1".to_string()),
					project_id: ProjectId("project-1".to_string()),
					provider_id: "codex".to_string(),
				},
			),
			event(
				2,
				"2026-05-01T10:00:05Z",
				SessionEventKind::SessionAborted {
					reason: Some("Stopped".to_string()),
				},
			),
			event(3, "2026-05-01T10:00:10Z", SessionEventKind::SessionResumed),
		];

		let projection = SessionProjection::fold(&events).expect("projection should exist");

		assert_eq!(projection.state, SessionState::Active);
	}

	#[test]
	fn fold_returns_none_for_empty_logs() {
		assert!(SessionProjection::fold(&[]).is_none());
	}
}
