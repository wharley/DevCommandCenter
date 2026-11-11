//! The room's turns at work, for the phone (`dcc_core::domain::team_room_live`,
//! as the desktop's live rows read them): an agent's open turn in one of the
//! room's conversations — what it does now, its last steps and the answer
//! streamed so far — and a running task an agent opened from there (its own
//! conversation: what it does and its last steps, no text); a turn that
//! stopped, with the prompt to try again; and the messages queued while a
//! turn runs. A running conversation is read from where its last turn (or
//! its oldest message still queued) starts, and only what came after the
//! last read is fetched, so a read every beat costs what streamed since.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use dcc_core::application::turn_queue::project_turn_queue;
use dcc_core::domain::delegation::Delegation;
use dcc_core::domain::session::{QueuedTurn, SessionEventKind, SessionEventRecord, SessionId};
use dcc_core::domain::task_report::{is_task, report_state, report_task_id, ReportState};
use dcc_core::domain::team_room_live::{
    turn_now_of_events, Doing, LiveStep, LiveTurn, StepState, StoppedTurn, TurnNow,
};
use dcc_core::domain::workspace::WorkspaceId;
use dcc_core::ports::DelegationRepo;
use dcc_infra::db::{AgentSessionBinding, SqliteSessionRepo};
use serde_json::{json, Value};

use super::room_feed::events_after;

/// The steps a live row gets; the earlier ones are counted.
pub(super) const STEPS_SHOWN: usize = 6;
/// A stop older than this no longer asks to be tried again.
const STOPPED_HOURS: i64 = 24;
/// The streamed text a live row gets, its latest part.
const TEXT_CHARS: usize = 6_000;
/// Conversations whose tail is kept between reads.
const KEPT_TAILS: usize = 64;

type Tails = Mutex<HashMap<String, (u64, Vec<SessionEventRecord>)>>;

fn tails() -> &'static Tails {
    static TAILS: OnceLock<Tails> = OnceLock::new();
    TAILS.get_or_init(Default::default)
}

/// Where a conversation's tail starts: its last turn's start, or before it
/// the first message still queued (the queue is read from there).
pub(super) fn tail_start(events: &[SessionEventRecord]) -> usize {
    let turn = events
        .iter()
        .rposition(|event| matches!(event.kind, SessionEventKind::TurnStarted { .. }))
        .unwrap_or(0);
    let pending: Vec<String> = project_turn_queue(events)
        .into_iter()
        .map(|queued| queued.id)
        .collect();
    let queued = events.iter().position(|event| {
        matches!(&event.kind, SessionEventKind::TurnQueued { queued_turn } if pending.contains(&queued_turn.id))
    });
    queued.map_or(turn, |queued| queued.min(turn))
}

/// A conversation's events from its tail's start, fetching only those after
/// the last read.
pub(super) fn tail(
    repo: &SqliteSessionRepo,
    session_id: &str,
) -> Result<Vec<SessionEventRecord>, String> {
    let kept = tails()
        .lock()
        .ok()
        .and_then(|tails| tails.get(session_id).cloned());
    let (cursor, mut events) = kept.unwrap_or_default();
    let fresh = events_after(repo, &SessionId(session_id.to_string()), cursor)?;
    if fresh.is_empty() && cursor > 0 {
        return Ok(events);
    }
    let last = fresh.last().map_or(cursor, |event| event.sequence);
    events.extend(fresh);
    let start = tail_start(&events);
    events.drain(..start);
    if let Ok(mut tails) = tails().lock() {
        if tails.len() >= KEPT_TAILS && !tails.contains_key(session_id) {
            tails.clear();
        }
        tails.insert(session_id.to_string(), (last, events.clone()));
    }
    Ok(events)
}

/// What a conversation's tail shows now, and the messages queued there.
pub(super) fn now_and_queue(
    repo: &SqliteSessionRepo,
    session_id: &str,
) -> Result<(TurnNow, Vec<QueuedTurn>), String> {
    let events = tail(repo, session_id)?;
    Ok((
        turn_now_of_events(&events, session_id),
        project_turn_queue(&events),
    ))
}

fn step_json(step: &LiveStep) -> Value {
    json!({
        "kind": step.kind.key(),
        "target": step.target,
        "state": match step.state {
            StepState::Running => "running",
            StepState::Done => "done",
            StepState::Failed => "failed",
        },
    })
}

/// The latest `TEXT_CHARS` of a streamed answer, "…" where it was cut.
pub(super) fn streamed_text(text: &str) -> String {
    let count = text.chars().count();
    if count <= TEXT_CHARS {
        return text.to_string();
    }
    let tail: String = text.chars().skip(count - TEXT_CHARS).collect();
    format!("…{tail}")
}

/// An open turn: `{turnId, startedAt, doing, steps, earlier, text?}` —
/// `doing` is `reading`, `thinking` or `tool` (the running step is the one
/// whose state is `running`), `steps` the last `STEPS_SHOWN`.
pub(super) fn turn_json(turn: &LiveTurn, with_text: bool) -> Value {
    let earlier = turn.steps.len().saturating_sub(STEPS_SHOWN);
    let mut value = json!({
        "turnId": turn.turn_id,
        "startedAt": turn.started_at,
        "doing": match turn.doing {
            Doing::Reading => "reading",
            Doing::Thinking => "thinking",
            Doing::Tool(_) => "tool",
        },
        "steps": turn.steps[earlier..].iter().map(step_json).collect::<Vec<_>>(),
        "earlier": earlier,
    });
    if with_text {
        value["text"] = json!(streamed_text(&turn.text));
    }
    value
}

/// An agent at work in one of the room's conversations.
pub(super) fn conversation_live(agent_id: &str, session_id: &str, turn: &LiveTurn) -> Value {
    let mut value = turn_json(turn, true);
    value["kind"] = json!("conversation");
    value["agentId"] = json!(agent_id);
    value["sessionId"] = json!(session_id);
    value
}

/// A running task an agent opened from the room: its own conversation.
pub(super) fn task_live(agent_id: &str, task: &RunningTask, turn: &LiveTurn) -> Value {
    let mut value = turn_json(turn, false);
    value["kind"] = json!("task");
    value["agentId"] = json!(agent_id);
    value["sessionId"] = json!(task.session_id);
    value["task"] = json!({
        "title": dcc_core::domain::task_report::report_title(&task.delegation),
        "workspaceId": task.workspace_id,
    });
    value
}

/// A turn's reason on one short line.
fn reason_line(reason: &str) -> String {
    let line = reason.lines().next().unwrap_or(reason).trim();
    let mut short: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        short.push('…');
    }
    short
}

/// "Dev parou antes de terminar" for a stop of the last day:
/// `{agentId, sessionId, turnId, reason, endedAt, retry}` with `retry`
/// `{sessionId, prompt, turnId}` when the person sent or passed it.
pub(super) fn stopped_json(
    agent_id: &str,
    session_id: &str,
    turn: &StoppedTurn,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<Value> {
    let ended = chrono::DateTime::parse_from_rfc3339(turn.ended_at.as_deref()?).ok()?;
    if now - ended.with_timezone(&chrono::Utc) > chrono::Duration::hours(STOPPED_HOURS) {
        return None;
    }
    Some(json!({
        "agentId": agent_id,
        "sessionId": session_id,
        "turnId": turn.turn_id,
        "reason": turn.reason.as_deref().map(reason_line).filter(|line| !line.is_empty()),
        "endedAt": turn.ended_at,
        "retry": turn.retry.as_ref().map(|prompt| json!({
            "sessionId": session_id,
            "prompt": prompt,
            "turnId": turn.turn_id,
        })),
    }))
}

/// The messages queued in a conversation, oldest first.
pub(super) fn queued_json(agent_id: &str, queue: &[QueuedTurn]) -> Vec<Value> {
    queue
        .iter()
        .map(|queued| {
            json!({
                "id": queued.id,
                "agentId": agent_id,
                "sessionId": queued.session_id.0,
                "text": queued.prompt,
                "at": queued.created_at,
            })
        })
        .collect()
}

/// A task an agent opened from one of the room's conversations, whose own
/// conversation runs now.
pub(super) struct RunningTask {
    pub agent_id: String,
    pub session_id: String,
    pub workspace_id: String,
    pub delegation: Delegation,
}

/// Whether a task may be running: one still at work, or one whose turn
/// ended and that a follow-up may have woken.
fn may_run(delegation: &Delegation) -> bool {
    is_task(delegation)
        && delegation.child_session_id.is_some()
        && matches!(
            report_state(delegation),
            ReportState::Working | ReportState::TurnEnded
        )
}

/// The running tasks the agents opened from `homes`.
pub(super) fn running_tasks(
    repo: &SqliteSessionRepo,
    homes: &[&AgentSessionBinding],
) -> Result<Vec<RunningTask>, String> {
    let runtime = tokio::runtime::Handle::current();
    let mut running = Vec::new();
    let mut sessions_of: HashMap<String, Vec<(String, bool)>> = HashMap::new();
    for home in homes {
        let delegations = runtime
            .block_on(repo.list_delegations(None, Some(&SessionId(home.session_id.clone()))))
            .map_err(|error| error.to_string())?;
        for delegation in delegations.into_iter().filter(may_run) {
            let (Some(workspace_id), Some(child)) = (
                report_task_id(&delegation),
                delegation.child_session_id.as_ref().map(|id| id.0.clone()),
            ) else {
                continue;
            };
            if !sessions_of.contains_key(&workspace_id) {
                let sessions = repo
                    .list_workspace_sessions(&WorkspaceId(workspace_id.clone()))
                    .map_err(|error| error.to_string())?
                    .iter()
                    .map(|summary| {
                        (
                            summary.session.id.0.clone(),
                            dcc_core::domain::attention::SessionActivityInput::from_summary(
                                summary,
                            )
                            .is_running(),
                        )
                    })
                    .collect();
                sessions_of.insert(workspace_id.clone(), sessions);
            }
            let runs = sessions_of[&workspace_id]
                .iter()
                .any(|(id, running)| *id == child && *running);
            if runs {
                running.push(RunningTask {
                    agent_id: home.agent_id.clone(),
                    session_id: child,
                    workspace_id,
                    delegation,
                });
            }
        }
    }
    Ok(running)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::timeline::activity::ToolKind;

    fn event(sequence: u64, json: serde_json::Value) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("e-{sequence}"),
            session_id: SessionId("s".into()),
            sequence,
            occurred_at: format!("2026-11-10T10:00:{sequence:02}Z"),
            kind: serde_json::from_value(json).unwrap(),
        }
    }

    fn started(sequence: u64, turn: &str) -> SessionEventRecord {
        event(
            sequence,
            json!({
                "type": "turn_started", "turnId": turn, "prompt": "oi",
                "planMode": false, "model": null, "evidence": null,
                "retryOfTurnId": null, "approvalPolicy": "ask"
            }),
        )
    }

    fn queued(sequence: u64, id: &str) -> SessionEventRecord {
        event(
            sequence,
            json!({
                "type": "turn_queued",
                "queuedTurn": { "id": id, "sessionId": "s", "prompt": format!("fila {id}"),
                    "createdAt": "2026-11-10T10:00:00Z" }
            }),
        )
    }

    fn dispatched(sequence: u64, id: &str) -> SessionEventRecord {
        event(
            sequence,
            json!({ "type": "queued_turn_dispatched", "queuedTurnId": id, "turnId": "t2" }),
        )
    }

    #[test]
    fn the_tail_starts_at_the_last_turn_or_the_oldest_message_still_queued() {
        let mut events = vec![started(1, "t1"), queued(2, "q1"), queued(3, "q2")];
        assert_eq!(tail_start(&events), 0);
        events.push(dispatched(4, "q1"));
        events.push(started(5, "t2"));
        // "q2" still waits: it was queued before "t2" started.
        assert_eq!(tail_start(&events), 2);
        events.push(dispatched(6, "q2"));
        events.push(started(7, "t3"));
        assert_eq!(tail_start(&events), 6);
        let queue: Vec<String> = project_turn_queue(&events[2..])
            .into_iter()
            .map(|queued| queued.id)
            .collect();
        assert!(queue.is_empty());
        assert_eq!(tail_start(&[]), 0);
    }

    #[test]
    fn a_live_turn_carries_its_last_steps_and_what_it_does() {
        let steps: Vec<LiveStep> = (0..8)
            .map(|n| LiveStep {
                kind: ToolKind::Read,
                target: Some(format!("src/{n}.rs")),
                state: if n == 7 {
                    StepState::Running
                } else if n == 3 {
                    StepState::Failed
                } else {
                    StepState::Done
                },
            })
            .collect();
        let turn = LiveTurn {
            turn_id: "t".into(),
            started_at: Some("2026-11-10T10:00:00Z".into()),
            text: "Achei".into(),
            steps,
            doing: Doing::Tool(7),
        };
        let value = conversation_live("dev", "s", &turn);
        assert_eq!(value["kind"], "conversation");
        assert_eq!(value["doing"], "tool");
        assert_eq!(value["earlier"], 2);
        assert_eq!(value["steps"].as_array().unwrap().len(), STEPS_SHOWN);
        assert_eq!(
            value["steps"][5],
            json!({ "kind": "read", "target": "src/7.rs", "state": "running" })
        );
        assert_eq!(value["steps"][1]["state"], "failed");
        assert_eq!(value["text"], "Achei");
        let quiet = turn_json(&turn, false);
        assert!(quiet.get("text").is_none());
    }

    #[test]
    fn a_long_answer_streams_its_latest_part() {
        assert_eq!(streamed_text("curto"), "curto");
        let long = "a".repeat(TEXT_CHARS) + "fim";
        let cut = streamed_text(&long);
        assert!(cut.starts_with('…') && cut.ends_with("fim"));
        assert_eq!(cut.chars().count(), TEXT_CHARS + 1);
    }

    #[test]
    fn a_stop_of_the_last_day_offers_the_prompt_again() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-10T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let turn = StoppedTurn {
            turn_id: "t".into(),
            reason: Some("exit 1\nmais".into()),
            ended_at: Some("2026-11-10T11:00:00Z".into()),
            retry: Some("corrige o teste".into()),
        };
        let value = stopped_json("dev", "s", &turn, now).unwrap();
        assert_eq!(value["reason"], "exit 1");
        assert_eq!(
            value["retry"],
            json!({ "sessionId": "s", "prompt": "corrige o teste", "turnId": "t" })
        );
        let dcc = StoppedTurn {
            retry: None,
            ..turn.clone()
        };
        assert!(stopped_json("dev", "s", &dcc, now).unwrap()["retry"].is_null());
        let old = StoppedTurn {
            ended_at: Some("2026-11-08T11:00:00Z".into()),
            ..turn
        };
        assert!(stopped_json("dev", "s", &old, now).is_none());
    }

    #[test]
    fn a_queued_message_names_its_conversation() {
        let queue = project_turn_queue(&[queued(1, "q1")]);
        let value = queued_json("dev", &queue);
        assert_eq!(
            value,
            [json!({ "id": "q1", "agentId": "dev", "sessionId": "s",
                "text": "fila q1", "at": "2026-11-10T10:00:00Z" })]
        );
    }
}
