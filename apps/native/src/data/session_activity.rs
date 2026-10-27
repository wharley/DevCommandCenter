//! The Session tab's activity (`SessionEventFeed`, `DelegationsSection`):
//! each event's raw label, tone, milestone (title and line, as i18n keys or
//! text) and the diagnostics' payload line; which ones are milestones; and
//! the delegations of one turn compared side by side.

use dcc_core::domain::session::{SessionEventKind, SessionEventRecord};
use serde::Serialize;

use super::conversation::DelegationRecord;

/// `eventTone`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Success,
    Warn,
    Secondary,
    Outline,
}

/// Text from a key (with its arguments) or as it came.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Text {
    Key(&'static str, Vec<(&'static str, String)>),
    Owned(String),
    /// `{session} · {t(key)}`
    Prefixed(String, &'static str),
}

impl Text {
    fn key(key: &'static str) -> Self {
        Self::Key(key, Vec::new())
    }
}

fn snake<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// `isSemanticSessionEvent`: streaming and successful tool internals are
/// the diagnostics' only.
pub fn is_milestone(event: &SessionEventRecord) -> bool {
    !matches!(
        event.kind,
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
    )
}

/// `eventLabel`
pub fn label(event: &SessionEventRecord) -> &'static str {
    use SessionEventKind as K;
    match &event.kind {
        K::SessionStarted { .. } => "session.started",
        K::SessionCompleted => "session.completed",
        K::SessionAborted { .. } => "session.aborted",
        K::SessionResumed => "session.resumed",
        K::ObjectivePaused { .. } => "session.objective.paused",
        K::TurnStarted { .. } => "session.turn.started",
        K::TurnDelta { .. } => "session.turn.delta",
        K::TurnAssistantMessageStarted { .. } => "session.turn.assistant-message.started",
        K::TurnAssistantMessageDelta { .. } => "session.turn.assistant-message.delta",
        K::TurnAssistantMessageCompleted { .. } => "session.turn.assistant-message.completed",
        K::TurnReasoningStarted { .. } => "session.turn.reasoning.started",
        K::TurnReasoningDelta { .. } => "session.turn.reasoning.delta",
        K::TurnReasoningCompleted { .. } => "session.turn.reasoning.completed",
        K::TurnToolCallStarted { .. } => "session.turn.tool-call.started",
        K::TurnToolCallDelta { .. } => "session.turn.tool-call.delta",
        K::TurnToolCallUpdated { .. } => "session.turn.tool-call.updated",
        K::TurnToolCallCompleted { .. } => "session.turn.tool-call.completed",
        K::TurnToolCallFailed { .. } => "session.turn.tool-call.failed",
        K::TurnNativeSubagentActivity { .. } => "session.turn.native-subagent.activity",
        K::TurnCompleted { .. } => "session.turn.completed",
        K::TurnAborted { .. } => "session.turn.aborted",
        K::CheckpointCreated { .. } => "session.checkpoint.created",
        K::PlanApproved { .. } => "session.plan.approved",
        K::PlanHandedOff { .. } => "session.plan.handed-off",
        _ => "event",
    }
}

/// `eventTone`
pub fn tone(event: &SessionEventRecord) -> Tone {
    use SessionEventKind as K;
    match &event.kind {
        K::SessionCompleted | K::PlanApproved { .. } | K::PlanHandedOff { .. } => Tone::Success,
        K::SessionAborted { .. } | K::TurnAborted { .. } | K::CheckpointCreated { .. } => {
            Tone::Warn
        }
        K::SessionStarted { .. } | K::SessionResumed => Tone::Secondary,
        _ => Tone::Outline,
    }
}

/// `semanticEventPresentation`: the milestone's title and line.
pub fn presentation(event: &SessionEventRecord) -> (&'static str, Text) {
    use SessionEventKind as K;
    let milestone = |name| match name {
        "sessionStarted" => "sessionEventFeed.milestones.sessionStarted",
        "sessionResumed" => "sessionEventFeed.milestones.sessionResumed",
        "objectivePaused" => "sessionEventFeed.milestones.objectivePaused",
        "turnStarted" => "sessionEventFeed.milestones.turnStarted",
        "toolFailed" => "sessionEventFeed.milestones.toolFailed",
        "turnCompleted" => "sessionEventFeed.milestones.turnCompleted",
        "turnAborted" => "sessionEventFeed.milestones.turnAborted",
        "checkpointCreated" => "sessionEventFeed.milestones.checkpointCreated",
        "planApproved" => "sessionEventFeed.milestones.planApproved",
        "planHandedOff" => "sessionEventFeed.milestones.planHandedOff",
        "sessionCompleted" => "sessionEventFeed.milestones.sessionCompleted",
        "sessionAborted" => "sessionEventFeed.milestones.sessionAborted",
        _ => "sessionEventFeed.milestones.event",
    };
    let or = |reason: &Option<String>, fallback| {
        reason
            .clone()
            .map_or_else(|| Text::key(fallback), Text::Owned)
    };
    match &event.kind {
        K::SessionStarted { provider_id, .. } => (
            milestone("sessionStarted"),
            Text::Key(
                "sessionEventFeed.details.sessionStarted",
                vec![("provider", provider_id.clone())],
            ),
        ),
        K::SessionResumed => (
            milestone("sessionResumed"),
            Text::key("sessionEventFeed.details.sessionResumed"),
        ),
        K::ObjectivePaused {
            reason,
            consecutive_failures,
            turns_used,
        } => {
            let key = match snake(reason).as_str() {
                "consecutive_failures" => {
                    "sessionEventFeed.details.objectivePaused.consecutive_failures"
                }
                "turn_budget" => "sessionEventFeed.details.objectivePaused.turn_budget",
                _ => "sessionEventFeed.details.objectivePaused.manual",
            };
            (
                milestone("objectivePaused"),
                Text::Key(
                    key,
                    vec![
                        ("failures", consecutive_failures.to_string()),
                        ("turns", turns_used.to_string()),
                    ],
                ),
            )
        }
        K::TurnStarted { prompt, .. } => (milestone("turnStarted"), Text::Owned(prompt.clone())),
        K::TurnToolCallFailed { reason, .. } => (
            milestone("toolFailed"),
            or(reason, "sessionEventFeed.details.noFailureReason"),
        ),
        K::TurnCompleted { .. } => (
            milestone("turnCompleted"),
            Text::key("sessionEventFeed.details.turnCompleted"),
        ),
        K::TurnAborted { reason, .. } => (
            milestone("turnAborted"),
            or(reason, "sessionEventFeed.details.noAbortReason"),
        ),
        K::CheckpointCreated { label, .. } => {
            (milestone("checkpointCreated"), Text::Owned(label.clone()))
        }
        K::PlanApproved { plan_version, .. } => (
            milestone("planApproved"),
            Text::Key(
                "sessionEventFeed.details.planApproved",
                vec![("version", plan_version.to_string())],
            ),
        ),
        K::PlanHandedOff { plan_version, .. } => (
            milestone("planHandedOff"),
            Text::Key(
                "sessionEventFeed.details.planHandedOff",
                vec![("version", plan_version.to_string())],
            ),
        ),
        K::SessionCompleted => (
            milestone("sessionCompleted"),
            Text::key("sessionEventFeed.details.sessionCompleted"),
        ),
        K::SessionAborted { reason } => (
            milestone("sessionAborted"),
            or(reason, "sessionEventFeed.details.noAbortReason"),
        ),
        _ => (
            milestone("event"),
            Text::key("sessionEventFeed.details.event"),
        ),
    }
}

fn preview(content: &str) -> String {
    if content.chars().count() > 60 {
        format!("{}…", content.chars().take(60).collect::<String>())
    } else {
        content.to_string()
    }
}

/// `eventPayloadSummary`, for the diagnostics.
pub fn payload(event: &SessionEventRecord) -> Text {
    use SessionEventKind as K;
    let session = &event.session_id.0;
    let with = |rest: &str| Text::Owned(format!("{session} · {rest}"));
    match &event.kind {
        K::SessionStarted { provider_id, .. } => with(provider_id),
        K::TurnStarted { prompt, .. } => with(prompt),
        K::TurnDelta { content, .. } => with(&preview(content)),
        K::TurnReasoningStarted {
            label,
            reasoning_id,
            ..
        } => with(label.as_deref().unwrap_or(reasoning_id)),
        K::TurnReasoningDelta { content, .. } => with(&preview(content)),
        K::TurnToolCallStarted { action, .. } => with(action),
        K::TurnToolCallFailed { reason, .. } | K::TurnAborted { reason, .. } => match reason {
            Some(reason) => with(reason),
            None => Text::Prefixed(
                session.clone(),
                if matches!(event.kind, K::TurnAborted { .. }) {
                    "sessionEventFeed.details.noAbortReason"
                } else {
                    "sessionEventFeed.details.noFailureReason"
                },
            ),
        },
        K::SessionAborted { reason } => match reason {
            Some(reason) => with(reason),
            None => Text::Prefixed(session.clone(), "sessionEventFeed.details.noAbortReason"),
        },
        K::CheckpointCreated { label, .. } => with(label),
        K::PlanApproved {
            plan_version,
            plan_hash,
            ..
        } => with(&format!("v{plan_version} · {plan_hash}")),
        K::PlanHandedOff {
            plan_version,
            action,
            ..
        } => with(&format!("v{plan_version} · {action}")),
        _ => Text::Owned(session.clone()),
    }
}

/// `delegationStatus`'s raw value (`review_pending`…).
pub fn status_key(delegation: &DelegationRecord) -> String {
    snake(&delegation.status)
}

/// One turn's delegations to the same task (`buildDelegationComparisonGroups`):
/// only groups of two or more, at most three.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub count: usize,
    pub providers: Vec<String>,
    /// `(status, count)` in first-seen order.
    pub statuses: Vec<(String, usize)>,
    pub files: Vec<String>,
}

pub fn comparisons(
    delegations: &[DelegationRecord],
    provider_label: impl Fn(&str) -> String,
) -> Vec<Comparison> {
    let mut groups: Vec<(String, Vec<&DelegationRecord>)> = Vec::new();
    for delegation in delegations {
        let key = format!(
            "{}:{}:{}",
            delegation
                .parent_turn_id
                .as_deref()
                .unwrap_or(&delegation.parent_session_id),
            delegation.mode,
            delegation.prompt
        );
        match groups.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, members)) => members.push(delegation),
            None => groups.push((key, vec![delegation])),
        }
    }
    groups
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .take(3)
        .map(|(_, members)| {
            let mut comparison = Comparison {
                count: members.len(),
                providers: Vec::new(),
                statuses: Vec::new(),
                files: Vec::new(),
            };
            for member in members {
                let label = provider_label(&member.provider_id);
                if !comparison.providers.contains(&label) {
                    comparison.providers.push(label);
                }
                let status = status_key(member);
                match comparison
                    .statuses
                    .iter_mut()
                    .find(|(existing, _)| *existing == status)
                {
                    Some((_, count)) => *count += 1,
                    None => comparison.statuses.push((status, 1)),
                }
                for file in &member.touched_files {
                    if !comparison.files.contains(file) {
                        comparison.files.push(file.clone());
                    }
                }
            }
            comparison
        })
        .collect()
}

/// The section's list: newest first, the one under review in front, six.
pub fn visible(delegations: &[DelegationRecord], reviewing: Option<&str>) -> Vec<DelegationRecord> {
    let mut sorted = delegations.to_vec();
    sorted.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    if let Some(index) = reviewing.and_then(|id| sorted.iter().position(|item| item.id == id)) {
        let item = sorted.remove(index);
        sorted.insert(0, item);
    }
    sorted.truncate(6);
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::session::SessionId;

    fn event(kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: "e".into(),
            session_id: SessionId("s-1".into()),
            sequence: 1,
            occurred_at: String::new(),
            kind,
        }
    }

    #[test]
    fn events_read_like_the_desktop() {
        let aborted = event(SessionEventKind::TurnAborted {
            turn_id: dcc_core::domain::session::TurnId("t".into()),
            reason: None,
        });
        assert_eq!(label(&aborted), "session.turn.aborted");
        assert_eq!(tone(&aborted), Tone::Warn);
        assert!(is_milestone(&aborted));
        assert_eq!(
            presentation(&aborted),
            (
                "sessionEventFeed.milestones.turnAborted",
                Text::key("sessionEventFeed.details.noAbortReason")
            )
        );
        let delta = event(SessionEventKind::TurnDelta {
            turn_id: dcc_core::domain::session::TurnId("t".into()),
            content: "x".repeat(70),
        });
        assert!(!is_milestone(&delta));
        assert_eq!(
            payload(&delta),
            Text::Owned(format!("s-1 · {}…", "x".repeat(60)))
        );
        assert_eq!(
            label(&event(SessionEventKind::SessionCompleted)),
            "session.completed"
        );
    }

    #[test]
    fn one_turns_delegations_are_compared() {
        let make = |id: &str, provider: &str, updated: &str| DelegationRecord {
            id: id.into(),
            mode: "review".into(),
            prompt: "Revise".into(),
            parent_session_id: "p".into(),
            parent_turn_id: Some("t-1".into()),
            provider_id: provider.into(),
            updated_at: updated.into(),
            touched_files: vec!["a.rs".into()],
            ..DelegationRecord::default()
        };
        let list = [make("d-1", "codex", "1"), make("d-2", "claude", "2")];
        let groups = comparisons(&list, |id| id.to_uppercase());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].providers, ["CODEX", "CLAUDE"]);
        assert_eq!(groups[0].files, ["a.rs"]);
        let shown = visible(&list, Some("d-1"));
        assert_eq!(shown[0].id, "d-1");
        assert_eq!(visible(&list, None)[0].id, "d-2");
    }
}
