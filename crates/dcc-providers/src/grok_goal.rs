//! Grok Build's own goal (`/goal`), as its ACP agent reports it.
//!
//! Grok runs the whole goal inside the `session/prompt` that set it: a plan
//! writer, the worker rounds and the verifiers are subagents of that prompt,
//! and the prompt only answers once the goal is complete, paused or out of
//! budget. So a goal is one DCC turn. Grok reports the goal in its extension
//! notification `_x.ai/session_notification` (`goal_updated`) with the whole
//! state each time, which DCC follows as a `GoalSignal::Snapshot`.
//!
//! `goal_updated` arrives every few seconds while subagents spend tokens.
//! Only a new objective or status goes out at once; token counts alone at
//! most every `TOKEN_REFRESH`, and the latest ones when the prompt ends.

use std::time::{Duration, Instant};

use serde_json::Value;

use dcc_core::domain::goal::{GoalSignal, GoalStatus, ProviderGoalSnapshot};

const TOKEN_REFRESH: Duration = Duration::from_secs(30);

/// A goal the way `goal_updated` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GrokGoal {
    pub snapshot: ProviderGoalSnapshot,
    /// Why Grok stopped working on it (`pause_message`).
    pub pause_message: Option<String>,
}

/// Grok's goal statuses. `None` for one DCC does not know, which is skipped
/// rather than guessed.
fn grok_goal_status(status: &str) -> Option<GoalStatus> {
    Some(match status {
        "active" => GoalStatus::Active,
        "complete" | "completed" | "achieved" => GoalStatus::Complete,
        "user_paused" | "back_off_paused" | "no_progress_paused" | "infra_paused" => {
            GoalStatus::Paused
        }
        "blocked" => GoalStatus::Blocked,
        "budget_limited" => GoalStatus::BudgetLimited,
        _ => return None,
    })
}

/// What a `goal_updated` says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GrokGoalUpdate {
    Goal(GrokGoal),
    /// `/goal clear`.
    Cleared,
}

/// The goal in a `goal_updated` update, when it is one DCC can show.
pub(crate) fn grok_goal_update(update: &Value) -> Option<GrokGoalUpdate> {
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("goal_updated") {
        return None;
    }
    let status = update.get("status")?.as_str()?;
    if status == "cleared" {
        return Some(GrokGoalUpdate::Cleared);
    }
    let objective = update.get("objective")?.as_str()?.trim();
    if objective.is_empty() {
        return None;
    }
    let status = grok_goal_status(status)?;
    Some(GrokGoalUpdate::Goal(GrokGoal {
        snapshot: ProviderGoalSnapshot {
            objective: objective.to_string(),
            status,
            tokens_used: update.get("tokens_used").and_then(Value::as_u64),
            token_budget: update.get("token_budget").and_then(Value::as_u64),
            time_used_seconds: update
                .get("elapsed_ms")
                .and_then(Value::as_u64)
                .map(|ms| ms / 1_000),
        },
        pause_message: update
            .get("pause_message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|message| !message.is_empty())
            .map(str::to_string),
    }))
}

/// What DCC last told the session about Grok's goal.
#[derive(Default)]
pub(crate) struct GrokGoalTracker {
    reported: Option<GrokGoal>,
    reported_at: Option<Instant>,
    /// The latest goal Grok sent, when it only moved the token counts.
    pending: Option<GrokGoal>,
}

impl GrokGoalTracker {
    /// The signals for a `goal_updated` Grok sent at `now`.
    pub fn observe(&mut self, update: GrokGoalUpdate, now: Instant) -> Vec<GoalSignal> {
        let goal = match update {
            GrokGoalUpdate::Goal(goal) => goal,
            GrokGoalUpdate::Cleared => {
                self.reported = None;
                self.reported_at = None;
                self.pending = None;
                return vec![GoalSignal::Cleared];
            }
        };
        let same_goal = self.reported.as_ref().is_some_and(|reported| {
            reported.snapshot.objective == goal.snapshot.objective
                && reported.snapshot.status == goal.snapshot.status
                && reported.pause_message == goal.pause_message
        });
        if same_goal {
            let fresh = self
                .reported_at
                .is_some_and(|at| now.duration_since(at) < TOKEN_REFRESH);
            if fresh || self.reported.as_ref() == Some(&goal) {
                self.pending = (self.reported.as_ref() != Some(&goal)).then_some(goal);
                return Vec::new();
            }
        }
        self.report(goal, now)
    }

    /// The prompt ended: the token counts held back go out now.
    pub fn flush(&mut self, now: Instant) -> Vec<GoalSignal> {
        match self.pending.take() {
            Some(goal) => self.report(goal, now),
            None => Vec::new(),
        }
    }

    /// The goal is still being pursued, as far as DCC knows.
    pub fn is_active(&self) -> bool {
        self.reported
            .as_ref()
            .is_some_and(|goal| goal.snapshot.status == GoalStatus::Active)
    }

    /// Grok's goal is gone with its process (Stop kills it, and a new process
    /// starts a new Grok session that never heard of the goal).
    pub fn end(&mut self) -> Vec<GoalSignal> {
        self.pending = None;
        let unfinished = self
            .reported
            .take()
            .is_some_and(|goal| goal.snapshot.status != GoalStatus::Complete);
        self.reported_at = None;
        if unfinished {
            vec![GoalSignal::Cleared]
        } else {
            Vec::new()
        }
    }

    fn report(&mut self, goal: GrokGoal, now: Instant) -> Vec<GoalSignal> {
        let mut signals = vec![GoalSignal::Snapshot {
            goal: Some(goal.snapshot.clone()),
        }];
        if goal.snapshot.status == GoalStatus::Paused {
            if let Some(reason) = goal.pause_message.clone() {
                signals.push(GoalSignal::Paused {
                    reason: Some(reason),
                });
            }
        }
        self.reported = Some(goal);
        self.reported_at = Some(now);
        self.pending = None;
        signals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn update(status: &str, tokens: u64) -> Value {
        json!({
            "sessionUpdate": "goal_updated",
            "goal_id": "goal-1",
            "objective": "Create hello.txt",
            "status": status,
            "phase": "executing",
            "tokens_used": tokens,
            "elapsed_ms": 61_500,
        })
    }

    fn goal(status: &str, tokens: u64) -> GrokGoal {
        match grok_goal_update(&update(status, tokens)) {
            Some(GrokGoalUpdate::Goal(goal)) => goal,
            other => panic!("expected a goal, got {other:?}"),
        }
    }

    fn observe(tracker: &mut GrokGoalTracker, goal: GrokGoal, now: Instant) -> Vec<GoalSignal> {
        tracker.observe(GrokGoalUpdate::Goal(goal), now)
    }

    #[test]
    fn reads_the_goal_grok_reports() {
        let goal = goal("active", 4975);
        assert_eq!(goal.snapshot.objective, "Create hello.txt");
        assert_eq!(goal.snapshot.status, GoalStatus::Active);
        assert_eq!(goal.snapshot.tokens_used, Some(4975));
        assert_eq!(goal.snapshot.token_budget, None);
        assert_eq!(goal.snapshot.time_used_seconds, Some(61));
    }

    #[test]
    fn maps_every_paused_status_and_skips_unknown_ones() {
        for status in [
            "user_paused",
            "back_off_paused",
            "no_progress_paused",
            "infra_paused",
        ] {
            assert_eq!(goal(status, 0).snapshot.status, GoalStatus::Paused);
        }
        assert_eq!(goal("blocked", 0).snapshot.status, GoalStatus::Blocked);
        assert_eq!(
            goal("budget_limited", 0).snapshot.status,
            GoalStatus::BudgetLimited
        );
        assert_eq!(goal("complete", 0).snapshot.status, GoalStatus::Complete);
        assert_eq!(grok_goal_update(&update("rebooting", 0)), None);
        assert_eq!(
            grok_goal_update(
                &json!({ "sessionUpdate": "goal_updated", "status": "cleared", "phase": "idle" })
            ),
            Some(GrokGoalUpdate::Cleared)
        );
        assert_eq!(
            grok_goal_update(&json!({ "sessionUpdate": "subagent_progress" })),
            None
        );
    }

    #[test]
    fn token_counts_alone_wait_and_go_out_when_the_prompt_ends() {
        let start = Instant::now();
        let mut tracker = GrokGoalTracker::default();
        assert_eq!(observe(&mut tracker, goal("active", 0), start).len(), 1);
        assert!(tracker.is_active());
        assert!(observe(
            &mut tracker,
            goal("active", 900),
            start + Duration::from_secs(2)
        )
        .is_empty());
        let refreshed = observe(&mut tracker, goal("active", 1_800), start + TOKEN_REFRESH);
        assert!(matches!(
            refreshed.as_slice(),
            [GoalSignal::Snapshot { goal: Some(goal) }] if goal.tokens_used == Some(1_800)
        ));
        assert!(observe(&mut tracker, goal("active", 2_000), start + TOKEN_REFRESH).is_empty());
        let flushed = tracker.flush(start + TOKEN_REFRESH);
        assert!(matches!(
            flushed.as_slice(),
            [GoalSignal::Snapshot { goal: Some(goal) }] if goal.tokens_used == Some(2_000)
        ));
        assert!(tracker.flush(start + TOKEN_REFRESH).is_empty());
    }

    #[test]
    fn a_new_status_goes_out_at_once_with_the_pause_reason() {
        let start = Instant::now();
        let mut tracker = GrokGoalTracker::default();
        observe(&mut tracker, goal("active", 0), start);
        let mut paused = update("user_paused", 10);
        paused["pause_message"] = json!("Goal paused due to turn error: 429.");
        let signals = tracker.observe(grok_goal_update(&paused).expect("goal"), start);
        assert!(matches!(
            signals.as_slice(),
            [
                GoalSignal::Snapshot { goal: Some(goal) },
                GoalSignal::Paused { reason: Some(reason) },
            ] if goal.status == GoalStatus::Paused && reason.contains("turn error")
        ));
        assert!(!tracker.is_active());
    }

    #[test]
    fn clear_drops_the_goal() {
        let now = Instant::now();
        let mut tracker = GrokGoalTracker::default();
        observe(&mut tracker, goal("user_paused", 0), now);
        assert_eq!(
            tracker.observe(GrokGoalUpdate::Cleared, now),
            vec![GoalSignal::Cleared]
        );
        assert!(tracker.end().is_empty());
    }

    #[test]
    fn a_goal_ends_with_its_process_unless_it_was_complete() {
        let now = Instant::now();
        let mut tracker = GrokGoalTracker::default();
        assert!(tracker.end().is_empty());
        observe(&mut tracker, goal("active", 0), now);
        assert_eq!(tracker.end(), vec![GoalSignal::Cleared]);
        observe(&mut tracker, goal("complete", 0), now);
        assert!(tracker.end().is_empty());
    }
}
