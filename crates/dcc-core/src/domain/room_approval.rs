//! The approval an agent's conversation waits on, for a room to ask it in
//! place (the person answers there instead of opening the conversation):
//! the oldest permission request of the open turn that is not resolved yet.
//! Read the same way by the desktop's room and the phone's.

use crate::domain::session::{SessionEventKind, SessionEventRecord, TurnId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingApproval {
    pub request_id: String,
    pub tool_name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub command: Option<String>,
    pub file: Option<String>,
}

impl PendingApproval {
    /// What the request is about, on one line: its command, else its file,
    /// else its description.
    pub fn detail(&self) -> Option<String> {
        self.command
            .clone()
            .or_else(|| self.file.clone())
            .or_else(|| self.description.clone())
            .map(|detail| detail.lines().next().unwrap_or_default().to_string())
            .filter(|detail| !detail.is_empty())
    }
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

/// Whether "Sempre permitir" fits the tool: a connector's, not DCC's own.
pub fn may_always_allow(tool_name: &str) -> bool {
    tool_name.starts_with("mcp__dcc-") && !tool_name.starts_with("mcp__dcc-browser-webview__")
}

/// A connector's tool by its own name (`slack_search_channels`), not the
/// long name the provider sees (`mcp__dcc-<id>__slack_search_channels`).
pub fn short_tool(tool_name: &str) -> &str {
    tool_name
        .strip_prefix("mcp__")
        .and_then(|rest| rest.split_once("__"))
        .map_or(tool_name, |(_, tool)| tool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::SessionId;

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
        let pending = pending_approval(&events).unwrap();
        assert_eq!(pending.tool_name, "slack_send_message");
        assert_eq!(pending.detail(), None);
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

    #[test]
    fn a_tool_reads_by_its_own_name() {
        assert_eq!(
            short_tool("mcp__dcc-slack__slack_send_message"),
            "slack_send_message"
        );
        assert_eq!(short_tool("Bash"), "Bash");
        assert!(may_always_allow("mcp__dcc-slack__slack_search"));
        assert!(!may_always_allow("mcp__dcc-browser-webview__click"));
        assert!(!may_always_allow("Bash"));
    }
}
