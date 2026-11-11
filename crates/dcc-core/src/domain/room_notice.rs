//! What a resident agent's room conversation gives the person to hear about
//! while they look elsewhere (Tucano's notices): the approval or question it
//! waits on, else how its last turn ended. Only turns the person started, or
//! passed along a chain, end in a notice: a routine's or a source's wake and
//! DCC's own wakes (a routine's, a source's) never do, nor a quiet answer; a
//! task's status (`[DCC] Task "…" ended a turn`) does: the agent's answer to
//! it is the result the person waits for.

use crate::domain::agent_routine::is_quiet;
use crate::domain::room_approval::{pending_approval, short_tool};
use crate::domain::room_question::pending_question;
use crate::domain::session::{SessionEventKind, SessionEventRecord};
use crate::domain::team_room_live::{answer_text, retry_prompt};
use crate::domain::timeline::{project_session, Role};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomNotice {
    /// A permission request: the tool by its short name and what it acts on.
    Approval {
        request_id: String,
        tool: String,
        detail: Option<String>,
    },
    /// The agent's first question.
    Question { request_id: String, text: String },
    /// The turn finished: the answer's first line.
    Done { turn_id: String, first_line: String },
    /// The turn stopped or failed before it finished.
    Stopped {
        turn_id: String,
        reason: Option<String>,
    },
}

impl RoomNotice {
    /// What it is about, so the same request or turn is told once.
    pub fn key(&self) -> &str {
        match self {
            Self::Approval { request_id, .. } | Self::Question { request_id, .. } => request_id,
            Self::Done { turn_id, .. } | Self::Stopped { turn_id, .. } => turn_id,
        }
    }

    /// Whether the agent waits on the person (the Dock bounces for these).
    pub fn asks(&self) -> bool {
        matches!(self, Self::Approval { .. } | Self::Question { .. })
    }
}

/// The notice the last turn's events hold. `waiting`: the conversation
/// waits on the person, so its open request is read; otherwise how the turn
/// ended. `events` may start at the last turn's `turn_started`.
pub fn notice_of(
    events: &[SessionEventRecord],
    session_id: &str,
    waiting: bool,
) -> Option<RoomNotice> {
    if waiting {
        if let Some(approval) = pending_approval(events) {
            return Some(RoomNotice::Approval {
                tool: short_tool(&approval.tool_name).to_string(),
                detail: approval.detail(),
                request_id: approval.request_id,
            });
        }
        let question = pending_question(events)?;
        let text = question.questions.first()?.question.trim().to_string();
        return Some(RoomNotice::Question {
            request_id: question.request_id,
            text,
        });
    }
    turn_end(events, session_id)
}

/// How the last turn ended, when the person started it.
fn turn_end(events: &[SessionEventRecord], session_id: &str) -> Option<RoomNotice> {
    let start = events
        .iter()
        .rposition(|event| matches!(event.kind, SessionEventKind::TurnStarted { .. }))?;
    let messages = project_session(&events[start..], session_id);
    let prompt = messages
        .iter()
        .rev()
        .find(|message| message.role == Role::User && message.turn_id.is_some())?;
    // The person's words, a pass, or one of its tasks reporting back.
    if retry_prompt(prompt).is_none() && prompt.hand_back.is_none() {
        return None;
    }
    let turn = prompt.turn_id.clone()?;
    let rows: Vec<_> = messages
        .iter()
        .filter(|message| {
            message.role == Role::Assistant && message.turn_id.as_deref() == Some(turn.as_str())
        })
        .collect();
    if !rows.iter().any(|row| row.turn_settled) {
        return None;
    }
    if let Some(status) = rows.iter().find_map(|row| row.status.as_ref()) {
        // A usage limit has the room's own card and its "continue".
        return (!status.usage_limited).then(|| RoomNotice::Stopped {
            turn_id: turn.clone(),
            reason: status.reason.clone(),
        });
    }
    let answer = rows
        .iter()
        .rev()
        .find_map(|row| answer_text(row))
        .unwrap_or_default();
    if is_quiet(&answer) {
        return None;
    }
    Some(RoomNotice::Done {
        turn_id: turn,
        first_line: first_line(&answer),
    })
}

/// The answer's first written line, without a heading's or a list's mark.
pub fn first_line(answer: &str) -> String {
    answer
        .lines()
        .map(|line| line.trim().trim_start_matches(['#', '>', '-', '*']).trim())
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::{SessionId, TurnId};
    use crate::ports::provider::ProviderUserInputQuestion;

    fn event(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("e-{sequence}"),
            session_id: SessionId("s".into()),
            sequence,
            occurred_at: format!("2026-11-11T10:00:{sequence:02}Z"),
            kind,
        }
    }

    fn json(sequence: u64, value: serde_json::Value) -> SessionEventRecord {
        event(sequence, serde_json::from_value(value).unwrap())
    }

    fn started(sequence: u64, prompt: &str) -> SessionEventRecord {
        json(
            sequence,
            serde_json::json!({
                "type": "turn_started", "turnId": "t", "prompt": prompt,
                "planMode": false, "model": null, "evidence": null,
                "retryOfTurnId": null, "approvalPolicy": "ask"
            }),
        )
    }

    fn answer(sequence: u64, text: &str) -> SessionEventRecord {
        json(
            sequence,
            serde_json::json!({ "type": "turn_delta", "turnId": "t", "content": text }),
        )
    }

    fn completed(sequence: u64) -> SessionEventRecord {
        json(
            sequence,
            serde_json::json!({ "type": "turn_completed", "turnId": "t" }),
        )
    }

    fn ended(prompt: &str, text: &str) -> Option<RoomNotice> {
        let events = [started(1, prompt), answer(2, text), completed(3)];
        notice_of(&events, "s", false)
    }

    #[test]
    fn a_turn_the_person_started_ends_in_its_first_line() {
        assert_eq!(
            ended("corrige o cache", "## Pronto\n\nTroquei o TTL."),
            Some(RoomNotice::Done {
                turn_id: "t".into(),
                first_line: "Pronto".into()
            })
        );
        let passed = "↪ Suporte → Dev: @Dev corrige\n\nWhat Suporte said in the team room:\nx";
        assert!(
            ended(passed, "Feito.").is_some(),
            "a chain the person started"
        );
    }

    #[test]
    fn the_answer_to_a_tasks_status_is_told() {
        let status = "[DCC] Task \"Levantar merges\" ended a turn — Codex (task t-1).\n\nConfirmados 19 PRs.";
        assert!(matches!(
            ended(status, "Confirmados 19 PRs mesclados e em produção."),
            Some(RoomNotice::Done { .. })
        ));
    }

    #[test]
    fn a_wake_or_a_quiet_answer_tells_nothing() {
        assert_eq!(
            ended("[DCC] Routine \"Bom dia\" woke you.", "Tudo ok."),
            None
        );
        assert_eq!(ended("[DCC] Source \"Sentry\" has news.", "Nada."), None);
        assert_eq!(ended("olha os erros", "NOTHING_TO_REPORT"), None);
    }

    #[test]
    fn a_turn_still_open_tells_nothing_and_a_stop_says_why() {
        let open = [started(1, "corrige"), answer(2, "Lendo…")];
        assert_eq!(notice_of(&open, "s", false), None);
        let stopped = [
            started(1, "corrige"),
            json(
                2,
                serde_json::json!({ "type": "turn_aborted", "turnId": "t", "reason": "Interrompido" }),
            ),
        ];
        assert_eq!(
            notice_of(&stopped, "s", false),
            Some(RoomNotice::Stopped {
                turn_id: "t".into(),
                reason: Some("Interrompido".into())
            })
        );
    }

    #[test]
    fn a_waiting_turn_tells_its_approval_else_its_question() {
        let approval = event(
            2,
            SessionEventKind::TurnPermissionRequested {
                turn_id: TurnId("t".into()),
                request_id: "r-1".into(),
                tool_name: "mcp__dcc-slack__slack_send_message".into(),
                title: None,
                description: None,
                command: None,
                file: Some("src/cache.rs".into()),
            },
        );
        let notice = notice_of(&[started(1, "avisa"), approval], "s", true).unwrap();
        assert_eq!(
            notice,
            RoomNotice::Approval {
                request_id: "r-1".into(),
                tool: "slack_send_message".into(),
                detail: Some("src/cache.rs".into())
            }
        );
        assert!(notice.asks());
        assert_eq!(notice.key(), "r-1");
        let question = event(
            2,
            SessionEventKind::TurnUserInputRequested {
                turn_id: TurnId("t".into()),
                request_id: "q-1".into(),
                questions: vec![ProviderUserInputQuestion {
                    id: "q".into(),
                    header: "Destino".into(),
                    question: " Para qual ambiente? ".into(),
                    options: Vec::new(),
                }],
            },
        );
        assert_eq!(
            notice_of(&[started(1, "publica"), question], "s", true),
            Some(RoomNotice::Question {
                request_id: "q-1".into(),
                text: "Para qual ambiente?".into()
            })
        );
        assert_eq!(notice_of(&[started(1, "x")], "s", true), None);
    }
}
