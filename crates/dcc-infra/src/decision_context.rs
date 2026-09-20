//! Bounded evidence for decisions. Uses durable turn snapshots, never live workspace diffs.
use dcc_core::domain::session::{
    AssistantMessagePhase, SessionEventKind, SessionEventRecord, TurnChangeSet, TurnId,
};
use serde_json::json;

pub struct DecisionContext {
    pub text: String,
    pub truncated: bool,
}

/// Keep the beginning and end so a long request cannot hide its final constraints.
pub fn excerpt(text: &str, budget: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= budget {
        return text.to_string();
    }
    let half = budget.saturating_sub(32) / 2;
    format!(
        "{}\n[... content omitted ...]\n{}",
        chars[..half].iter().collect::<String>(),
        chars[chars.len() - half..].iter().collect::<String>()
    )
}

/// For a review, exclude the reviewed turn and all subsequent turns.
pub fn conversation_context(
    events: &[SessionEventRecord],
    before_turn: Option<&TurnId>,
) -> DecisionContext {
    let end = before_turn.and_then(|target| events.iter().position(|e| matches!(&e.kind, SessionEventKind::TurnStarted { turn_id, .. } if turn_id == target))).unwrap_or(events.len());
    let messages: Vec<_> = events[..end]
        .iter()
        .filter_map(|e| match &e.kind {
            SessionEventKind::TurnStarted { prompt, .. } => Some(("user", prompt.as_str())),
            SessionEventKind::TurnAssistantMessageCompleted {
                phase,
                content: Some(content),
                ..
            } if *phase != AssistantMessagePhase::Commentary => {
                Some(("assistant", content.as_str()))
            }
            _ => None,
        })
        .collect();
    let mut truncated = messages.len() > 6;
    let selected: Vec<_> = messages
        .iter()
        .enumerate()
        .filter(|(i, _)| *i == 0 || *i >= messages.len().saturating_sub(5))
        .map(|(_, (role, text))| {
            truncated |= text.chars().count() > 800;
            json!({"role": role, "text": excerpt(text, 800)})
        })
        .collect();
    DecisionContext {
        text: json!({"messages": selected, "history_incomplete": truncated}).to_string(),
        truncated,
    }
}

pub fn execution_evidence(
    events: &[SessionEventRecord],
    turn: &TurnId,
    snapshots: &[TurnChangeSet],
) -> DecisionContext {
    let mut tools: Vec<(String, serde_json::Value, String)> = Vec::new();
    let mut truncated = false;
    for event in events {
        match &event.kind {
            SessionEventKind::TurnToolCallStarted {
                turn_id,
                tool_call_id,
                action,
                command,
                file,
            } if turn_id == turn => {
                let metadata = json!({"action": excerpt(action, 200), "command": command.as_deref().map(|s| excerpt(s, 500)), "file": file.as_deref().map(|s| excerpt(s, 300)), "status": "started"});
                truncated |= action.chars().count() > 200
                    || command.as_ref().is_some_and(|s| s.chars().count() > 500)
                    || file.as_ref().is_some_and(|s| s.chars().count() > 300);
                tools.push((tool_call_id.clone(), metadata, String::new()));
            }
            SessionEventKind::TurnToolCallDelta {
                turn_id,
                tool_call_id,
                content,
            } if turn_id == turn => {
                if let Some((_, _, output)) =
                    tools.iter_mut().rev().find(|(id, _, _)| id == tool_call_id)
                {
                    output.push_str(content);
                    truncated |= output.chars().count() > 1_000;
                    *output = excerpt(output, 1_000);
                } else {
                    truncated = true;
                }
            }
            SessionEventKind::TurnToolCallCompleted {
                turn_id,
                tool_call_id,
            } if turn_id == turn => {
                if let Some((_, data, _)) =
                    tools.iter_mut().rev().find(|(id, _, _)| id == tool_call_id)
                {
                    data["status"] = json!("completed (does not imply tests passed)");
                } else {
                    truncated = true;
                }
            }
            SessionEventKind::TurnToolCallFailed {
                turn_id,
                tool_call_id,
                reason,
            } if turn_id == turn => {
                if let Some((_, data, _)) =
                    tools.iter_mut().rev().find(|(id, _, _)| id == tool_call_id)
                {
                    truncated |= reason.as_ref().is_some_and(|s| s.chars().count() > 500);
                    data["status"] = json!("failed");
                    data["reason"] = json!(reason.as_deref().map(|s| excerpt(s, 500)));
                } else {
                    truncated = true;
                }
            }
            _ => {}
        }
    }
    let tool_count = tools.len();
    truncated |= tool_count > 6;
    let tool_evidence: Vec<_> = tools
        .into_iter()
        .rev()
        .take(6)
        .map(|(_, mut data, output)| {
            data["output"] = json!(output);
            data
        })
        .collect();
    let matching: Vec<_> = snapshots.iter().filter(|s| &s.turn_id == turn).collect();
    let snapshot_count = matching.len();
    truncated |= snapshot_count > 2;
    let changes: Vec<_> = matching.into_iter().take(2).map(|snapshot| {
        truncated |= snapshot.diff_truncated || snapshot.files.len() > 12 || snapshot.file_diffs.len() > 4 || snapshot.observed_validations.len() > 8;
        truncated |= snapshot.files.iter().any(|f| f.path.chars().count() > 300)
            || snapshot.observed_validations.iter().any(|v| v.chars().count() > 300)
            || snapshot.error.as_ref().is_some_and(|e| e.chars().count() > 300);
        let diffs: Vec<_> = snapshot.file_diffs.iter().take(4).map(|(path, diff)| {
            truncated |= diff.chars().count() > 600 || path.chars().count() > 300;
            json!({"path": excerpt(path, 300), "diff": excerpt(diff, 600)})
        }).collect();
        json!({"state": snapshot.state, "outcome": snapshot.turn_outcome,
            "files": snapshot.files.iter().take(12).map(|f| excerpt(&f.path, 300)).collect::<Vec<_>>(),
            "diffs": diffs,
            "observed_validation_commands_not_proof_of_success": snapshot.observed_validations.iter().take(8).map(|s| excerpt(s, 300)).collect::<Vec<_>>(),
            "capture_error": snapshot.error.as_deref().map(|s| excerpt(s, 300))})
    }).collect();
    let text = json!({"tool_count": tool_count, "recent_tools": tool_evidence, "turn_snapshots": changes,
        "snapshot_available": snapshot_count > 0, "evidence_incomplete": truncated,
        "interpretation": "Observed execution only. Missing records cannot verify assistant claims. Validation command names and completed tool calls do not prove passing tests."}).to_string();
    truncated |= text.chars().count() > 16_000;
    DecisionContext { text, truncated }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::session::SessionId;

    fn event(kind: serde_json::Value) -> SessionEventRecord {
        SessionEventRecord {
            event_id: "e".into(),
            session_id: SessionId("s".into()),
            sequence: 0,
            occurred_at: "t".into(),
            kind: serde_json::from_value(kind).unwrap(),
        }
    }

    #[test]
    fn conversation_excludes_reviewed_and_future_turns() {
        let events = vec![
            event(json!({"type":"turn_started","turnId":"first","prompt":"original requirement"})),
            event(json!({"type":"turn_started","turnId":"reviewed","prompt":"do it"})),
            event(json!({"type":"turn_started","turnId":"future","prompt":"unrelated followup"})),
        ];
        let context = conversation_context(&events, Some(&TurnId("reviewed".into())));
        assert!(context.text.contains("original requirement"));
        assert!(!context.text.contains("do it"));
        assert!(!context.text.contains("unrelated followup"));
    }

    #[test]
    fn evidence_is_scoped_to_turn_and_keeps_failed_test_output() {
        let events = vec![
            event(
                json!({"type":"turn_tool_call_started","turnId":"t","toolCallId":"x","action":"shell","command":"cargo test"}),
            ),
            event(
                json!({"type":"turn_tool_call_delta","turnId":"t","toolCallId":"x","content":"test FAILED: expected true"}),
            ),
            event(json!({"type":"turn_tool_call_completed","turnId":"t","toolCallId":"x"})),
            event(
                json!({"type":"turn_tool_call_started","turnId":"other","toolCallId":"y","action":"unrelated action"}),
            ),
        ];
        let context = execution_evidence(&events, &TurnId("t".into()), &[]);
        assert!(context.text.contains("test FAILED"));
        assert!(context.text.contains("does not imply tests passed"));
        assert!(!context.text.contains("unrelated action"));
        assert!(context.text.contains("\"snapshot_available\":false"));
    }

    #[test]
    fn review_reads_only_snapshots_for_its_turn_and_marks_partial_diffs() {
        let snapshot = |turn: &str, diff: &str, partial: bool| {
            serde_json::from_value::<TurnChangeSet>(json!({
                "snapshotId":"snapshot", "sessionId":"s", "turnId":turn, "workspaceId":"w",
                "captureVersion":1, "state":"ready", "baseTree":null, "resultTree":null,
                "fileDiffs":{"src/main.rs":diff}, "diffTruncated":partial,
                "turnOutcome":"completed", "outcomeReason":null, "error":null,
                "createdAt":"t", "completedAt":"t"
            }))
            .unwrap()
        };
        let context = execution_evidence(
            &[],
            &TurnId("reviewed".into()),
            &[
                snapshot("other", "unrelated private content", false),
                snapshot("reviewed", "+actual change", true),
            ],
        );
        assert!(context.text.contains("+actual change"));
        assert!(!context.text.contains("unrelated private content"));
        assert!(context.truncated);
    }

    #[test]
    fn excerpt_handles_unicode_and_preserves_final_constraints() {
        let text = format!("start{}final constraint", "🦀".repeat(1_000));
        let cut = excerpt(&text, 100);
        assert!(cut.starts_with("start"));
        assert!(cut.ends_with("final constraint"));
        assert!(cut.chars().count() <= 100);
    }
}
