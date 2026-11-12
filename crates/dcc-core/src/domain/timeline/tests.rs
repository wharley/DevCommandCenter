//! The desktop's projection cases (`session-thread-history.logic.test.ts`),
//! written as the records the database holds.

use serde_json::{json, Value};

use super::{project_session, Annotation, DelegationPhase, HandBackOutcome, Role, TimelineMessage};
use crate::domain::provider::NativeSubagentStatus;
use crate::domain::session::{AssistantMessagePhase, SessionEventRecord};

/// Builds a session's records, one second apart.
struct Script {
    session: &'static str,
    records: Vec<SessionEventRecord>,
}

impl Script {
    fn new() -> Self {
        Self {
            session: "session-a",
            records: Vec::new(),
        }
    }

    fn at(&self) -> String {
        format!("2026-05-01T12:00:{:02}Z", self.records.len())
    }

    fn push(mut self, kind: Value) -> Self {
        let sequence = self.records.len() as u64 + 1;
        let record = json!({
            "eventId": format!("evt-{sequence}"),
            "sessionId": self.session,
            "sequence": sequence,
            "occurredAt": self.at(),
            "kind": kind,
        });
        self.records
            .push(serde_json::from_value(record).expect("record"));
        self
    }

    fn started(self, turn: &str, prompt: &str) -> Self {
        self.push(json!({ "type": "turn_started", "turnId": turn, "prompt": prompt }))
    }

    fn message_started(self, turn: &str, id: &str, phase: &str) -> Self {
        self.push(json!({ "type": "turn_assistant_message_started", "turnId": turn, "messageId": id, "phase": phase }))
    }

    fn message_delta(self, turn: &str, id: &str, content: &str) -> Self {
        self.push(json!({ "type": "turn_assistant_message_delta", "turnId": turn, "messageId": id, "content": content }))
    }

    fn message_completed(self, turn: &str, id: &str, phase: &str, content: Option<&str>) -> Self {
        self.push(json!({ "type": "turn_assistant_message_completed", "turnId": turn, "messageId": id, "phase": phase, "content": content }))
    }

    fn delta(self, turn: &str, content: &str) -> Self {
        self.push(json!({ "type": "turn_delta", "turnId": turn, "content": content }))
    }

    fn completed(self, turn: &str) -> Self {
        self.push(json!({ "type": "turn_completed", "turnId": turn }))
    }

    fn project(&self) -> Vec<TimelineMessage> {
        project_session(&self.records, self.session)
    }

    fn assistants(&self) -> Vec<TimelineMessage> {
        self.project()
            .into_iter()
            .filter(|message| message.role == Role::Assistant)
            .collect()
    }
}

fn commentary(annotation: &Annotation) -> (&str, bool) {
    match annotation {
        Annotation::Commentary {
            content, streaming, ..
        } => (content, *streaming),
        other => panic!("expected commentary, got {other:?}"),
    }
}

#[test]
fn live_commentary_folds_into_activity_until_the_turn_settles() {
    let script = Script::new()
        .started("turn-1", "Alpha")
        .message_started("turn-1", "comment-1", "commentary")
        .message_delta("turn-1", "comment-1", "Vou investigar.")
        .message_completed("turn-1", "comment-1", "commentary", Some("Vou investigar."))
        .message_started("turn-1", "final-1", "final_answer")
        .message_delta("turn-1", "final-1", "Resolvido.");
    let rows = script.assistants();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, "assistant-session-a-turn-1");
    assert_eq!(row.content, "");
    assert_eq!(
        row.assistant_phase,
        Some(AssistantMessagePhase::FinalAnswer)
    );
    assert!(row.streaming);
    assert_eq!(commentary(&row.annotations[0]), ("Vou investigar.", false));
    assert_eq!(commentary(&row.annotations[1]), ("Resolvido.", true));
}

#[test]
fn a_terminal_snapshot_under_another_id_closes_the_sole_active_item() {
    let script = Script::new()
        .started("turn-1", "Alpha")
        .message_started("turn-1", "stream-envelope", "unknown")
        .message_delta("turn-1", "stream-envelope", "Toda a ")
        .message_delta("turn-1", "stream-envelope", "mensagem")
        .message_completed(
            "turn-1",
            "msg-authoritative",
            "unknown",
            Some("Toda a mensagem, incluindo o final sem perder palavras."),
        )
        // A buffered delta under the original envelope, after the snapshot.
        .message_delta(
            "turn-1",
            "stream-envelope",
            " mensagem atrasada que não deve aparecer",
        );
    let rows = script.assistants();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].annotations.len(), 1);
    assert_eq!(
        commentary(&rows[0].annotations[0]).0,
        "Toda a mensagem, incluindo o final sem perder palavras."
    );
}

#[test]
fn ambiguous_phaseless_items_stay_one_live_row() {
    let script = Script::new()
        .started("turn-1", "Alpha")
        .message_started("turn-1", "stream-1", "unknown")
        .message_delta("turn-1", "stream-1", "Primeiro")
        .message_started("turn-1", "stream-2", "unknown")
        .message_delta("turn-1", "stream-2", "Segundo")
        .message_completed("turn-1", "terminal-3", "unknown", Some("Terceiro"));
    let rows = script.assistants();
    assert_eq!(rows.len(), 1);
    let texts: Vec<&str> = rows[0]
        .annotations
        .iter()
        .map(|item| commentary(item).0)
        .collect();
    assert_eq!(texts, ["Primeiro", "Segundo", "Terceiro"]);
}

#[test]
fn a_settled_turn_shows_its_authoritative_final_answer() {
    let script = Script::new()
        .started("turn-1", "Alpha")
        .message_started("turn-1", "comment-1", "commentary")
        .message_delta("turn-1", "comment-1", "Vou investigar.")
        .message_completed(
            "turn-1",
            "comment-1",
            "commentary",
            Some("Vou investigar com cuidado."),
        )
        .message_started("turn-1", "final-1", "final_answer")
        .message_delta("turn-1", "final-1", "Parcial")
        .message_completed(
            "turn-1",
            "final-1",
            "final_answer",
            Some("Resposta final autoritativa."),
        )
        .completed("turn-1");
    let rows = script.assistants();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].content, "Resposta final autoritativa.");
    assert!(!rows[0].streaming);
    assert!(rows[0].turn_settled);
    assert_eq!(rows[0].annotations.len(), 1);
    assert_eq!(
        commentary(&rows[0].annotations[0]).0,
        "Vou investigar com cuidado."
    );
}

#[test]
fn without_native_phases_the_last_completed_text_is_the_answer() {
    let script = Script::new()
        .started("turn-1", "Alpha")
        .message_started("turn-1", "segment-0", "unknown")
        .message_delta("turn-1", "segment-0", "Vou ler os arquivos.")
        .message_completed("turn-1", "segment-0", "unknown", None)
        .message_started("turn-1", "segment-1", "unknown")
        .message_delta("turn-1", "segment-1", "Encontrei a causa.")
        .message_completed("turn-1", "segment-1", "unknown", None)
        .completed("turn-1");
    let rows = script.assistants();
    assert_eq!(rows[0].content, "Encontrei a causa.");
    assert_eq!(
        commentary(&rows[0].annotations[0]).0,
        "Vou ler os arquivos."
    );
}

#[test]
fn reasoning_and_tool_calls_annotate_the_turn() {
    let rows = Script::new()
        .started("turn-1", "Alpha")
        .push(json!({ "type": "turn_reasoning_started", "turnId": "turn-1", "reasoningId": "reasoning-1", "label": null }))
        .push(json!({ "type": "turn_reasoning_delta", "turnId": "turn-1", "reasoningId": "reasoning-1", "content": "Thinking through the repo." }))
        .push(json!({ "type": "turn_reasoning_completed", "turnId": "turn-1", "reasoningId": "reasoning-1" }))
        .push(json!({ "type": "turn_tool_call_started", "turnId": "turn-1", "toolCallId": "tool-1", "action": "Read file", "command": "cat src/main.ts", "file": "src/main.ts" }))
        .push(json!({ "type": "turn_tool_call_delta", "turnId": "turn-1", "toolCallId": "tool-1", "content": "Reading the file." }))
        .push(json!({ "type": "turn_tool_call_completed", "turnId": "turn-1", "toolCallId": "tool-1", "detail": { "output": "ok", "exitCode": 0 } }))
        .push(json!({ "type": "turn_tool_call_started", "turnId": "turn-1", "toolCallId": "ask", "action": "AskUserQuestion", "command": null, "file": null }))
        .delta("turn-1", "Hello from assistant")
        .completed("turn-1")
        .project();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "user-session-a-turn-1");
    assert_eq!(rows[0].content, "Alpha");
    let row = &rows[1];
    assert_eq!(row.content, "Hello from assistant");
    assert_eq!(row.turn_started_at.as_deref(), Some("2026-05-01T12:00:00Z"));
    assert_eq!(row.turn_ended_at.as_deref(), Some("2026-05-01T12:00:09Z"));
    assert_eq!(
        row.annotations.len(),
        2,
        "AskUserQuestion is not a tool row"
    );
    match &row.annotations[0] {
        Annotation::Reasoning {
            label,
            content,
            streaming,
            ..
        } => {
            assert_eq!(label.as_deref(), Some("Thinking"));
            assert_eq!(content, "Thinking through the repo.");
            assert!(!streaming);
        }
        other => panic!("{other:?}"),
    }
    match &row.annotations[1] {
        Annotation::ToolCall(call) => {
            assert_eq!(call.action, "Read file");
            assert_eq!(call.content, "Reading the file.");
            assert!(!call.streaming);
            let detail = call.detail.as_ref().unwrap();
            assert_eq!(detail.output.as_deref(), Some("ok"));
            assert_eq!(detail.exit_code, Some(0));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_turn_aborted_before_any_output_still_gets_a_row() {
    let rows = Script::new()
        .started("turn-1", "Alpha")
        .push(json!({ "type": "usage_limit_reached", "turnId": "turn-1" }))
        .push(json!({ "type": "turn_aborted", "turnId": "turn-1", "reason": "limit" }))
        .push(json!({ "type": "usage_limit_resumed", "automatic": true, "accountLabel": "Zedy" }))
        .assistants();
    assert_eq!(rows.len(), 1);
    let status = rows[0].status.as_ref().unwrap();
    assert_eq!(status.reason.as_deref(), Some("limit"));
    assert!(status.usage_limited);
    assert_eq!(status.continued_on_account.as_deref(), Some("Zedy"));
    assert!(rows[0].turn_settled);
}

#[test]
fn approvals_and_questions_resolve_in_place() {
    let rows = Script::new()
        .started("turn-1", "Alpha")
        .push(json!({ "type": "turn_permission_requested", "turnId": "turn-1", "requestId": "p1", "toolName": "Bash", "title": null, "description": null, "command": "rm -rf", "file": null }))
        .push(json!({ "type": "turn_permission_resolved", "turnId": "turn-1", "requestId": "p1", "behavior": "deny" }))
        .push(json!({ "type": "turn_user_input_requested", "turnId": "turn-1", "requestId": "q1", "questions": [{ "id": "a", "header": "H", "question": "Qual?" }] }))
        .push(json!({ "type": "turn_user_input_resolved", "turnId": "turn-1", "requestId": "q1", "answers": [{ "question": "Qual?", "answer": "Esta" }] }))
        .assistants();
    match &rows[0].annotations[0] {
        Annotation::Approval {
            behavior,
            streaming,
            command,
            ..
        } => {
            assert_eq!(behavior.as_deref(), Some("deny"));
            assert_eq!(command.as_deref(), Some("rm -rf"));
            assert!(!streaming);
        }
        other => panic!("{other:?}"),
    }
    match &rows[0].annotations[1] {
        Annotation::UserInput {
            answers, streaming, ..
        } => {
            assert_eq!(answers[0].answer, "Esta");
            assert!(!streaming);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn native_subagents_merge_by_thread_and_take_confirmed_models() {
    let rows = Script::new()
        .started("turn-1", "Alpha")
        .push(json!({ "type": "turn_native_subagent_model_requested", "turnId": "turn-1", "correlationId": "thread-1", "model": "small" }))
        .push(json!({ "type": "turn_native_subagent_activity", "turnId": "turn-1", "id": "a1", "agentId": null, "agentThreadId": "thread-1", "name": "explorer", "role": null, "model": null, "status": "running" }))
        .push(json!({ "type": "turn_native_subagent_activity", "turnId": "turn-1", "id": "a2", "agentId": null, "agentThreadId": "thread-1", "name": null, "role": null, "model": null, "status": "completed" }))
        .push(json!({ "type": "turn_native_subagent_model_confirmed", "turnId": "turn-1", "correlationId": "thread-1", "model": "big" }))
        .push(json!({ "type": "turn_model_effective", "turnId": "turn-1", "model": "parent-model" }))
        .assistants();
    assert_eq!(rows[0].annotations.len(), 1);
    match &rows[0].annotations[0] {
        Annotation::NativeSubagent(agent) => {
            assert_eq!(agent.id, "a1");
            assert_eq!(agent.name.as_deref(), Some("explorer"));
            assert_eq!(agent.status, NativeSubagentStatus::Completed);
            assert_eq!(agent.requested_model.as_deref(), Some("small"));
            assert_eq!(agent.model.as_deref(), Some("big"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(rows[0].model.as_deref(), Some("parent-model"));
}

#[test]
fn delegation_cards_follow_the_turn_that_asked_for_them() {
    let rows = Script::new()
        .started("turn-1", "Alpha")
        .push(json!({ "type": "delegation_requested", "delegationId": "d1" }))
        .push(json!({ "type": "delegation_started", "delegationId": "d1", "childSessionId": "child" }))
        .delta("turn-1", "Pedi ajuda.")
        .push(json!({ "type": "delegation_completed", "delegationId": "d1", "summary": "Feito." }))
        .completed("turn-1")
        .project();
    let roles: Vec<Role> = rows.iter().map(|row| row.role).collect();
    assert_eq!(roles, [Role::User, Role::Assistant, Role::System]);
    let card = rows[2].delegation.as_ref().unwrap();
    assert_eq!(card.phase, DelegationPhase::Completed);
    assert_eq!(card.child_session_id.as_deref(), Some("child"));
    assert_eq!(rows[2].content, "Feito.");
    assert_eq!(rows[2].action.as_ref().unwrap().label, "Open child session");
}

#[test]
fn the_conversation_starts_with_the_prompt_and_shows_forks_and_pauses() {
    let plain = Script::new()
        .push(json!({ "type": "session_started", "workspaceId": "w", "projectId": "p", "providerId": "claude", "model": null }))
        .started("turn-1", "Alpha")
        .project();
    assert_eq!(plain[0].role, Role::User);
    let forked = Script::new()
        .push(json!({ "type": "session_started", "workspaceId": "w", "projectId": "p", "providerId": "claude", "model": null, "forkedFrom": { "sessionId": "source", "turnId": "t0" } }))
        .push(json!({ "type": "objective_paused", "reason": "turn_budget", "consecutiveFailures": 0, "turnsUsed": 12 }))
        .project();
    assert_eq!(forked[0].label, "session.forked");
    assert_eq!(forked[0].action.as_ref().unwrap().session_id, "source");
    assert!(forked[0].content.contains("at one of its turns"));
    assert_eq!(
        forked[1].content,
        "Objective paused automatically: turn budget reached (12). Queued follow-ups stop until you resume it."
    );
}

#[test]
fn rewound_turns_leave_the_conversation() {
    let rows = Script::new()
        .started("turn-1", "Alpha")
        .delta("turn-1", "Um")
        .completed("turn-1")
        .started("turn-2", "Beta")
        .delta("turn-2", "Dois")
        .push(json!({ "type": "conversation_rewound", "anchorTurnId": "turn-2", "removedTurnIds": ["turn-2"], "providerContext": "native", "restoredTurnIds": [] }))
        .project();
    let contents: Vec<&str> = rows.iter().map(|row| row.content.as_str()).collect();
    assert_eq!(
        contents,
        [
            "Alpha",
            "Um",
            "Edited from an earlier message: 1 turn left the conversation; the agent's own conversation was cut at the same point; files kept as they were."
        ]
    );
}

#[test]
fn consecutive_deltas_join_and_other_sessions_stay_out() {
    let mut script = Script::new()
        .started("turn-1", "Alpha")
        .delta("turn-1", "Vou")
        .delta("turn-1", " ler");
    script.records.push(serde_json::from_value(json!({
        "eventId": "other", "sessionId": "session-b", "sequence": 99, "occurredAt": "2026-05-01T12:00:30Z",
        "kind": { "type": "turn_started", "turnId": "x", "prompt": "Outra" }
    })).unwrap());
    let rows = script.delta("turn-1", " já").completed("turn-1").project();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].content, "Vou ler já");
}

#[test]
fn hand_back_turns_are_recognised() {
    let rows = Script::new()
        .started(
            "turn-1",
            "[DCC] Delegated review task finished — summary here (task d-42).\nmore",
        )
        .project();
    let hand_back = rows[0].hand_back.as_ref().unwrap();
    assert_eq!(hand_back.delegation_id, "d-42");
    assert_eq!(hand_back.outcome, HandBackOutcome::Finished);
    assert_eq!(
        super::parse_delegation_hand_back("[DCC] The human DISCARDED the edits from your delegated fix task — nothing kept (task 7).")
            .unwrap()
            .outcome,
        HandBackOutcome::Discarded
    );
    assert_eq!(
        super::parse_delegation_hand_back("[DCC] Task \"Fase 1\" ended a turn — see (task t9).")
            .unwrap()
            .delegation_id,
        "t9"
    );
    assert!(super::parse_delegation_hand_back("Olá").is_none());
}

#[test]
fn only_open_permission_requests_wait_in_the_panel() {
    let script = Script::new()
        .started("turn-1", "Alpha")
        .push(json!({ "type": "turn_permission_requested", "turnId": "turn-1", "requestId": "p1", "toolName": "Bash", "title": null, "description": null, "command": "ls", "file": null }))
        .push(json!({ "type": "turn_permission_resolved", "turnId": "turn-1", "requestId": "p1", "behavior": "allow" }))
        .push(json!({ "type": "turn_permission_requested", "turnId": "turn-1", "requestId": "p2", "toolName": "Edit", "title": null, "description": null, "command": null, "file": "a.rs" }));
    let rows = script.project();
    let pending = super::pending_permission_requests(&rows);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id(), "p2");
}

#[test]
fn text_after_a_tool_call_starts_a_new_paragraph() {
    let rows = Script::new()
        .started("turn-1", "Alpha")
        .delta("turn-1", "Vou ler a doc")
        .delta("turn-1", "umentação.")
        .push(json!({ "type": "turn_tool_call_started", "turnId": "turn-1", "toolCallId": "t1", "action": "WebFetch", "command": null, "file": null }))
        .push(json!({ "type": "turn_tool_call_completed", "turnId": "turn-1", "toolCallId": "t1" }))
        .delta("turn-1", "WebFetch foi bloqueado.\n")
        .push(json!({ "type": "turn_reasoning_started", "turnId": "turn-1", "reasoningId": "r1", "label": null }))
        .delta("turn-1", "Achei.")
        .push(json!({ "type": "turn_tool_call_started", "turnId": "turn-1", "toolCallId": "t2", "action": "Read file", "command": null, "file": null }))
        .delta("turn-1", "\n\nPronto.")
        .completed("turn-1")
        .assistants();
    assert_eq!(
        rows[0].content,
        "Vou ler a documentação.\n\nWebFetch foi bloqueado.\n\nAchei.\n\nPronto."
    );
}

#[test]
fn a_message_sent_mid_turn_reads_where_it_was_sent() {
    let tool = |id: &str| json!({ "type": "turn_tool_call_started", "turnId": "turn-1", "toolCallId": id, "action": "Run", "command": "ls", "file": null });
    let done = |id: &str| json!({ "type": "turn_tool_call_completed", "turnId": "turn-1", "toolCallId": id, "detail": { "output": "ok", "exitCode": 0 } });
    let script = Script::new()
        .started("turn-1", "Alpha")
        .message_started("turn-1", "comment-1", "commentary")
        .message_delta("turn-1", "comment-1", "Vou investigar.")
        .message_completed("turn-1", "comment-1", "commentary", Some("Vou investigar."))
        .push(tool("tool-1"))
        .push(json!({ "type": "turn_steered", "turnId": "turn-1", "prompt": "pode seguir nela" }))
        .push(done("tool-1"))
        .push(tool("tool-2"));
    let rows = script.project();
    let roles: Vec<(Role, &str)> = rows.iter().map(|row| (row.role, row.id.as_str())).collect();
    assert_eq!(
        roles,
        vec![
            (Role::User, "user-session-a-turn-1"),
            (Role::Assistant, "assistant-session-a-turn-1-before-1"),
            (Role::User, "user-steer-session-a-turn-1-1"),
            (Role::Assistant, "assistant-session-a-turn-1"),
        ]
    );
    // What came before the message is done; the call it was running ends there.
    let before = &rows[1];
    assert!(!before.streaming);
    assert!(before.annotations.iter().any(|annotation| matches!(
        annotation,
        Annotation::ToolCall(call) if call.id == "tool-1" && !call.streaming
    )));
    // The turn goes on below the message.
    let after = &rows[3];
    assert!(after.streaming);
    assert!(after.annotations.iter().any(|annotation| matches!(
        annotation,
        Annotation::ToolCall(call) if call.id == "tool-2"
    )));
    // Once the turn ends, both stay where they were.
    let settled = script
        .message_started("turn-1", "final-1", "final_answer")
        .message_delta("turn-1", "final-1", "Pronto.")
        .message_completed("turn-1", "final-1", "final_answer", Some("Pronto."))
        .completed("turn-1")
        .project();
    assert_eq!(settled.len(), 4);
    assert_eq!(settled[3].content, "Pronto.");
    assert!(!settled[3].streaming);
}

#[test]
fn a_streamed_turn_split_by_a_message_keeps_its_text_above_it() {
    let rows = Script::new()
        .started("turn-1", "Alpha")
        .delta("turn-1", "Primeira parte.")
        .push(json!({ "type": "turn_steered", "turnId": "turn-1", "prompt": "e o banco?" }))
        .delta("turn-1", "Segunda parte.")
        .completed("turn-1")
        .project();
    let texts: Vec<(Role, &str)> = rows
        .iter()
        .map(|row| (row.role, row.content.as_str()))
        .collect();
    assert_eq!(
        texts,
        vec![
            (Role::User, "Alpha"),
            (Role::Assistant, "Primeira parte."),
            (Role::User, "e o banco?"),
            (Role::Assistant, "Segunda parte."),
        ]
    );
}
