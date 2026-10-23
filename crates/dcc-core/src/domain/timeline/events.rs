//! The events a projection folds: one session's records, rewound turns left
//! out, and each run of streaming deltas to the same item joined into one.

use crate::domain::session::{visible_session_events, SessionEventKind, SessionEventRecord};

/// One event to fold. A joined delta run carries the whole text in
/// `content` and the first delta's time.
pub(super) struct TimelineEvent {
    pub kind: SessionEventKind,
    pub occurred_at: String,
}

/// The item a streaming delta appends to, or `None` for any other event.
fn delta_target(kind: &SessionEventKind) -> Option<(u8, &str, &str)> {
    match kind {
        SessionEventKind::TurnDelta { turn_id, .. } => Some((0, &turn_id.0, "")),
        SessionEventKind::TurnAssistantMessageDelta {
            turn_id,
            message_id,
            ..
        } => Some((1, &turn_id.0, message_id)),
        SessionEventKind::TurnReasoningDelta {
            turn_id,
            reasoning_id,
            ..
        } => Some((2, &turn_id.0, reasoning_id)),
        SessionEventKind::TurnToolCallDelta {
            turn_id,
            tool_call_id,
            ..
        } => Some((3, &turn_id.0, tool_call_id)),
        _ => None,
    }
}

fn delta_content_mut(kind: &mut SessionEventKind) -> Option<&mut String> {
    match kind {
        SessionEventKind::TurnDelta { content, .. }
        | SessionEventKind::TurnAssistantMessageDelta { content, .. }
        | SessionEventKind::TurnReasoningDelta { content, .. }
        | SessionEventKind::TurnToolCallDelta { content, .. } => Some(content),
        _ => None,
    }
}

fn delta_content(kind: &SessionEventKind) -> &str {
    match kind {
        SessionEventKind::TurnDelta { content, .. }
        | SessionEventKind::TurnAssistantMessageDelta { content, .. }
        | SessionEventKind::TurnReasoningDelta { content, .. }
        | SessionEventKind::TurnToolCallDelta { content, .. } => content,
        _ => "",
    }
}

/// `compactDeltaRuns`: deltas only append to their item, so a run of
/// consecutive deltas to the same item folds exactly like one delta with the
/// joined text. Runs never cross another event.
pub(super) fn timeline(records: &[SessionEventRecord], session_id: &str) -> Vec<TimelineEvent> {
    let own: Vec<SessionEventRecord> = records
        .iter()
        .filter(|record| record.session_id.0 == session_id)
        .cloned()
        .collect();
    let visible = visible_session_events(&own);
    let mut compacted: Vec<TimelineEvent> = Vec::with_capacity(visible.len());
    let mut index = 0;
    while index < visible.len() {
        let first = &visible[index];
        let target = delta_target(&first.kind);
        let mut end = index + 1;
        if target.is_some() {
            while end < visible.len() && delta_target(&visible[end].kind) == target {
                end += 1;
            }
        }
        let mut kind = first.kind.clone();
        if end - index > 1 {
            let joined: String = visible[index..end]
                .iter()
                .map(|record| delta_content(&record.kind))
                .collect();
            if let Some(content) = delta_content_mut(&mut kind) {
                *content = joined;
            }
        }
        compacted.push(TimelineEvent {
            kind,
            occurred_at: first.occurred_at.clone(),
        });
        index = end;
    }
    compacted
}
