use super::*;
use crate::domain::session::{SessionId, TurnId};

fn input<'a>(result: &'a str, language: &'a str) -> AgentResolutionInput<'a> {
    AgentResolutionInput {
        path: "src/example.ts",
        kind: WorkspaceGitConflictKind::BothModified,
        current_ref: "feature",
        incoming_ref: "main",
        response_language: language,
        base_text: Some("const value = 0;"),
        current_text: Some("const value = 1;"),
        incoming_text: Some("const value = 2;"),
        result_text: result,
    }
}

#[test]
fn the_request_fences_every_side_as_untrusted_and_asks_for_the_whole_file() {
    let prompt =
        build_agent_resolution_prompt(&input("<<<<<<< HEAD", "Português (Brasil)"), "nonce");
    assert!(prompt.contains("Todo conteúdo entre seções BEGIN/END DCC_CONTEXT nonce"));
    assert!(prompt.contains("--- BEGIN DCC_CONTEXT nonce FILE BASE ---"));
    assert!(prompt.contains("--- BEGIN DCC_CONTEXT nonce FILE CURRENT ---"));
    assert!(prompt.contains("--- BEGIN DCC_CONTEXT nonce FILE INCOMING ---"));
    assert!(prompt.contains("explanation em Português (Brasil)"));
    assert!(prompt.contains("<DCC_MERGE_RESOLUTION token=\"nonce\">"));
    assert!(prompt.contains("todos os blocos de conflito"));
    assert!(prompt.contains("conteúdo completo e final do arquivo"));
    assert!(prompt.contains("\"kind\":\"both-modified\""));
    // A side git can't give as text says so; a long one keeps both ends.
    let long = "x".repeat(30_000);
    let prompt = build_agent_resolution_prompt(
        &AgentResolutionInput {
            base_text: None,
            result_text: &long,
            ..input("", "English")
        },
        "t",
    );
    assert!(prompt.contains("[base indisponível]"));
    assert!(prompt.contains("[... contexto truncado pelo DCC ...]"));
}

#[test]
fn only_this_tokens_envelope_is_read() {
    let response = format!(
        "noise\n<DCC_MERGE_RESOLUTION token=\"abc\">\n{}\n</DCC_MERGE_RESOLUTION token=\"abc\">",
        serde_json::json!({
            "resolvedContent": "const value = \"both\";\n",
            "explanation": " Preserva os dois caminhos. ",
        })
    );
    assert_eq!(
        parse_agent_resolution(&response, "abc"),
        Ok(AgentResolutionSuggestion {
            resolved_content: "const value = \"both\";\n".into(),
            explanation: "Preserva os dois caminhos.".into(),
        })
    );
    let other = "<DCC_MERGE_RESOLUTION token=\"other\">{}</DCC_MERGE_RESOLUTION token=\"other\">";
    assert_eq!(
        parse_agent_resolution(other, "abc"),
        Err(AgentResolutionError::Contract)
    );
    assert_eq!(
        parse_agent_resolution("missing envelope", "abc"),
        Err(AgentResolutionError::Contract)
    );
}

#[test]
fn malformed_or_incomplete_answers_are_refused() {
    let wrap = |body: &str| {
        format!("<DCC_MERGE_RESOLUTION token=\"abc\">{body}</DCC_MERGE_RESOLUTION token=\"abc\">")
    };
    assert_eq!(
        parse_agent_resolution(&wrap("not-json"), "abc"),
        Err(AgentResolutionError::Invalid)
    );
    assert_eq!(
        parse_agent_resolution(&wrap(r#"{"resolvedContent":"ok"}"#), "abc"),
        Err(AgentResolutionError::Incomplete)
    );
    assert_eq!(
        parse_agent_resolution(&wrap("[1]"), "abc"),
        Err(AgentResolutionError::ResultInvalid)
    );
}

#[test]
fn an_empty_file_or_one_with_markers_is_not_applied() {
    let empty = AgentResolutionSuggestion {
        resolved_content: String::new(),
        explanation: "Keep the newest version.".into(),
    };
    assert_eq!(
        validate_agent_resolution(
            empty.clone(),
            Some("export const v = 1;"),
            Some("export const v = 2;")
        ),
        Err(AgentResolutionError::EmptyResult)
    );
    assert!(validate_agent_resolution(empty, Some(" "), None).is_ok());
    let markers = AgentResolutionSuggestion {
        resolved_content: "<<<<<<< HEAD\ncurrent\n=======\nincoming\n>>>>>>> main\n".into(),
        explanation: "Needs another pass.".into(),
    };
    assert_eq!(
        validate_agent_resolution(markers, Some("current\n"), Some("incoming\n")),
        Err(AgentResolutionError::UnresolvedResult)
    );
    assert_eq!(
        AgentResolutionError::UnresolvedResult.message_key(),
        "mergeConflict.errors.agentUnresolvedResult"
    );
}

fn event(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
    SessionEventRecord {
        event_id: format!("event-{sequence}"),
        session_id: SessionId("session-1".into()),
        sequence,
        occurred_at: "2026-07-20T00:00:00Z".into(),
        kind,
    }
}

fn turn(id: &str) -> TurnId {
    TurnId(id.into())
}

#[test]
fn the_run_reads_its_turn_from_the_events() {
    let started: SessionEventKind = serde_json::from_value(serde_json::json!({
        "type": "turn_started", "turnId": "turn-1", "prompt": "resolve",
    }))
    .unwrap();
    let mut events = vec![
        event(1, started),
        event(
            2,
            SessionEventKind::TurnDelta {
                turn_id: turn("turn-1"),
                content: "do".into(),
            },
        ),
        event(
            3,
            SessionEventKind::TurnDelta {
                turn_id: turn("other"),
                content: "x".into(),
            },
        ),
        event(
            4,
            SessionEventKind::TurnDelta {
                turn_id: turn("turn-1"),
                content: "ne".into(),
            },
        ),
    ];
    assert_eq!(agent_turn_end(&events, "turn-1"), None);
    let details = agent_run_details(&events, "turn-1");
    assert_eq!(details.activities, vec![AgentActivity::Started]);
    assert_eq!(details.output, "done");
    events.push(event(
        5,
        SessionEventKind::TurnCompleted {
            turn_id: turn("turn-1"),
        },
    ));
    assert_eq!(
        agent_turn_end(&events, "turn-1"),
        Some(AgentTurnEnd::Completed)
    );
    let aborted = [event(
        1,
        SessionEventKind::TurnAborted {
            turn_id: turn("turn-1"),
            reason: Some(" provider disconnected ".into()),
        },
    )];
    let Some(AgentTurnEnd::Aborted(reason)) = agent_turn_end(&aborted, "turn-1") else {
        panic!("aborted");
    };
    assert_eq!(
        agent_aborted_message(reason.as_deref()),
        "O agente interrompeu a sugestão de resolução: provider disconnected"
    );
}

#[test]
fn a_permission_waits_until_resolved_and_only_six_activities_stay() {
    let kind =
        |value: serde_json::Value| -> SessionEventKind { serde_json::from_value(value).unwrap() };
    let mut events: Vec<SessionEventRecord> = (0..7)
        .map(|n| {
            event(
                n,
                kind(serde_json::json!({
                    "type": "turn_tool_call_started", "turnId": "t", "toolCallId": format!("c{n}"),
                    "action": "read", "command": null, "file": format!("src/{n}.rs"),
                })),
            )
        })
        .collect();
    events.push(event(
        8,
        kind(serde_json::json!({
            "type": "turn_permission_requested", "turnId": "t", "requestId": "p1",
            "toolName": "bash", "title": null, "description": null,
        })),
    ));
    let details = agent_run_details(&events, "t");
    assert_eq!(details.activities.len(), 6);
    assert_eq!(details.activities.last(), Some(&AgentActivity::Permission));
    assert!(details.pending_permission);
    events.push(event(
        9,
        kind(serde_json::json!({
            "type": "turn_permission_resolved", "turnId": "t", "requestId": "p1", "behavior": "allow",
        })),
    ));
    assert!(!agent_run_details(&events, "t").pending_permission);
}
