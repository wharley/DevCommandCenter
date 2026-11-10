//! The approval an agent's conversation waits on, for the team room to ask
//! it in place (the person answers there instead of opening the
//! conversation): the oldest permission request of the open turn that is
//! not resolved yet.

use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, TurnId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingApproval {
    pub request_id: String,
    pub tool_name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub command: Option<String>,
    pub file: Option<String>,
}

pub fn pending_approval(events: &[SessionEventRecord]) -> Option<PendingApproval> {
    let mut open_turn: Option<&TurnId> = None;
    let mut pending: Vec<PendingApproval> = Vec::new();
    for event in events {
        match &event.kind {
            SessionEventKind::TurnStarted { turn_id, .. } => {
                open_turn = Some(turn_id);
                pending.clear();
            }
            SessionEventKind::TurnCompleted { turn_id }
            | SessionEventKind::TurnAborted { turn_id, .. }
                if open_turn == Some(turn_id) =>
            {
                open_turn = None;
                pending.clear();
            }
            SessionEventKind::TurnPermissionRequested {
                turn_id,
                request_id,
                tool_name,
                title,
                description,
                command,
                file,
            } if open_turn == Some(turn_id) => pending.push(PendingApproval {
                request_id: request_id.clone(),
                tool_name: tool_name.clone(),
                title: title.clone(),
                description: description.clone(),
                command: command.clone(),
                file: file.clone(),
            }),
            SessionEventKind::TurnPermissionResolved { request_id, .. } => {
                pending.retain(|request| request.request_id != *request_id);
            }
            SessionEventKind::SessionCompleted | SessionEventKind::SessionAborted { .. } => {
                open_turn = None;
                pending.clear();
            }
            _ => {}
        }
    }
    pending.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::session::SessionId;

    fn event(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("e-{sequence}"),
            session_id: SessionId("s".into()),
            sequence,
            occurred_at: format!("2026-11-10T13:3{sequence}:00Z"),
            kind,
        }
    }

    fn asked(sequence: u64, request: &str) -> SessionEventRecord {
        event(
            sequence,
            SessionEventKind::TurnPermissionRequested {
                turn_id: TurnId("t".into()),
                request_id: request.into(),
                tool_name: "slack_send_message".into(),
                title: Some("Approve MCP tool call".into()),
                description: None,
                command: None,
                file: None,
            },
        )
    }

    #[test]
    fn the_open_request_is_the_one_asked() {
        let started = serde_json::from_value::<SessionEventKind>(serde_json::json!({
            "type": "turn_started", "turnId": "t", "prompt": "avisa o Daniel",
            "planMode": false, "model": null, "evidence": null, "retryOfTurnId": null,
            "approvalPolicy": "ask"
        }))
        .unwrap();
        let mut events = vec![event(1, started), asked(2, "r-1")];
        assert_eq!(
            pending_approval(&events).map(|request| request.tool_name),
            Some("slack_send_message".to_string())
        );
        events.push(event(
            3,
            SessionEventKind::TurnPermissionResolved {
                turn_id: TurnId("t".into()),
                request_id: "r-1".into(),
                behavior: "allow".into(),
            },
        ));
        assert_eq!(pending_approval(&events), None);
    }
}
