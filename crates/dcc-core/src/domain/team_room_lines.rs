//! The team room's lines (`team_room`): what the person said to each agent
//! in its own conversation and the agent's answer, each answer with its
//! receipt (how long it took, how many tokens). Read the same way by the
//! desktop's room and the phone's.

use std::collections::HashMap;

use serde::Serialize;

use crate::domain::session::AssistantMessagePhase;
use crate::domain::team_room_steps::TurnSteps;
use crate::domain::timeline::{Role, TimelineMessage};

/// Who said a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Speaker {
    Person,
    Agent,
}

/// How an answer went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub seconds: Option<i64>,
    pub tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomLine {
    pub key: String,
    pub agent_id: String,
    pub session_id: String,
    pub speaker: Speaker,
    pub text: String,
    /// When it was said (RFC 3339).
    pub at: String,
    pub receipt: Option<Receipt>,
    /// The tasks a [DCC] status reported in the turn this answers (the task
    /// that ended and the others opened with it), for the room's report.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<String>,
    /// What the turn's tool calls did, counted once here (the room's folded
    /// line); the phone does not read it.
    #[serde(skip)]
    pub steps: TurnSteps,
}

/// The tasks a `[DCC] Task "…" ended a turn` status reports: its own and
/// the others listed as opened in the same turn (`task_group`).
pub fn reported_tasks(status: &str) -> Vec<String> {
    let mut tasks = Vec::new();
    if let Some(hand_back) = crate::domain::timeline::parse_delegation_hand_back(status) {
        tasks.push(hand_back.delegation_id);
    }
    let Some(start) = status.find("The other tasks you opened in the same turn") else {
        return tasks;
    };
    for line in status[start..].lines().skip(1) {
        let Some(rest) = line.strip_prefix("- \"") else {
            break;
        };
        let Some(id) = rest
            .split_once("\" (task ")
            .and_then(|(_, tail)| tail.split_once(')'))
            .map(|(id, _)| id)
            .filter(|id| !id.is_empty() && !id.contains(char::is_whitespace))
        else {
            continue;
        };
        tasks.push(id.to_string());
    }
    tasks
}

fn seconds_between(start: &str, end: &str) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(start).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(end).ok()?;
    Some((end - start).num_seconds().max(0))
}

/// One conversation's lines: the person's messages (not DCC's own turns)
/// and, per finished turn, the agent's last answer.
pub fn lines_of(
    agent_id: &str,
    session_id: &str,
    messages: &[TimelineMessage],
    tokens: &HashMap<String, u64>,
) -> Vec<RoomLine> {
    let mut lines: Vec<RoomLine> = Vec::new();
    // The last answer of each turn, by its turn.
    let mut answer_of: HashMap<&str, usize> = HashMap::new();
    // The tasks a status reported, by the turn it started.
    let mut reported: HashMap<&str, Vec<String>> = HashMap::new();
    // The tool calls of each turn, over all its assistant rows.
    let mut steps: HashMap<&str, TurnSteps> = HashMap::new();
    for message in messages {
        if let (Role::Assistant, Some(turn)) = (message.role, message.turn_id.as_deref()) {
            if !message.annotations.is_empty() {
                steps.entry(turn).or_default().add(&message.annotations);
            }
        }
        let at = message
            .created_at
            .clone()
            .or_else(|| message.turn_ended_at.clone())
            .unwrap_or_default();
        match message.role {
            Role::User if message.hand_back.is_some() => {
                if let Some(turn) = message.turn_id.as_deref() {
                    reported.insert(turn, reported_tasks(&message.content));
                }
            }
            Role::User if !message.content.trim().is_empty() => {
                lines.push(RoomLine {
                    key: format!("{session_id}:{}", message.id),
                    agent_id: agent_id.to_string(),
                    session_id: session_id.to_string(),
                    speaker: Speaker::Person,
                    text: message.content.trim().to_string(),
                    at,
                    receipt: None,
                    tasks: Vec::new(),
                    steps: TurnSteps::default(),
                });
            }
            Role::Assistant
                if message.assistant_phase != Some(AssistantMessagePhase::Commentary)
                    && !message.streaming
                    && message.turn_settled
                    && !message.content.trim().is_empty() =>
            {
                let Some(turn) = message.turn_id.as_deref() else {
                    continue;
                };
                let receipt = Receipt {
                    seconds: message
                        .turn_started_at
                        .as_deref()
                        .zip(message.turn_ended_at.as_deref())
                        .and_then(|(start, end)| seconds_between(start, end)),
                    tokens: tokens.get(turn).copied(),
                };
                let line = RoomLine {
                    key: format!("{session_id}:{turn}"),
                    agent_id: agent_id.to_string(),
                    session_id: session_id.to_string(),
                    speaker: Speaker::Agent,
                    text: message.content.trim().to_string(),
                    at: message.turn_ended_at.clone().unwrap_or(at),
                    receipt: Some(receipt),
                    tasks: reported.get(turn).cloned().unwrap_or_default(),
                    steps: TurnSteps::default(),
                };
                match answer_of.get(turn) {
                    Some(&index) => lines[index] = line,
                    None => {
                        answer_of.insert(turn, lines.len());
                        lines.push(line);
                    }
                }
            }
            _ => {}
        }
    }
    for (turn, index) in answer_of {
        if let Some(steps) = steps.remove(turn) {
            lines[index].steps = steps;
        }
    }
    lines
}

/// The agent that answered last and how many minutes ago.
pub fn last_answer(
    lines: &[&RoomLine],
    now: chrono::DateTime<chrono::Utc>,
) -> Option<(String, i64)> {
    let line = lines
        .iter()
        .rev()
        .find(|line| line.speaker == Speaker::Agent)?;
    let at = chrono::DateTime::parse_from_rfc3339(&line.at).ok()?;
    Some((
        line.agent_id.clone(),
        (now - at.with_timezone(&chrono::Utc)).num_minutes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_names_the_tasks_it_reports() {
        let status = "[DCC] Task \"Campo X\" ended a turn — Claude Code (task t-2).\n\nFeito.\nThe other tasks you opened in the same turn ended their first turn too:\n- \"Análise\" (task t-1): dá para enviar.\nGive the person one report for all of them.\n\nYour tasks:\n- Outra — open (task t-9)";
        assert_eq!(reported_tasks(status), ["t-2", "t-1"]);
        assert!(reported_tasks("corrige isso").is_empty());
    }

    fn message(id: &str, role: Role, turn: &str, content: &str, at: &str) -> TimelineMessage {
        let mut message = TimelineMessage {
            id: String::new(),
            role: Role::User,
            turn_id: None,
            turn_settled: false,
            turn_started_at: None,
            turn_ended_at: None,
            assistant_phase: None,
            content: String::new(),
            label: String::new(),
            model: None,
            streaming: false,
            created_at: None,
            status: None,
            annotations: Vec::new(),
            plan_mode: false,
            answers_plan_mode: false,
            evidence: None,
            retry_of_turn_id: None,
            action: None,
            delegation: None,
            delegation_turn_id: None,
            hand_back: None,
        };
        message.id = id.into();
        message.role = role;
        message.turn_id = Some(turn.into());
        message.turn_settled = true;
        message.content = content.into();
        message.created_at = Some(at.into());
        message.turn_started_at = Some("2026-11-07T10:00:00Z".into());
        message.turn_ended_at = Some("2026-11-07T10:00:42Z".into());
        message
    }

    #[test]
    fn a_conversation_reads_as_the_persons_lines_and_the_last_answer_of_each_turn() {
        let mut thinking = message(
            "m-2",
            Role::Assistant,
            "t-1",
            "olhando…",
            "2026-11-07T10:00:10Z",
        );
        thinking.assistant_phase = Some(AssistantMessagePhase::Commentary);
        let messages = vec![
            message(
                "m-1",
                Role::User,
                "t-1",
                "revisa o PR",
                "2026-11-07T10:00:00Z",
            ),
            thinking,
            message(
                "m-3",
                Role::Assistant,
                "t-1",
                "Primeira parte",
                "2026-11-07T10:00:20Z",
            ),
            message(
                "m-4",
                Role::Assistant,
                "t-1",
                "Tudo certo.",
                "2026-11-07T10:00:40Z",
            ),
        ];
        let tokens = HashMap::from([("t-1".to_string(), 1200)]);
        let lines = lines_of("rev", "s-1", &messages, &tokens);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].speaker, Speaker::Person);
        assert_eq!(lines[1].text, "Tudo certo.");
        assert_eq!(
            lines[1].receipt,
            Some(Receipt {
                seconds: Some(42),
                tokens: Some(1200)
            })
        );
        let room: Vec<&RoomLine> = lines.iter().collect();
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-07T10:05:42Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(last_answer(&room, now), Some(("rev".to_string(), 5)));
    }

    #[test]
    fn an_answer_carries_the_tool_calls_of_its_whole_turn() {
        let mut work = message("m-2", Role::Assistant, "t-1", "", "2026-11-07T10:00:10Z");
        work.annotations = vec![crate::domain::timeline::Annotation::ToolCall(
            crate::domain::timeline::ToolCall {
                id: "c".into(),
                action: "Bash".into(),
                command: None,
                file: None,
                content: String::new(),
                detail: None,
                streaming: false,
                created_at: None,
                failure: None,
            },
        )];
        // The work sits on a row of its own, after the answer's.
        let messages = vec![
            message(
                "m-1",
                Role::Assistant,
                "t-1",
                "Pronto.",
                "2026-11-07T10:00:40Z",
            ),
            work,
        ];
        let lines = lines_of("rev", "s-1", &messages, &HashMap::new());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].steps.tools, 1);
        let quiet = vec![message(
            "m-3",
            Role::Assistant,
            "t-2",
            "Oi.",
            "2026-11-07T10:01:00Z",
        )];
        assert!(lines_of("rev", "s-1", &quiet, &HashMap::new())[0]
            .steps
            .is_empty());
    }
}
