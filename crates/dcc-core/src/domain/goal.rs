//! The provider's own goal (`/goal`).
//!
//! Claude Code and Codex can keep a task going until a condition holds: the
//! person sets a goal and the provider runs turn after turn, checking the
//! condition, until it is met, paused or out of budget. DCC does not run that
//! loop; it only follows it so the person sees what the agent is chasing and
//! where it stands.
//!
//! Each provider reports the goal differently. Claude writes it into the
//! transcript (`Goal set: …`, `Stop hook feedback: …`) and never says it was
//! achieved: it clears the goal once the condition holds, so DCC infers
//! completion when a turn that worked on the goal ends normally. Codex sends
//! the whole goal on every change. Both arrive as a `GoalSignal`; the session
//! records the resulting goal (`SessionEventKind::GoalUpdated`) and the
//! latest one is the current goal, so it survives restarts without a store.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::session::{visible_session_events, SessionEventKind, SessionEventRecord, TurnId};

/// Longest feedback kept from a check: enough for the reason, not the
/// transcript it quotes.
pub const MAX_GOAL_CHECK_CHARS: usize = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Paused,
    Blocked,
    UsageLimited,
    BudgetLimited,
    Complete,
}

/// The goal as the session knows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionGoal {
    pub objective: String,
    pub status: GoalStatus,
    /// Times the provider checked the condition and found it not met yet.
    #[serde(default)]
    pub checks: u32,
    /// Why the latest check said it is not done yet.
    #[serde(default)]
    pub last_check: Option<String>,
    #[serde(default)]
    pub tokens_used: Option<u64>,
    #[serde(default)]
    pub token_budget: Option<u64>,
    #[serde(default)]
    pub time_used_seconds: Option<u64>,
    /// The provider never reports completion (Claude): a turn that worked on
    /// the goal and ended normally means the condition held.
    #[serde(default)]
    pub completes_with_turn: bool,
    pub updated_at: String,
}

/// The goal as a provider reports it in full (Codex).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderGoalSnapshot {
    pub objective: String,
    pub status: GoalStatus,
    #[serde(default)]
    pub tokens_used: Option<u64>,
    #[serde(default)]
    pub token_budget: Option<u64>,
    #[serde(default)]
    pub time_used_seconds: Option<u64>,
}

/// What a provider said about its goal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GoalSignal {
    /// A new goal (Claude `Goal set: …`).
    Set { objective: String },
    /// The goal is still on, with the checks so far (Claude `Goal active: …`).
    Active { objective: String, checks: u32 },
    /// A check found the condition not met; the provider keeps going.
    /// `objective` is the goal the check was about, when the provider said:
    /// a check of another goal (or of a hook of the person's own) is ignored.
    Check {
        objective: Option<String>,
        feedback: String,
    },
    /// The provider stopped pursuing the goal until the person continues.
    Paused { reason: Option<String> },
    /// No goal anymore (cleared by the person, or none was set).
    Cleared,
    /// The current goal moved to `status` (Cursor reports only the status).
    Status { status: GoalStatus },
    /// The whole goal, or `None` when there is none (Codex).
    Snapshot { goal: Option<ProviderGoalSnapshot> },
}

fn bounded(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_GOAL_CHECK_CHARS {
        return trimmed.to_string();
    }
    let mut kept: String = trimmed.chars().take(MAX_GOAL_CHECK_CHARS).collect();
    kept.push('…');
    kept
}

/// The goal after `signal`, or `None` when nothing changes.
pub fn apply_goal_signal(
    current: Option<&SessionGoal>,
    signal: &GoalSignal,
    at: &str,
) -> Option<Option<SessionGoal>> {
    let next = match signal {
        GoalSignal::Set { objective } => {
            let objective = objective.trim();
            if objective.is_empty() {
                return None;
            }
            Some(SessionGoal {
                objective: objective.to_string(),
                status: GoalStatus::Active,
                checks: 0,
                last_check: None,
                tokens_used: None,
                token_budget: None,
                time_used_seconds: None,
                completes_with_turn: true,
                updated_at: at.to_string(),
            })
        }
        GoalSignal::Active { objective, checks } => {
            let objective = objective.trim();
            if objective.is_empty() {
                return None;
            }
            let same = current.filter(|goal| goal.objective == objective);
            Some(SessionGoal {
                objective: objective.to_string(),
                status: GoalStatus::Active,
                checks: (*checks).max(same.map_or(0, |goal| goal.checks)),
                last_check: same.and_then(|goal| goal.last_check.clone()),
                tokens_used: None,
                token_budget: None,
                time_used_seconds: None,
                completes_with_turn: true,
                updated_at: at.to_string(),
            })
        }
        GoalSignal::Check {
            objective,
            feedback,
        } => {
            let goal = current?;
            if objective
                .as_deref()
                .is_some_and(|objective| objective.trim() != goal.objective)
            {
                return None;
            }
            Some(SessionGoal {
                status: GoalStatus::Active,
                checks: goal.checks.saturating_add(1),
                last_check: Some(bounded(feedback)).filter(|text| !text.is_empty()),
                updated_at: at.to_string(),
                ..goal.clone()
            })
        }
        GoalSignal::Paused { reason } => {
            let goal = current?;
            Some(SessionGoal {
                status: GoalStatus::Paused,
                last_check: reason
                    .as_deref()
                    .map(bounded)
                    .filter(|text| !text.is_empty())
                    .or_else(|| goal.last_check.clone()),
                updated_at: at.to_string(),
                ..goal.clone()
            })
        }
        GoalSignal::Cleared => None,
        GoalSignal::Status { status } => {
            let goal = current?;
            Some(SessionGoal {
                status: *status,
                last_check: (*status != GoalStatus::Complete)
                    .then(|| goal.last_check.clone())
                    .flatten(),
                updated_at: at.to_string(),
                ..goal.clone()
            })
        }
        GoalSignal::Snapshot { goal } => goal.as_ref().and_then(|snapshot| {
            let objective = snapshot.objective.trim();
            (!objective.is_empty()).then(|| {
                let same = current.filter(|goal| goal.objective == objective);
                SessionGoal {
                    objective: objective.to_string(),
                    status: snapshot.status,
                    checks: same.map_or(0, |goal| goal.checks),
                    last_check: same.and_then(|goal| goal.last_check.clone()),
                    tokens_used: snapshot.tokens_used,
                    token_budget: snapshot.token_budget,
                    time_used_seconds: snapshot.time_used_seconds,
                    completes_with_turn: false,
                    updated_at: at.to_string(),
                }
            })
        }),
    };
    let unchanged = match (current, next.as_ref()) {
        (None, None) => true,
        (Some(current), Some(next)) => {
            SessionGoal {
                updated_at: current.updated_at.clone(),
                ..next.clone()
            } == *current
        }
        _ => false,
    };
    (!unchanged).then_some(next)
}

/// The session's current goal: the latest one recorded in the visible
/// conversation (a rewind drops the goals of the turns it removed).
pub fn session_goal(events: &[SessionEventRecord]) -> Option<SessionGoal> {
    latest_goal(&visible_session_events(events)).flatten()
}

/// The latest recorded goal: `None` when none was ever recorded,
/// `Some(None)` when the latest record cleared it.
fn latest_goal(events: &[SessionEventRecord]) -> Option<Option<SessionGoal>> {
    events.iter().rev().find_map(|event| match &event.kind {
        SessionEventKind::GoalUpdated { goal, .. } => Some(goal.clone()),
        _ => None,
    })
}

/// The goal once `turn_id` ended normally, when that completes it: a goal
/// whose provider never reports completion, still active, that the turn
/// worked on after it was set (an answer or a tool call). A turn that only
/// set the goal or asked for its state does not count.
pub fn goal_completed_by_turn(
    events: &[SessionEventRecord],
    turn_id: &TurnId,
    at: &str,
) -> Option<SessionGoal> {
    let events = visible_session_events(events);
    let set_at = events
        .iter()
        .rposition(|event| matches!(event.kind, SessionEventKind::GoalUpdated { .. }))?;
    let goal = latest_goal(&events).flatten()?;
    if goal.status != GoalStatus::Active || !goal.completes_with_turn {
        return None;
    }
    let worked = events[set_at + 1..].iter().any(|event| match &event.kind {
        SessionEventKind::TurnAssistantMessageCompleted {
            turn_id: event_turn,
            content,
            ..
        } => {
            event_turn == turn_id
                && content
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty())
        }
        SessionEventKind::TurnToolCallStarted {
            turn_id: event_turn,
            ..
        } => event_turn == turn_id,
        _ => false,
    });
    worked.then(|| SessionGoal {
        status: GoalStatus::Complete,
        updated_at: at.to_string(),
        ..goal
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::SessionId;

    fn record(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("event-{sequence}"),
            session_id: SessionId("session-1".to_string()),
            sequence,
            occurred_at: format!("2026-10-18T10:00:{sequence:02}Z"),
            kind,
        }
    }

    fn goal_event(sequence: u64, turn: &str, goal: Option<SessionGoal>) -> SessionEventRecord {
        record(
            sequence,
            SessionEventKind::GoalUpdated {
                turn_id: Some(TurnId(turn.to_string())),
                goal,
            },
        )
    }

    fn set(objective: &str) -> SessionGoal {
        apply_goal_signal(
            None,
            &GoalSignal::Set {
                objective: objective.to_string(),
            },
            "t0",
        )
        .flatten()
        .expect("goal")
    }

    fn answer(sequence: u64, turn: &str, text: &str) -> SessionEventRecord {
        record(
            sequence,
            SessionEventKind::TurnAssistantMessageCompleted {
                turn_id: TurnId(turn.to_string()),
                message_id: format!("message-{sequence}"),
                phase: crate::domain::session::AssistantMessagePhase::Unknown,
                content: Some(text.to_string()),
            },
        )
    }

    #[test]
    fn set_starts_an_active_goal_that_completes_with_the_turn() {
        let goal = set("  tests pass  ");
        assert_eq!(goal.objective, "tests pass");
        assert_eq!(goal.status, GoalStatus::Active);
        assert_eq!(goal.checks, 0);
        assert!(goal.completes_with_turn);
    }

    #[test]
    fn checks_count_and_keep_the_latest_reason() {
        let goal = set("count is 3");
        let once = apply_goal_signal(
            Some(&goal),
            &GoalSignal::Check {
                objective: Some("count is 3".to_string()),
                feedback: "count is 1".to_string(),
            },
            "t1",
        )
        .flatten()
        .expect("checked");
        assert_eq!(once.checks, 1);
        assert_eq!(once.last_check.as_deref(), Some("count is 1"));
        let twice = apply_goal_signal(
            Some(&once),
            &GoalSignal::Check {
                objective: None,
                feedback: "count is 2".to_string(),
            },
            "t2",
        )
        .flatten()
        .expect("checked");
        assert_eq!(twice.checks, 2);
        assert_eq!(twice.last_check.as_deref(), Some("count is 2"));
    }

    #[test]
    fn a_check_without_a_goal_changes_nothing() {
        assert_eq!(
            apply_goal_signal(
                None,
                &GoalSignal::Check {
                    objective: None,
                    feedback: "x".to_string()
                },
                "t"
            ),
            None
        );
    }

    #[test]
    fn a_check_of_another_goal_is_ignored() {
        let goal = set("g");
        assert_eq!(
            apply_goal_signal(
                Some(&goal),
                &GoalSignal::Check {
                    objective: Some("my own stop hook".to_string()),
                    feedback: "x".to_string()
                },
                "t"
            ),
            None
        );
    }

    #[test]
    fn long_feedback_is_bounded() {
        let goal = set("g");
        let checked = apply_goal_signal(
            Some(&goal),
            &GoalSignal::Check {
                objective: None,
                feedback: "x".repeat(MAX_GOAL_CHECK_CHARS + 50),
            },
            "t",
        )
        .flatten()
        .expect("checked");
        assert_eq!(
            checked.last_check.expect("reason").chars().count(),
            MAX_GOAL_CHECK_CHARS + 1
        );
    }

    #[test]
    fn active_report_keeps_the_known_checks_of_the_same_goal() {
        let mut goal = set("g");
        goal.checks = 4;
        goal.last_check = Some("not yet".to_string());
        let reported = apply_goal_signal(
            Some(&goal),
            &GoalSignal::Active {
                objective: "g".to_string(),
                checks: 2,
            },
            "t",
        );
        assert_eq!(reported, None, "same goal, nothing new");
        let other = apply_goal_signal(
            Some(&goal),
            &GoalSignal::Active {
                objective: "other".to_string(),
                checks: 1,
            },
            "t",
        )
        .flatten()
        .expect("other goal");
        assert_eq!(other.checks, 1);
        assert_eq!(other.last_check, None);
    }

    #[test]
    fn cleared_removes_the_goal_and_is_a_no_op_without_one() {
        let goal = set("g");
        assert_eq!(
            apply_goal_signal(Some(&goal), &GoalSignal::Cleared, "t"),
            Some(None)
        );
        assert_eq!(apply_goal_signal(None, &GoalSignal::Cleared, "t"), None);
    }

    #[test]
    fn a_status_moves_the_current_goal_and_needs_one() {
        let mut goal = set("g");
        goal.last_check = Some("not yet".to_string());
        let done = apply_goal_signal(
            Some(&goal),
            &GoalSignal::Status {
                status: GoalStatus::Complete,
            },
            "t",
        )
        .flatten()
        .expect("complete");
        assert_eq!(done.status, GoalStatus::Complete);
        assert_eq!(done.objective, "g");
        assert_eq!(done.last_check, None);
        assert_eq!(
            apply_goal_signal(
                None,
                &GoalSignal::Status {
                    status: GoalStatus::Complete
                },
                "t"
            ),
            None
        );
    }

    #[test]
    fn snapshot_carries_provider_usage_and_never_completes_with_the_turn() {
        let snapshot = ProviderGoalSnapshot {
            objective: "ship".to_string(),
            status: GoalStatus::BudgetLimited,
            tokens_used: Some(1200),
            token_budget: Some(1000),
            time_used_seconds: Some(90),
        };
        let goal = apply_goal_signal(
            None,
            &GoalSignal::Snapshot {
                goal: Some(snapshot.clone()),
            },
            "t",
        )
        .flatten()
        .expect("goal");
        assert_eq!(goal.status, GoalStatus::BudgetLimited);
        assert_eq!(goal.tokens_used, Some(1200));
        assert!(!goal.completes_with_turn);
        assert_eq!(
            apply_goal_signal(
                Some(&goal),
                &GoalSignal::Snapshot {
                    goal: Some(snapshot)
                },
                "t2"
            ),
            None,
            "the same snapshot again changes nothing"
        );
    }

    #[test]
    fn the_latest_record_is_the_current_goal() {
        let events = vec![
            goal_event(1, "turn-1", Some(set("first"))),
            goal_event(2, "turn-2", None),
        ];
        assert_eq!(session_goal(&events), None);
        let events = vec![
            goal_event(1, "turn-1", None),
            goal_event(2, "turn-2", Some(set("second"))),
        ];
        assert_eq!(
            session_goal(&events).map(|goal| goal.objective),
            Some("second".to_string())
        );
    }

    #[test]
    fn a_rewind_drops_the_goal_of_the_removed_turn() {
        let events = vec![
            goal_event(1, "turn-1", Some(set("kept"))),
            goal_event(2, "turn-2", Some(set("removed"))),
            record(
                3,
                SessionEventKind::ConversationRewound {
                    anchor_turn_id: TurnId("turn-2".to_string()),
                    removed_turn_ids: vec![TurnId("turn-2".to_string())],
                    provider_context: crate::domain::session::RewindProviderContext::Native,
                    restored_turn_ids: Vec::new(),
                },
            ),
        ];
        assert_eq!(
            session_goal(&events).map(|goal| goal.objective),
            Some("kept".to_string())
        );
    }

    #[test]
    fn a_turn_that_worked_on_the_goal_completes_it() {
        let turn = TurnId("turn-1".to_string());
        let events = vec![
            goal_event(1, "turn-1", Some(set("count is 3"))),
            answer(2, "turn-1", "count is 3 now"),
        ];
        let done = goal_completed_by_turn(&events, &turn, "t9").expect("complete");
        assert_eq!(done.status, GoalStatus::Complete);
        assert_eq!(done.objective, "count is 3");
    }

    #[test]
    fn a_turn_that_only_set_or_reported_the_goal_does_not_complete_it() {
        let turn = TurnId("turn-1".to_string());
        let events = vec![goal_event(1, "turn-1", Some(set("g")))];
        assert_eq!(goal_completed_by_turn(&events, &turn, "t"), None);
        let earlier_work = vec![
            answer(1, "turn-1", "work before the goal"),
            goal_event(2, "turn-1", Some(set("g"))),
        ];
        assert_eq!(goal_completed_by_turn(&earlier_work, &turn, "t"), None);
    }

    #[test]
    fn work_in_another_turn_or_a_provider_reported_goal_does_not_complete() {
        let turn = TurnId("turn-2".to_string());
        let events = vec![
            goal_event(1, "turn-1", Some(set("g"))),
            answer(2, "turn-1", "worked in turn 1"),
        ];
        assert_eq!(goal_completed_by_turn(&events, &turn, "t"), None);
        let mut codex = set("g");
        codex.completes_with_turn = false;
        let events = vec![
            goal_event(1, "turn-2", Some(codex)),
            answer(2, "turn-2", "done"),
        ];
        assert_eq!(goal_completed_by_turn(&events, &turn, "t"), None);
    }

    #[test]
    fn a_later_turn_that_works_on_a_goal_still_active_completes_it() {
        let turn = TurnId("turn-2".to_string());
        let events = vec![
            goal_event(1, "turn-1", Some(set("g"))),
            answer(2, "turn-2", "continued and finished"),
        ];
        assert!(goal_completed_by_turn(&events, &turn, "t").is_some());
    }
}
