//! The team room's inbox, as one rule for the desktop's room and the
//! phone's: an agent's conversation in a task that finished a turn or waits
//! on the person becomes a card ("Dev terminou em Checkout"), marked with
//! the trigger that woke it when one ran shortly before, and unread until
//! the person opens it. A task an agent opened from its own conversation
//! gives a card once its own turn ends. Cards older than `KEEP_DAYS` leave.

use chrono::DateTime;

use super::agent_conversation::home_agent_id;
use super::agent_trigger::{Trigger, TriggerRun};

/// Cards older than this leave the room.
pub const KEEP_DAYS: i64 = 7;
const KEEP_MS: i64 = KEEP_DAYS * 24 * 60 * 60 * 1000;
/// A trigger run this close before a result is what woke the agent.
pub const WOKEN_WITHIN_MS: i64 = 2 * 60 * 60 * 1000;

/// An RFC 3339 time in milliseconds.
pub fn parse_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|time| time.timestamp_millis())
}

/// The trigger that woke `agent` on `target` shortly before `at_ms`.
pub fn woken_by(runs: &[TriggerRun], agent: &str, target: &str, at_ms: i64) -> Option<Trigger> {
    runs.iter()
        .filter(|run| run.agent_id == agent && run.target == target)
        .filter_map(|run| parse_ms(&run.at).map(|ran| (ran, run.trigger)))
        .filter(|(ran, _)| *ran <= at_ms && at_ms - ran <= WOKEN_WITHIN_MS)
        .max_by_key(|(ran, _)| *ran)
        .map(|(_, trigger)| trigger)
}

/// Whether a result that finished at `completed` is still unread: after
/// the reading marker's `baseline` (none: nothing is unread) and after the
/// last result of that conversation the person opened (`seen`).
pub fn result_unread(baseline: Option<&str>, seen: Option<&str>, completed: Option<&str>) -> bool {
    let (Some(completed), Some(baseline)) =
        (completed.and_then(parse_ms), baseline.and_then(parse_ms))
    else {
        return false;
    };
    if completed <= baseline {
        return false;
    }
    seen.and_then(parse_ms).is_none_or(|seen| seen < completed)
}

/// An agent's conversation in a task, as the cards read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskSession<'a> {
    pub agent_id: &'a str,
    pub session_id: &'a str,
    pub workspace_id: &'a str,
    /// The project the conversation is bound to (an agent's own: its home).
    pub project_id: &'a str,
    /// The conversation's title, when the task has no name.
    pub title: Option<&'a str>,
    /// Mid-turn and waiting on the person.
    pub waiting: bool,
    /// Mid-turn.
    pub working: bool,
    /// When its last turn finished, if it did.
    pub completed_at: Option<&'a str>,
    /// When it last moved.
    pub updated_at: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskCardKind {
    /// The conversation finished a turn.
    Result { completed_at: String },
    /// It waits on the person.
    Waiting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskCard {
    pub key: String,
    pub agent_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub kind: TaskCardKind,
    /// The task's name, else the conversation's title.
    pub title: String,
    pub trigger: Option<Trigger>,
    /// When it happened (RFC 3339); a waiting one, when it last moved.
    pub at: String,
    pub unread: bool,
}

/// The card of an agent's conversation in a task, if it has one now;
/// `unread(session, completed)` is the reading marker.
pub fn task_card(
    session: &TaskSession<'_>,
    task_name: Option<&str>,
    runs: &[TriggerRun],
    unread: &dyn Fn(&str, &str) -> bool,
    now_ms: i64,
) -> Option<TaskCard> {
    // Its own conversation is the room itself.
    if home_agent_id(session.project_id).is_some() {
        return None;
    }
    let title = task_name.or(session.title).unwrap_or_default().to_string();
    if session.waiting {
        return Some(TaskCard {
            key: format!("wait:{}", session.session_id),
            agent_id: session.agent_id.to_string(),
            workspace_id: session.workspace_id.to_string(),
            session_id: session.session_id.to_string(),
            kind: TaskCardKind::Waiting,
            title,
            trigger: None,
            at: session.updated_at.to_string(),
            unread: true,
        });
    }
    let completed = session.completed_at?;
    let at = parse_ms(completed)?;
    if session.working || now_ms - at > KEEP_MS {
        return None;
    }
    Some(TaskCard {
        key: format!("done:{}:{completed}", session.session_id),
        agent_id: session.agent_id.to_string(),
        workspace_id: session.workspace_id.to_string(),
        session_id: session.session_id.to_string(),
        kind: TaskCardKind::Result {
            completed_at: completed.to_string(),
        },
        title,
        trigger: woken_by(runs, session.agent_id, session.workspace_id, at),
        at: completed.to_string(),
        unread: unread(session.session_id, completed),
    })
}

/// A task `agent_id` opened from its conversation, once the task's own
/// conversation (`task_session`) ended its turn: `open` while that turn
/// runs, `completed` when it last ended.
#[allow(clippy::too_many_arguments)]
pub fn opened_task_card(
    agent_id: &str,
    task_id: &str,
    task_name: &str,
    task_session: &str,
    open: bool,
    completed: Option<&str>,
    unread: &dyn Fn(&str, &str) -> bool,
    now_ms: i64,
) -> Option<TaskCard> {
    if open {
        return None;
    }
    let completed = completed?;
    if parse_ms(completed).is_none_or(|at| now_ms - at > KEEP_MS) {
        return None;
    }
    Some(TaskCard {
        key: format!("done:{task_session}:{completed}"),
        agent_id: agent_id.to_string(),
        workspace_id: task_id.to_string(),
        session_id: task_session.to_string(),
        kind: TaskCardKind::Result {
            completed_at: completed.to_string(),
        },
        title: task_name.to_string(),
        trigger: None,
        at: completed.to_string(),
        unread: unread(task_session, completed),
    })
}

/// Whether a card of `card_room` shows in `room` (none: Geral). A result of
/// a task an agent opened (`asked_from`: the room of the conversation that
/// asked for it) stays out of that room, whose conversation already carries
/// the report.
pub fn shows_in(
    card_room: Option<&str>,
    asked_from: Option<Option<&str>>,
    room: Option<&str>,
) -> bool {
    card_room == room && asked_from != Some(room)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session<'a>(id: &'a str, project: &'a str) -> TaskSession<'a> {
        TaskSession {
            agent_id: "rev",
            session_id: id,
            workspace_id: "w-1",
            project_id: project,
            title: Some("Conversa"),
            waiting: false,
            working: false,
            completed_at: Some("2026-11-07T10:30:00Z"),
            updated_at: "2026-11-07T11:00:00Z",
        }
    }

    #[test]
    fn a_task_conversation_is_a_result_or_a_wait_with_what_woke_it() {
        let now = parse_ms("2026-11-07T11:00:00Z").unwrap();
        let runs = [TriggerRun {
            agent_id: "rev".into(),
            trigger: Trigger::CiFailed,
            target: "w-1".into(),
            at: "2026-11-07T10:20:00Z".into(),
        }];
        let unread = |session: &str, _: &str| session == "s-1";
        let done = task_card(
            &session("s-1", "p-1"),
            Some("Checkout"),
            &runs,
            &unread,
            now,
        )
        .unwrap();
        assert_eq!(done.key, "done:s-1:2026-11-07T10:30:00Z");
        assert_eq!(done.title, "Checkout");
        assert_eq!(done.trigger, Some(Trigger::CiFailed));
        assert!(done.unread);

        let mut asking = session("s-2", "p-1");
        asking.waiting = true;
        let wait = task_card(&asking, None, &runs, &unread, now).unwrap();
        assert_eq!(
            (wait.kind, wait.title.as_str()),
            (TaskCardKind::Waiting, "Conversa")
        );
        assert!(wait.unread);

        // Its own conversation, one at work, one too old: no card.
        assert!(task_card(&session("s-3", "dcc-agent-rev"), None, &runs, &unread, now).is_none());
        let mut working = session("s-4", "p-1");
        working.working = true;
        assert!(task_card(&working, None, &runs, &unread, now).is_none());
        let mut old = session("s-5", "p-1");
        old.completed_at = Some("2026-10-01T10:00:00Z");
        assert!(task_card(&old, None, &runs, &unread, now).is_none());
    }

    #[test]
    fn an_opened_task_is_a_card_once_its_turn_ends() {
        let now = parse_ms("2026-11-10T19:00:00Z").unwrap();
        let unread = |_: &str, _: &str| true;
        let card = opened_task_card(
            "dev",
            "w-d",
            "Corrigir domínio",
            "s-task",
            false,
            Some("2026-11-10T18:50:00Z"),
            &unread,
            now,
        )
        .unwrap();
        assert_eq!(card.title, "Corrigir domínio");
        assert_eq!(card.session_id, "s-task");
        assert!(opened_task_card(
            "dev",
            "w",
            "t",
            "s",
            true,
            Some("2026-11-10T18:50:00Z"),
            &unread,
            now
        )
        .is_none());
        assert!(opened_task_card("dev", "w", "t", "s", false, None, &unread, now).is_none());
    }

    #[test]
    fn unread_is_after_the_baseline_and_the_last_seen() {
        let base = Some("2026-11-01T00:00:00Z");
        assert!(result_unread(base, None, Some("2026-11-02T00:00:00Z")));
        assert!(!result_unread(base, None, Some("2026-10-30T00:00:00Z")));
        assert!(!result_unread(None, None, Some("2026-11-02T00:00:00Z")));
        assert!(!result_unread(
            base,
            Some("2026-11-02T00:00:00Z"),
            Some("2026-11-02T00:00:00Z")
        ));
        assert!(result_unread(
            base,
            Some("2026-11-02T00:00:00Z"),
            Some("2026-11-03T00:00:00Z")
        ));
    }

    #[test]
    fn a_card_shows_in_its_room_but_not_where_it_was_asked() {
        assert!(shows_in(Some("p"), None, Some("p")));
        assert!(!shows_in(Some("p"), None, None));
        assert!(shows_in(None, Some(Some("p")), None));
        assert!(!shows_in(Some("p"), Some(Some("p")), Some("p")));
    }
}
