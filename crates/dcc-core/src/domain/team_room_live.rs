//! An agent's turn at work, for the team room (Tucano's `RunSteps`): while
//! a conversation's last turn is open, what it does now (the tool running,
//! else thinking, else reading the request), the steps so far and the answer
//! as it streams; once it ended stopped or failed, why and the prompt to
//! send again. The room's settled lines stay `team_room_lines`' own.

use crate::domain::session::{AssistantMessagePhase, SessionEventKind, SessionEventRecord};
use crate::domain::team_room_steps::step_of;
use crate::domain::timeline::activity::ToolKind;
use crate::domain::timeline::{project_session, Annotation, Role, TimelineMessage};

/// The turns DCC itself writes for an agent (`[DCC] Routine`, `[DCC]
/// Source`, a task's status): never offered again by the room.
const DCC_MARK: &str = "[DCC]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Running,
    Done,
    Failed,
}

/// One tool call of the open turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveStep {
    pub kind: ToolKind,
    /// The file, command or tool it acts on.
    pub target: Option<String>,
    pub state: StepState,
}

/// What the open turn does now, for its one-line label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doing {
    /// Nothing came back yet.
    Reading,
    /// Something came back, no tool runs now.
    Thinking,
    /// The step at this index runs.
    Tool(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveTurn {
    pub turn_id: String,
    /// When the turn started (RFC 3339).
    pub started_at: Option<String>,
    /// The answer streamed so far (not the agent's commentary).
    pub text: String,
    pub steps: Vec<LiveStep>,
    pub doing: Doing,
}

impl LiveTurn {
    /// The step running now, if any.
    pub fn running_step(&self) -> Option<&LiveStep> {
        match self.doing {
            Doing::Tool(index) => self.steps.get(index),
            _ => None,
        }
    }
}

/// A last turn that ended before it finished.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoppedTurn {
    pub turn_id: String,
    pub reason: Option<String>,
    pub ended_at: Option<String>,
    /// The prompt that started it, when the person sent or passed it; none
    /// for a turn DCC wrote (a routine's or a source's wake, a status).
    pub retry: Option<String>,
}

/// A conversation's last turn as the room shows it now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TurnNow {
    /// No turn, or the last one finished.
    #[default]
    Idle,
    Live(LiveTurn),
    Stopped(StoppedTurn),
}

impl TurnNow {
    pub fn live(&self) -> Option<&LiveTurn> {
        match self {
            TurnNow::Live(turn) => Some(turn),
            _ => None,
        }
    }

    pub fn stopped(&self) -> Option<&StoppedTurn> {
        match self {
            TurnNow::Stopped(turn) => Some(turn),
            _ => None,
        }
    }
}

/// The last turn of a projected conversation. A usage limit is not a stop
/// here: the room's limit card says it and offers to continue.
pub fn turn_now(messages: &[TimelineMessage]) -> TurnNow {
    let Some(prompt) = messages
        .iter()
        .rev()
        .find(|message| message.role == Role::User && message.turn_id.is_some())
    else {
        return TurnNow::Idle;
    };
    let turn = prompt.turn_id.as_deref().unwrap_or_default();
    let rows: Vec<&TimelineMessage> = messages
        .iter()
        .filter(|message| {
            message.role == Role::Assistant && message.turn_id.as_deref() == Some(turn)
        })
        .collect();
    if rows.iter().any(|row| row.turn_settled) {
        return rows
            .iter()
            .find_map(|row| {
                let status = row.status.as_ref().filter(|status| !status.usage_limited)?;
                Some(TurnNow::Stopped(StoppedTurn {
                    turn_id: turn.to_string(),
                    reason: status.reason.clone(),
                    ended_at: row.turn_ended_at.clone(),
                    retry: retry_prompt(prompt),
                }))
            })
            .unwrap_or_default();
    }
    TurnNow::Live(live_turn(turn, prompt, &rows))
}

/// `turn_now` of the conversation's events, projecting only those from the
/// last turn's start: what a poll of a running turn needs, cheap however
/// long the conversation is.
pub fn turn_now_of_events(events: &[SessionEventRecord], session_id: &str) -> TurnNow {
    let start = events
        .iter()
        .rposition(|event| matches!(event.kind, SessionEventKind::TurnStarted { .. }))
        .unwrap_or(0);
    turn_now(&project_session(&events[start..], session_id))
}

/// Whether `kind` only moves an open turn's own work (its text, reasoning,
/// tools): the room's settled lines cannot change with it.
pub fn moves_only_the_turn(kind: &SessionEventKind) -> bool {
    matches!(
        kind,
        SessionEventKind::TurnDelta { .. }
            | SessionEventKind::TurnAssistantMessageStarted { .. }
            | SessionEventKind::TurnAssistantMessageDelta { .. }
            | SessionEventKind::TurnAssistantMessageCompleted { .. }
            | SessionEventKind::TurnReasoningStarted { .. }
            | SessionEventKind::TurnReasoningDelta { .. }
            | SessionEventKind::TurnReasoningCompleted { .. }
            | SessionEventKind::TurnToolCallStarted { .. }
            | SessionEventKind::TurnToolCallDelta { .. }
            | SessionEventKind::TurnToolCallUpdated { .. }
            | SessionEventKind::TurnToolCallCompleted { .. }
            | SessionEventKind::TurnToolCallFailed { .. }
            | SessionEventKind::TurnModelEffective { .. }
            | SessionEventKind::TurnNativeSubagentActivity { .. }
            | SessionEventKind::TurnNativeSubagentModelRequested { .. }
            | SessionEventKind::TurnNativeSubagentModelConfirmed { .. }
    )
}

/// The person's own prompt (or one they passed), never DCC's.
pub(crate) fn retry_prompt(prompt: &TimelineMessage) -> Option<String> {
    let text = prompt.content.trim();
    let dcc = prompt.hand_back.is_some() || text.starts_with(DCC_MARK);
    (!dcc && !text.is_empty()).then(|| prompt.content.clone())
}

fn live_turn(turn: &str, prompt: &TimelineMessage, rows: &[&TimelineMessage]) -> LiveTurn {
    let steps: Vec<LiveStep> = rows
        .iter()
        .flat_map(|row| &row.annotations)
        .filter_map(|annotation| match annotation {
            Annotation::ToolCall(call) => {
                let step = step_of(call);
                Some(LiveStep {
                    kind: step.kind,
                    target: step.target,
                    state: if call.failure.is_some() {
                        StepState::Failed
                    } else if call.streaming {
                        StepState::Running
                    } else {
                        StepState::Done
                    },
                })
            }
            _ => None,
        })
        .collect();
    let text = rows
        .iter()
        .rev()
        .find_map(|row| answer_text(row))
        .unwrap_or_default();
    let doing = match steps
        .iter()
        .rposition(|step| step.state == StepState::Running)
    {
        Some(index) => Doing::Tool(index),
        None if rows.is_empty() => Doing::Reading,
        None => Doing::Thinking,
    };
    LiveTurn {
        turn_id: turn.to_string(),
        started_at: prompt
            .created_at
            .clone()
            .or_else(|| rows.first().and_then(|row| row.turn_started_at.clone())),
        text,
        steps,
        doing,
    }
}

/// A running row's answer so far: its own text (one stream for the whole
/// turn), or — a turn of several messages, folded while it runs — the
/// message being written, unless that one is commentary.
pub(crate) fn answer_text(row: &TimelineMessage) -> Option<String> {
    if row.assistant_phase == Some(AssistantMessagePhase::Commentary) {
        return None;
    }
    if !row.content.trim().is_empty() {
        return Some(row.content.clone());
    }
    let commentary = |streaming_only: bool| {
        row.annotations
            .iter()
            .rev()
            .find_map(|annotation| match annotation {
                Annotation::Commentary {
                    content, streaming, ..
                } if (*streaming || !streaming_only) && !content.trim().is_empty() => {
                    Some(content.clone())
                }
                _ => None,
            })
    };
    commentary(true).or_else(|| commentary(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::SessionId;

    fn event(sequence: u64, json: serde_json::Value) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("e-{sequence}"),
            session_id: SessionId("s".into()),
            sequence,
            occurred_at: format!("2026-11-10T10:00:{sequence:02}Z"),
            kind: serde_json::from_value(json).unwrap(),
        }
    }

    fn started(sequence: u64, turn: &str, prompt: &str) -> SessionEventRecord {
        event(
            sequence,
            serde_json::json!({
                "type": "turn_started", "turnId": turn, "prompt": prompt,
                "planMode": false, "model": null, "evidence": null,
                "retryOfTurnId": null, "approvalPolicy": "ask"
            }),
        )
    }

    fn delta(sequence: u64, turn: &str, text: &str) -> SessionEventRecord {
        event(
            sequence,
            serde_json::json!({ "type": "turn_delta", "turnId": turn, "content": text }),
        )
    }

    fn tool(sequence: u64, turn: &str, id: &str, action: &str, file: &str) -> SessionEventRecord {
        event(
            sequence,
            serde_json::json!({
                "type": "turn_tool_call_started", "turnId": turn, "toolCallId": id,
                "action": action, "command": null, "file": file
            }),
        )
    }

    fn tool_done(sequence: u64, turn: &str, id: &str) -> SessionEventRecord {
        event(
            sequence,
            serde_json::json!({ "type": "turn_tool_call_completed", "turnId": turn, "toolCallId": id }),
        )
    }

    fn tool_failed(sequence: u64, turn: &str, id: &str) -> SessionEventRecord {
        event(
            sequence,
            serde_json::json!({
                "type": "turn_tool_call_failed", "turnId": turn, "toolCallId": id,
                "reason": "exit 1"
            }),
        )
    }

    fn completed(sequence: u64, turn: &str) -> SessionEventRecord {
        event(
            sequence,
            serde_json::json!({ "type": "turn_completed", "turnId": turn }),
        )
    }

    fn aborted(sequence: u64, turn: &str, reason: &str) -> SessionEventRecord {
        event(
            sequence,
            serde_json::json!({ "type": "turn_aborted", "turnId": turn, "reason": reason }),
        )
    }

    fn now(events: &[SessionEventRecord]) -> TurnNow {
        turn_now(&project_session(events, "s"))
    }

    #[test]
    fn no_turn_or_a_finished_one_is_idle() {
        assert_eq!(now(&[]), TurnNow::Idle);
        let events = [
            started(1, "t1", "oi"),
            delta(2, "t1", "Olá."),
            completed(3, "t1"),
        ];
        assert_eq!(now(&events), TurnNow::Idle);
    }

    #[test]
    fn a_turn_just_sent_reads_the_request_then_thinks() {
        let events = vec![started(1, "t1", "revisa o PR")];
        let turn = now(&events).live().cloned().unwrap();
        assert_eq!(turn.turn_id, "t1");
        assert_eq!(turn.doing, Doing::Reading);
        assert_eq!(turn.started_at.as_deref(), Some("2026-11-10T10:00:01Z"));
        assert!(turn.text.is_empty() && turn.steps.is_empty());

        let mut thinking = events.clone();
        thinking.push(event(
            2,
            serde_json::json!({
                "type": "turn_reasoning_started", "turnId": "t1",
                "reasoningId": "r1", "label": null
            }),
        ));
        assert_eq!(now(&thinking).live().unwrap().doing, Doing::Thinking);
    }

    #[test]
    fn a_running_tool_is_what_it_does_and_the_steps_keep_their_state() {
        let events = [
            started(1, "t1", "roda os testes"),
            tool(2, "t1", "c1", "Read", "/a/src/lib.rs"),
            tool_done(3, "t1", "c1"),
            tool(4, "t1", "c2", "Bash", ""),
            tool_failed(5, "t1", "c2"),
            tool(6, "t1", "c3", "Edit", "/a/src/main.rs"),
        ];
        let turn = now(&events).live().cloned().unwrap();
        let states: Vec<StepState> = turn.steps.iter().map(|step| step.state).collect();
        assert_eq!(
            states,
            [StepState::Done, StepState::Failed, StepState::Running]
        );
        assert_eq!(turn.doing, Doing::Tool(2));
        let running = turn.running_step().unwrap();
        assert_eq!(running.kind, ToolKind::Edit);
        assert_eq!(running.target.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn the_answer_streams_while_the_turn_runs_and_leaves_once_it_ends() {
        let mut events = vec![
            started(1, "t1", "explica"),
            delta(2, "t1", "Primeiro, "),
            delta(3, "t1", "o **cache**"),
        ];
        let turn = now(&events).live().cloned().unwrap();
        assert_eq!(turn.text, "Primeiro, o **cache**");
        assert_eq!(turn.doing, Doing::Thinking);
        events.push(completed(4, "t1"));
        assert_eq!(now(&events), TurnNow::Idle);
    }

    #[test]
    fn a_folded_turn_streams_its_answer_but_not_its_commentary() {
        let item = |sequence: u64, kind: &str, id: &str, extra: serde_json::Value| {
            let mut json = serde_json::json!({ "type": kind, "turnId": "t1", "messageId": id });
            json.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            event(sequence, json)
        };
        let mut events = vec![
            started(1, "t1", "olha"),
            item(
                2,
                "turn_assistant_message_started",
                "m1",
                serde_json::json!({ "phase": "commentary" }),
            ),
            item(
                3,
                "turn_assistant_message_delta",
                "m1",
                serde_json::json!({ "content": "Vou olhar o arquivo." }),
            ),
        ];
        assert_eq!(now(&events).live().unwrap().text, "");
        events.push(item(
            4,
            "turn_assistant_message_started",
            "m2",
            serde_json::json!({ "phase": "final_answer" }),
        ));
        events.push(item(
            5,
            "turn_assistant_message_delta",
            "m2",
            serde_json::json!({ "content": "Está certo." }),
        ));
        assert_eq!(now(&events).live().unwrap().text, "Está certo.");
    }

    #[test]
    fn a_failed_turn_offers_its_prompt_again() {
        let events = [
            started(1, "t1", "corrige o login"),
            tool(2, "t1", "c1", "Bash", ""),
            aborted(3, "t1", "provider crashed"),
        ];
        let stopped = now(&events).stopped().cloned().unwrap();
        assert_eq!(stopped.turn_id, "t1");
        assert_eq!(stopped.reason.as_deref(), Some("provider crashed"));
        assert_eq!(stopped.retry.as_deref(), Some("corrige o login"));
        assert_eq!(stopped.ended_at.as_deref(), Some("2026-11-10T10:00:03Z"));
        // A turn passed to the agent is the person's too.
        let passed = [
            started(1, "t1", "↪ Dev → Revisor: olha isso"),
            aborted(2, "t1", "stopped"),
        ];
        assert!(now(&passed).stopped().unwrap().retry.is_some());
    }

    #[test]
    fn a_wake_that_failed_is_not_offered_again() {
        for prompt in [
            "[DCC] Routine \"Bom dia\" woke you.",
            "[DCC] Source \"Sentry\" has news.",
        ] {
            let events = [started(1, "t1", prompt), aborted(2, "t1", "boom")];
            let stopped = now(&events).stopped().cloned().unwrap();
            assert_eq!(stopped.retry, None, "{prompt}");
        }
    }

    #[test]
    fn a_usage_limit_is_not_a_stop() {
        let events = [
            started(1, "t1", "oi"),
            event(
                2,
                serde_json::json!({
                    "type": "usage_limit_reached", "turnId": "t1",
                    "resetsAt": null, "autoResume": false, "paused": true
                }),
            ),
            aborted(3, "t1", "usage limit"),
        ];
        assert_eq!(now(&events), TurnNow::Idle);
    }

    #[test]
    fn a_poll_projects_only_the_last_turn() {
        let mut events = vec![
            started(1, "t1", "antes"),
            delta(2, "t1", "Feito."),
            completed(3, "t1"),
            started(4, "t2", "agora"),
            delta(5, "t2", "Olhando"),
        ];
        let turn = turn_now_of_events(&events, "s");
        assert_eq!(turn, now(&events));
        assert_eq!(turn.live().unwrap().text, "Olhando");
        events.push(completed(6, "t2"));
        assert_eq!(turn_now_of_events(&events, "s"), TurnNow::Idle);
        assert!(moves_only_the_turn(&delta(7, "t2", "x").kind));
        assert!(!moves_only_the_turn(&completed(8, "t2").kind));
        assert!(!moves_only_the_turn(&started(9, "t3", "y").kind));
    }
}
