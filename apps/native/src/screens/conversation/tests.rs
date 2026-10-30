//! Draws the conversation headlessly from a projected history: settled and
//! live turns, tool details, a delegation card and the folds opened.

use std::rc::Rc;
use std::sync::Arc;

use dcc_core::domain::session::SessionEventRecord;
use dcc_core::domain::timeline::project_session;
use gpui::{TestAppContext, WindowAppearance};
use serde_json::{Value, json};

use super::{ConversationView, changed_span};
use crate::data::conversation::Conversation;
use crate::theme::{Theme, ThemePreference};

fn records(kinds: Vec<Value>) -> Vec<SessionEventRecord> {
    kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            serde_json::from_value(json!({
                "eventId": format!("evt-{index}"),
                "sessionId": "s1",
                "sequence": index + 1,
                "occurredAt": format!("2026-05-01T12:00:{:02}Z", index),
                "kind": kind,
            }))
            .expect("record")
        })
        .collect()
}

fn history(live: bool) -> Conversation {
    let mut kinds = vec![
        json!({ "type": "turn_started", "turnId": "t1", "prompt": "Veja @/Users/me/tela.png e @src/app.rs" }),
        json!({ "type": "turn_reasoning_started", "turnId": "t1", "reasoningId": "r1", "label": null }),
        json!({ "type": "turn_reasoning_delta", "turnId": "t1", "reasoningId": "r1", "content": "Lendo." }),
        json!({ "type": "turn_reasoning_completed", "turnId": "t1", "reasoningId": "r1" }),
        json!({ "type": "turn_tool_call_started", "turnId": "t1", "toolCallId": "c1", "action": "Bash", "command": "cargo test", "file": null }),
        json!({ "type": "turn_tool_call_completed", "turnId": "t1", "toolCallId": "c1", "detail": { "output": "ok", "exitCode": 0 } }),
        json!({ "type": "turn_tool_call_started", "turnId": "t1", "toolCallId": "c2", "action": "Edit", "command": null, "file": "src/app.rs" }),
        json!({ "type": "turn_tool_call_completed", "turnId": "t1", "toolCallId": "c2", "detail": { "diff": "@@ -1 +1 @@\n-a\n+b\n" } }),
        json!({ "type": "turn_tool_call_started", "turnId": "t1", "toolCallId": "c3", "action": "TodoWrite", "command": null, "file": null }),
        json!({ "type": "turn_tool_call_completed", "turnId": "t1", "toolCallId": "c3", "detail": { "input": "{\"todos\":[{\"content\":\"A\",\"status\":\"completed\"},{\"content\":\"B\",\"status\":\"pending\"}]}" } }),
        json!({ "type": "turn_assistant_message_started", "turnId": "t1", "messageId": "m1", "phase": "final_answer" }),
        json!({ "type": "turn_assistant_message_delta", "turnId": "t1", "messageId": "m1", "content": "# Feito\n\n- um\n- dois\n\n```rust\nfn main() {}\n```" }),
        json!({ "type": "turn_assistant_message_completed", "turnId": "t1", "messageId": "m1", "phase": "final_answer", "content": null }),
        json!({ "type": "turn_completed", "turnId": "t1" }),
        json!({ "type": "delegation_requested", "delegationId": "d1" }),
        json!({ "type": "delegation_started", "delegationId": "d1", "childSessionId": "child" }),
        json!({ "type": "turn_started", "turnId": "t2", "prompt": "Agora rode" }),
        json!({ "type": "turn_tool_call_started", "turnId": "t2", "toolCallId": "c4", "action": "Bash", "command": "yarn test", "file": null }),
    ];
    if live {
        kinds.push(json!({ "type": "turn_assistant_message_started", "turnId": "t2", "messageId": "m2", "phase": "commentary" }));
        kinds.push(json!({ "type": "turn_assistant_message_delta", "turnId": "t2", "messageId": "m2", "content": "Rodando os testes agora, já volto com o resultado" }));
        kinds.push(json!({ "type": "turn_permission_requested", "turnId": "t2", "requestId": "p1", "toolName": "Bash", "title": null, "description": null, "command": "rm -rf dist", "file": null }));
        kinds.push(json!({ "type": "turn_user_input_requested", "turnId": "t2", "requestId": "q1", "questions": [{ "id": "a", "header": "Destino", "question": "Qual ambiente?", "options": [{ "label": "staging", "description": "" }, { "label": "prod", "description": "" }] }] }));
    } else {
        kinds.push(json!({ "type": "turn_aborted", "turnId": "t2", "reason": "parou" }));
    }
    let events = records(kinds);
    Conversation {
        task_id: "task".into(),
        session_id: Some("s1".into()),
        provider_id: Some("codex".into()),
        model: Some("gpt-5.1-codex".into()),
        running: live,
        rows: Arc::new(project_session(&events, "s1")),
        last_sequence: events.len() as u64,
        ..Conversation::default()
    }
}

fn open_view(
    cx: &mut TestAppContext,
) -> (gpui::Entity<ConversationView>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    cx.add_window_view(ConversationView::new)
}

#[gpui::test]
fn draws_settled_and_live_turns_with_their_details_open(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    for live in [false, true] {
        view.update(cx, |view, cx| {
            view.task_id = Some("task".into());
            view.loading = false;
            view.apply(Rc::new(history(live)), cx);
        });
        cx.run_until_parked();
        let keys: Vec<String> = view.read_with(cx, |view, _| {
            view.data
                .as_ref()
                .expect("data")
                .rows
                .iter()
                .flat_map(|row| {
                    let id = row.id.clone();
                    [
                        format!("{id}-fold"),
                        format!("{id}-s0-run"),
                        format!("{id}-s0-tool-c1"),
                        format!("{id}-s0-tool-c2"),
                        format!("{id}-s0-tool-c3"),
                        format!("{id}-s0-thought-r1"),
                        format!("report-{id}"),
                    ]
                })
                .collect()
        });
        view.update(cx, |view, cx| {
            for key in keys {
                view.toggle(key.into(), cx);
            }
        });
        cx.run_until_parked();
    }
    view.read_with(cx, |view, _| {
        let data = view.data.as_ref().expect("data");
        assert!(data.rows.len() >= 4, "{} rows", data.rows.len());
        assert!(view.list.item_count() == data.rows.len() + 2);
        // The rows were drawn: their markdown went through the cache.
        assert!(view.markdown.borrow().len() >= 2);
        // The live narration went through the paced reveal.
        assert!(!view.paced.borrow().is_empty());
    });
}

#[test]
fn a_new_reply_replaces_only_the_rows_that_changed() {
    let settled = history(false).rows;
    let live = history(true).rows;
    let (span, count) = changed_span(&live, &settled);
    assert!(span.start >= 1, "the first prompt stays measured");
    assert_eq!(span.end, live.len());
    assert_eq!(count, settled.len() - span.start);
}

#[gpui::test]
fn answers_wait_for_the_desktop_and_options_fill_the_field(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    // A local port nobody listens on, never the real desktop.
    let free = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    view.update(cx, |view, cx| {
        view.task_id = Some("task".into());
        view.loading = false;
        view.requests.endpoint = Some(crate::data::desktop_api::DesktopApi::new(free, "k"));
        view.apply(Rc::new(history(true)), cx);
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_eq!(view.answer_text("q1", "Qual ambiente?", cx), "");
        view.pick_option("q1", "Qual ambiente?", "staging".into(), cx);
        assert_eq!(view.answer_text("q1", "Qual ambiente?", cx), "staging");
        // While the answer travels the buttons hold still.
        view.respond_permission("p1".into(), true, cx);
        assert_eq!(view.sending("p1"), Some(super::requests::Sending::Allow));
    });
    cx.run_until_parked();
    // The desktop isn't there in tests: the request comes back to waiting.
    view.read_with(cx, |view, _| assert_eq!(view.sending("p1"), None));
}

/// An attached audio file plays in place; one that is gone stays a chip.
#[gpui::test]
fn an_attached_voice_note_shows_as_a_player(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let wav = crate::data::audio_clip::tests::wav(dir.path());
    let (view, cx) = open_view(cx);
    let show = |turn: &str, prompt: String, cx: &mut gpui::VisualTestContext| {
        let events = records(vec![
            json!({ "type": "turn_started", "turnId": turn, "prompt": prompt }),
            json!({ "type": "turn_completed", "turnId": turn }),
        ]);
        let conversation = Conversation {
            task_id: "task".into(),
            session_id: Some("s1".into()),
            rows: Arc::new(project_session(&events, "s1")),
            // A later load reads as news.
            last_sequence: if turn == "t1" { 2 } else { 4 },
            ..Conversation::default()
        };
        view.update(cx, |view, cx| {
            view.task_id = Some("task".into());
            view.loading = false;
            view.apply(Rc::new(conversation), cx);
        });
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    };
    show("t1", format!("Ouça @{} por favor", wav.display()), cx);
    let card = cx.debug_bounds("user-message-audio").expect("the player");
    assert!(card.size.width > gpui::px(200.0), "{card:?}");
    let gone = dir.path().join("sumiu.opus");
    show("t2", format!("Ouça @{} por favor", gone.display()), cx);
    assert!(cx.debug_bounds("user-message-audio").is_none());
}
