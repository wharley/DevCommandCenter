//! A task proposal's card driven headlessly: where it will run, the brief
//! edited and "Começar" sending it; once started, a task opened from an
//! agent's conversation opens in its own project.

use std::rc::Rc;
use std::sync::Arc;

use dcc_core::domain::agent_conversation::NewTaskTarget;
use dcc_core::domain::delegation::DelegationStatus;
use dcc_core::domain::session::SessionEventRecord;
use dcc_core::domain::timeline::project_session;
use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowAppearance};
use serde_json::{Value, json};

use super::ConversationView;
use crate::data::conversation::Conversation;
use crate::data::delegation_record::DelegationRecord;
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

fn records(kinds: Vec<Value>) -> Vec<SessionEventRecord> {
    kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            serde_json::from_value(json!({
                "eventId": format!("evt-{index}"),
                "sessionId": "home",
                "sequence": index + 1,
                "occurredAt": format!("2026-11-06T12:00:{:02}Z", index),
                "kind": kind,
            }))
            .unwrap()
        })
        .collect()
}

fn conversation(record: DelegationRecord) -> Conversation {
    let events = records(vec![
        json!({ "type": "turn_started", "turnId": "t1", "prompt": "abre uma tarefa no Checkout" }),
        json!({ "type": "delegation_requested", "delegationId": "d1" }),
        json!({ "type": "turn_completed", "turnId": "t1" }),
    ]);
    Conversation {
        task_id: "home-task".into(),
        session_id: Some("home".into()),
        provider_id: Some("claude_code".into()),
        rows: Arc::new(project_session(&events, "home")),
        last_sequence: events.len() as u64,
        delegations: [("d1".to_string(), record)].into_iter().collect(),
        ..Conversation::default()
    }
}

fn proposal() -> DelegationRecord {
    DelegationRecord {
        id: "d1".into(),
        status: DelegationStatus::Draft,
        is_task: true,
        implement: true,
        shared_checkout: true,
        title: Some("Código de rastreio".into()),
        instruction: "Mostrar o código de rastreio no checkout.".into(),
        provider_id: "claude_code".into(),
        parent_session_id: "home".into(),
        new_task: Some(NewTaskTarget {
            project_id: "p-loja".into(),
            project: "Checkout".into(),
            root_path: "/loja".into(),
            base_branch: "main".into(),
            worktree: true,
            effort: Some("high".into()),
            workspace_id: None,
        }),
        ..DelegationRecord::default()
    }
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    cx.run_until_parked();
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} on screen"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn a_proposal_is_started_with_the_brief_as_edited_and_opens_in_its_project(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (port, server) = fake_desktop_routes(vec![(
        "/api/v1/task-proposals/d1/start",
        200,
        r#"{"taskId":"d1","workspaceId":"w-loja","childSessionId":"s-loja"}"#.into(),
    )]);
    let (view, cx) = cx.add_window_view(ConversationView::new);
    view.update(cx, |view, cx| {
        view.composer.update(cx, |composer, _| {
            composer.endpoint = Some(DesktopApi::new(port, "k"));
        });
        view.task_id = Some("home-task".into());
        view.loading = false;
        view.apply(Rc::new(conversation(proposal())), cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("proposal-card").is_some());
    let meta = cx.update(|_, cx| super::proposal_card::meta_line(&proposal(), cx));
    assert_eq!(
        meta,
        "Checkout · worktree próprio · Claude Code · esforço alto"
    );

    click(cx, "proposal-edit-d1");
    assert!(cx.debug_bounds("proposal-brief-input").is_some());
    assert!(
        cx.debug_bounds("proposal-edit-d1").is_none(),
        "already editing"
    );
    view.update(cx, |view, cx| {
        let area = view.proposals.editing.get("d1").cloned().expect("editing");
        area.update(cx, |area, cx| area.set_text("Só o código, sem e-mail.", cx));
    });
    click(cx, "proposal-start-d1");
    let requests = server.join().unwrap();
    assert_eq!(requests[0].0, "POST /api/v1/task-proposals/d1/start");
    let body: Value = serde_json::from_str(&requests[0].1).unwrap();
    assert_eq!(body["brief"], "Só o código, sem e-mail.");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.proposals.editing.is_empty()));

    // Started: the card is the task's, and it opens in the project.
    let mut started = proposal();
    started.status = DelegationStatus::Running;
    started.child_session_id = Some("s-loja".into());
    if let Some(target) = started.new_task.as_mut() {
        target.workspace_id = Some("w-loja".into());
    }
    view.update(cx, |view, cx| {
        view.apply(Rc::new(conversation(started)), cx)
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("proposal-card").is_none());
    let open = view.read_with(cx, |view, _| {
        view.data
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|row| -> &'static str {
                Box::leak(format!("{}-open-child", row.id).into_boxed_str())
            })
            .collect::<Vec<_>>()
    });
    let open = open
        .into_iter()
        .find(|selector| cx.debug_bounds(selector).is_some())
        .expect("the open button");
    click(cx, open);
    assert_eq!(
        view.read_with(cx, |view, _| view.task_id.clone()),
        Some("w-loja".into())
    );
}
