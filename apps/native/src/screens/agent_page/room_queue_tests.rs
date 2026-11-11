//! Writing while the agent works, headlessly: a message to a conversation
//! with a turn at work is queued on the desktop, not sent; its bubble says
//! it waits for the agent, and "Tirar da fila" takes it out.

use gpui::{Modifiers, TestAppContext};

use super::room_list_tests::room_with_lines;
use crate::data::agents::AgentState;
use crate::test_support::fake_desktop_routes;

/// Codex, as the desktop's catalog lists it.
pub(super) fn codex_catalog() -> std::sync::Arc<Vec<dcc_core::domain::provider::ProviderDescriptor>>
{
    std::sync::Arc::new(
        serde_json::from_value(serde_json::json!([{
            "id": "codex", "label": "Codex", "description": "",
            "models": [{ "id": "gpt-5", "label": "GPT-5",
                         "description": "", "recommended": true, "effortLevels": [] }],
            "capabilities": {
                "streaming": true, "mcpSupport": "unsupported", "tools": true,
                "vision": true, "resumable": true, "experimental": false,
                "canBeDelegationTarget": true, "canRequestDelegation": true,
                "supportsReadOnlyDelegation": true, "supportsEditDelegation": true,
                "approvalPolicies": ["read_only", "ask", "auto", "full_access"],
                "planModeSupport": "native"
            },
            "health": "Healthy", "stable": true
        }]))
        .unwrap(),
    )
}

#[gpui::test]
fn a_message_to_an_agent_at_work_is_queued(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_lines(cx, Vec::new(), AgentState::Working);
    let (port, desktop) = fake_desktop_routes(vec![
        (
            "/api/v1/sessions/s-geral/queue",
            200,
            r#"{"queuedTurn":{"id":"q-1","sessionId":"s-geral","prompt":"e o cache?","createdAt":"2026-11-11T10:00:00Z"}}"#.into(),
        ),
        ("/api/v1/sessions/s-geral/queue/q-1", 200, r#"{"queue":[]}"#.into()),
    ]);
    room.update(cx, |room, cx| {
        room.composer
            .update(cx, |composer, _| composer.catalog = Some(codex_catalog()));
        room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        room.input
            .update(cx, |area, cx| area.set_text("e o cache?", cx));
        room.send(cx);
        assert_eq!(room.pending.len(), 1);
        assert!(room.pending[0].queued.is_some(), "marked at once");
        assert_eq!(room.input.read(cx).text(), "");
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-pending-0").is_some());
    room.update(cx, |room, cx| {
        let sent = room.pending[0].clone();
        assert_eq!(
            sent.queued.as_ref().unwrap().id.as_deref(),
            Some("q-1"),
            "the desktop's id"
        );
        assert_eq!(
            room.pending_note(&sent, "Dev", cx),
            "Na fila · sai quando o Dev terminar"
        );
    });

    // "Tirar da fila": the desktop drops it, and so does the room.
    let unqueue = cx.debug_bounds("room-unqueue-0").unwrap();
    cx.simulate_click(unqueue.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(room.update(cx, |room, _| room.pending.is_empty()));
    assert!(cx.debug_bounds("room-pending-0").is_none());
    assert!(
        crate::test_support::desktop_done(&desktop, || cx.run_until_parked()),
        "the desktop never got the call"
    );
    let seen = desktop.join().unwrap();
    assert_eq!(seen[0].0, "POST /api/v1/sessions/s-geral/queue");
    let body: serde_json::Value = serde_json::from_str(&seen[0].1).unwrap();
    assert_eq!(body["prompt"], "e o cache?");
    assert_eq!(seen[1].0, "DELETE /api/v1/sessions/s-geral/queue/q-1");
}
