//! The question an agent's conversation waits on (the provider's ask-user
//! tool, recorded as `turn_user_input_requested`), for a room to ask it in
//! place: the oldest one of the open turn not answered yet, with its
//! questions as the conversation shows them. Read the same way by the
//! desktop's room and the phone's, beside `room_approval`.

use crate::domain::session::{SessionEventKind, SessionEventRecord, TurnId};
use crate::ports::provider::ProviderUserInputQuestion;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingQuestion {
    pub request_id: String,
    /// The agent's questions, the blank ones left out.
    pub questions: Vec<ProviderUserInputQuestion>,
}

pub fn pending_question(events: &[SessionEventRecord]) -> Option<PendingQuestion> {
    let mut open_turn: Option<&TurnId> = None;
    let mut pending: Vec<PendingQuestion> = Vec::new();
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
            SessionEventKind::TurnUserInputRequested {
                turn_id,
                request_id,
                questions,
            } if open_turn == Some(turn_id) => {
                let questions: Vec<ProviderUserInputQuestion> = questions
                    .iter()
                    .filter(|question| !question.question.trim().is_empty())
                    .cloned()
                    .collect();
                if !questions.is_empty() {
                    pending.push(PendingQuestion {
                        request_id: request_id.clone(),
                        questions,
                    });
                }
            }
            SessionEventKind::TurnUserInputResolved { request_id, .. } => {
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
    use crate::domain::session::SessionId;
    use crate::ports::provider::{ProviderUserInputAnswer, ProviderUserInputOption};

    fn event(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("e-{sequence}"),
            session_id: SessionId("s".into()),
            sequence,
            occurred_at: format!("2026-11-10T13:3{sequence}:00Z"),
            kind,
        }
    }

    fn started(sequence: u64, turn: &str) -> SessionEventRecord {
        event(
            sequence,
            serde_json::from_value::<SessionEventKind>(serde_json::json!({
                "type": "turn_started", "turnId": turn, "prompt": "publica",
                "planMode": false, "model": null, "evidence": null,
                "retryOfTurnId": null, "approvalPolicy": "ask"
            }))
            .unwrap(),
        )
    }

    fn asked(sequence: u64, turn: &str, request: &str, texts: &[&str]) -> SessionEventRecord {
        event(
            sequence,
            SessionEventKind::TurnUserInputRequested {
                turn_id: TurnId(turn.into()),
                request_id: request.into(),
                questions: texts
                    .iter()
                    .enumerate()
                    .map(|(index, text)| ProviderUserInputQuestion {
                        id: format!("q{index}"),
                        header: "Destino".into(),
                        question: (*text).into(),
                        options: vec![ProviderUserInputOption {
                            label: "staging".into(),
                            description: String::new(),
                        }],
                    })
                    .collect(),
            },
        )
    }

    fn answered(sequence: u64, request: &str) -> SessionEventRecord {
        event(
            sequence,
            SessionEventKind::TurnUserInputResolved {
                turn_id: TurnId("t".into()),
                request_id: request.into(),
                answers: vec![ProviderUserInputAnswer {
                    question: "Qual ambiente?".into(),
                    answer: "staging".into(),
                }],
            },
        )
    }

    #[test]
    fn the_open_question_is_the_one_asked_until_answered() {
        let mut events = vec![
            started(1, "t"),
            asked(2, "t", "q-1", &["Qual ambiente?", "  "]),
        ];
        let pending = pending_question(&events).unwrap();
        assert_eq!(pending.request_id, "q-1");
        // The blank question is left out.
        assert_eq!(pending.questions.len(), 1);
        assert_eq!(pending.questions[0].options[0].label, "staging");
        events.push(answered(3, "q-1"));
        assert_eq!(pending_question(&events), None);
    }

    #[test]
    fn the_oldest_of_two_shows_and_the_next_follows() {
        let mut events = vec![
            started(1, "t"),
            asked(2, "t", "q-1", &["Qual ambiente?"]),
            asked(3, "t", "q-2", &["Posso publicar?"]),
        ];
        assert_eq!(pending_question(&events).unwrap().request_id, "q-1");
        events.push(answered(4, "q-1"));
        assert_eq!(pending_question(&events).unwrap().request_id, "q-2");
    }

    #[test]
    fn a_question_of_an_ended_or_older_turn_does_not_wait() {
        let ended = vec![
            started(1, "t"),
            asked(2, "t", "q-1", &["Qual ambiente?"]),
            event(
                3,
                SessionEventKind::TurnCompleted {
                    turn_id: TurnId("t".into()),
                },
            ),
        ];
        assert_eq!(pending_question(&ended), None);
        let older = vec![asked(1, "old", "q-0", &["Antes?"]), started(2, "t")];
        assert_eq!(pending_question(&older), None);
        // Only blank questions: nothing to ask.
        let blank = vec![started(1, "t"), asked(2, "t", "q-1", &[" "])];
        assert_eq!(pending_question(&blank), None);
    }
}
