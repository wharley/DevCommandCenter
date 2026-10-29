//! The composer driven headlessly: typing, Enter sending, the draft kept per
//! conversation, and a send that finds no desktop.

use gpui::{TestAppContext, WindowAppearance};

use super::mentions::MenuItem;
use super::{Composer, ComposerSession, continuation_prompt};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::{fake_desktop, fake_desktop_answers};
use crate::theme::{Theme, ThemePreference};

pub(super) fn open(
    cx: &mut TestAppContext,
) -> (gpui::Entity<Composer>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_area::bind_keys(cx);
    });
    cx.add_window_view(Composer::new)
}

pub(super) fn session(id: &str) -> ComposerSession {
    ComposerSession {
        session_id: Some(id.into()),
        provider_id: Some("claude_code".into()),
        model: Some("claude-sonnet-5-5".into()),
        ..ComposerSession::default()
    }
}

#[gpui::test]
fn types_keeps_drafts_and_sends_on_enter(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    let free = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    composer.update_in(cx, |composer, window, cx| {
        composer.endpoint = Some(DesktopApi::new(free, "k"));
        composer.set_session(session("a"), cx);
        composer.focus(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_input("olá");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("mundo");
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.input.read(cx).text(), "olá\nmundo");
        assert!(composer.can_send(cx));
    });
    // Another conversation starts empty; coming back restores the draft.
    composer.update(cx, |composer, cx| composer.set_session(session("b"), cx));
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.input.read(cx).text(), "")
    });
    composer.update(cx, |composer, cx| composer.set_session(session("a"), cx));
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.input.read(cx).text(), "olá\nmundo")
    });
    // Enter sends; with no desktop the prompt stays for another try.
    composer.update_in(cx, |composer, window, cx| composer.focus(window, cx));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.busy, None);
        assert_eq!(composer.input.read(cx).text(), "olá\nmundo");
    });
}

#[gpui::test]
fn enter_while_the_agent_works_queues_the_message(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop("{}".into());
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, port);
        composer.set_session(
            ComposerSession {
                running: true,
                ..session("a")
            },
            cx,
        );
        composer.focus(window, cx);
        assert!(!composer.can_steer(), "Claude here takes no guidance");
    });
    cx.simulate_input("depois rode os testes");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/sessions/a/queue");
    assert!(
        body.contains(r#""prompt":"depois rode os testes""#),
        "{body}"
    );
    assert!(
        !body.contains("providerId"),
        "the session's own provider: {body}"
    );
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.busy, None);
        assert_eq!(composer.input.read(cx).text(), "", "the field empties");
    });
}

#[gpui::test]
fn guidance_that_arrives_late_goes_to_the_queue(cx: &mut TestAppContext) {
    let closed = serde_json::json!({ "ok": false, "error": {
        "code": "steer_window_closed", "message": "provider error: steer_window_closed" } })
    .to_string();
    let (port, server) = fake_desktop_answers(vec![(409, closed), (200, "{}".into())]);
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, port);
        let mut providers = catalog();
        providers[0].capabilities.supports_steering = true;
        composer.catalog = Some(std::sync::Arc::new(providers));
        composer.set_session(
            ComposerSession {
                running: true,
                ..session("a")
            },
            cx,
        );
        composer.focus(window, cx);
        assert!(composer.can_steer());
    });
    cx.simulate_input("use a outra API");
    composer.update(cx, |composer, cx| {
        composer.follow_up(super::queue::FollowUp::Steer, cx)
    });
    cx.run_until_parked();
    let calls = server.join().unwrap();
    assert_eq!(calls[0].0, "POST /api/v1/sessions/a/steer");
    assert_eq!(calls[0].1, r#"{"prompt":"use a outra API"}"#);
    assert_eq!(calls[1].0, "POST /api/v1/sessions/a/queue");
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.input.read(cx).text(), "");
    });
}

#[test]
fn continuation_quotes_the_original_request() {
    assert_eq!(
        continuation_prompt(Some(" faça X "), "Continue.", "Pedido original"),
        "Continue.\n\nPedido original:\nfaça X"
    );
    assert_eq!(continuation_prompt(None, " Continue. ", "x"), "Continue.");
}

#[gpui::test]
fn mentions_pick_files_and_attachments_become_paths(cx: &mut TestAppContext) {
    use std::sync::Arc;
    use std::time::Instant;

    use super::mentions::FileList;
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.files = Some(FileList {
            root: "/repo".into(),
            files: Arc::new(vec![
                "src/app.rs".into(),
                "src/payment.ts".into(),
                "docs/readme.md".into(),
            ]),
            read_at: Instant::now(),
        });
        composer.set_session(
            ComposerSession {
                workspace_path: Some("/repo".into()),
                ..session("a")
            },
            cx,
        );
        composer.focus(window, cx);
    });
    cx.simulate_input("ver @p");
    composer.read_with(cx, |composer, _| {
        let menu = composer.mention.as_ref().expect("the popup opened");
        assert_eq!(
            menu.options,
            vec![
                MenuItem::File("src/payment.ts".into()),
                MenuItem::File("src/app.rs".into())
            ]
        );
    });
    // ↓ then Enter picks the second file; Enter does not send while it's open.
    cx.simulate_keystrokes("down enter");
    composer.read_with(cx, |composer, cx| {
        assert!(composer.mention.is_none());
        assert_eq!(composer.prompt(cx), "ver @src/app.rs");
        assert_eq!(composer.busy, None);
    });
    // A picked file inside the checkout goes relative, outside stays absolute.
    composer.update(cx, |composer, cx| {
        composer.add_paths(
            vec!["/repo/docs/shot.png".into(), "/tmp/notes.txt".into()],
            cx,
        )
    });
    cx.simulate_input("fim");
    composer.read_with(cx, |composer, cx| {
        assert_eq!(
            composer.prompt(cx),
            "ver @src/app.rs @docs/shot.png @/tmp/notes.txtfim"
        );
        let chips = composer.input.read(cx).chips();
        assert!(matches!(chips[1].1, crate::ui::text_area::Chip::Image(_)));
    });
    // Backspace after a chip removes it whole.
    composer.update(cx, |composer, cx| {
        let input = composer.input.clone();
        input.update(cx, |input, cx| input.set_text("", cx));
    });
    cx.run_until_parked();
}

#[gpui::test]
fn long_pastes_become_a_snippet_chip(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.set_session(session("a"), cx);
        composer.focus(window, cx);
    });
    let long = "linha\n".repeat(300);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(long.clone()));
    cx.simulate_input("antes ");
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    composer.read_with(cx, |composer, cx| {
        let chips = composer.input.read(cx).chips();
        assert_eq!(chips.len(), 1);
        assert_eq!(composer.prompt(cx), format!("antes \n{}", long.trim_end()));
    });
}

pub(super) fn catalog() -> Vec<dcc_core::domain::provider::ProviderDescriptor> {
    let mut providers = claude_only();
    let mut codex: dcc_core::domain::provider::ProviderDescriptor =
        serde_json::from_value(serde_json::to_value(&providers[0]).unwrap()).unwrap();
    codex.id = dcc_core::domain::provider::ProviderId("codex".into());
    codex.label = "Codex".into();
    codex.models[0].id = "gpt-6.1-sol".into();
    codex.models[0].label = "GPT-6.1 Sol".into();
    codex.models[0].effort_levels =
        vec!["low".into(), "medium".into(), "high".into(), "xhigh".into()];
    providers.push(codex);
    providers
}

fn claude_only() -> Vec<dcc_core::domain::provider::ProviderDescriptor> {
    let capabilities = serde_json::json!({
        "streaming": true, "mcpSupport": "unsupported", "tools": true, "vision": true,
        "resumable": true, "experimental": false, "canBeDelegationTarget": true,
        "canRequestDelegation": true, "supportsReadOnlyDelegation": true, "supportsEditDelegation": true,
        "approvalPolicies": ["read_only", "ask", "auto", "full_access"], "planModeSupport": "native"
    });
    serde_json::from_value(serde_json::json!([{
        "id": "claude_code", "label": "Claude Code", "description": "",
        "models": [{ "id": "claude-sonnet-5-5", "label": "Claude Sonnet 5.5", "description": "",
                     "recommended": true, "effortLevels": ["low", "medium", "high", "max"] }],
        "capabilities": capabilities, "health": "Healthy", "stable": true
    }]))
    .expect("descriptor")
}

#[gpui::test]
fn turn_settings_follow_the_catalog_and_go_with_the_send(cx: &mut TestAppContext) {
    use std::sync::Arc;

    use dcc_core::domain::provider::ProviderApprovalPolicy;
    let (port, server) = fake_desktop("{}".into());
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.endpoint = Some(DesktopApi::new(port, "k"));
        composer.catalog = Some(Arc::new(catalog()));
        composer.catalog_at = Some(std::time::Instant::now());
        composer.set_session(
            ComposerSession {
                task_id: Some("t".into()),
                ..session("a")
            },
            cx,
        );
        // Auto by default, as the desktop.
        assert_eq!(composer.approval(), Some(ProviderApprovalPolicy::Auto));
        assert_eq!(composer.effort_label_id(), "medium");
        // Full access asks first.
        composer.pick_approval(ProviderApprovalPolicy::FullAccess, cx);
        assert!(composer.confirm_full_access);
        assert_eq!(composer.approval(), Some(ProviderApprovalPolicy::Auto));
        composer.confirm_full_access = false;
        composer.set_approval(ProviderApprovalPolicy::FullAccess, cx);
        composer.toggle_plan(cx);
        assert!(composer.plan_mode());
        composer.focus(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_input("pense nisso, ultrathink");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let (_, body) = server.join().unwrap();
    for part in [
        r#""approvalPolicy":"full_access""#,
        r#""planMode":true"#,
        r#""effort":"max""#,
        r#""prompt":"pense nisso, ultrathink""#,
    ] {
        assert!(body.contains(part), "{part} in {body}");
    }
}

#[gpui::test]
fn the_picker_moves_the_conversation_and_hands_off(cx: &mut TestAppContext) {
    use std::sync::Arc;

    use crate::data::model_picker::FAVORITES_TAB;
    let (port, server) = fake_desktop("{}".into());
    // A conversation with a turn already on Claude.
    let events: Vec<dcc_core::domain::session::SessionEventRecord> = [
        serde_json::json!({ "type": "turn_started", "turnId": "t1", "prompt": "Primeiro pedido" }),
        serde_json::json!({ "type": "turn_delta", "turnId": "t1", "content": "Feito o primeiro." }),
        serde_json::json!({ "type": "turn_completed", "turnId": "t1" }),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, kind)| {
        serde_json::from_value(serde_json::json!({ "eventId": format!("e{index}"), "sessionId": "a", "sequence": index + 1,
            "occurredAt": format!("2026-05-01T12:00:0{index}Z"), "kind": kind }))
        .unwrap()
    })
    .collect();
    let history = Arc::new(dcc_core::domain::timeline::project_session(&events, "a"));
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.endpoint = Some(DesktopApi::new(port, "k"));
        composer.catalog = Some(Arc::new(catalog()));
        composer.catalog_at = Some(std::time::Instant::now());
        composer.set_history(history);
        composer.set_session(
            ComposerSession {
                task_id: Some("t".into()),
                workspace_name: Some("Demo".into()),
                ..session("a")
            },
            cx,
        );
        composer.set_picker_open(true, window, cx);
        assert!(composer.picker.open);
        assert_ne!(
            composer.picker.tab, FAVORITES_TAB,
            "no favorites yet: the provider's tab"
        );
        composer.set_tab("codex".into(), window, cx);
        let rows = composer.picker_rows(cx);
        assert_eq!(rows[0].model_id, "gpt-6.1-sol");
        composer.toggle_favorite(&rows[0], cx);
        assert_eq!(composer.settings.favorites.len(), 1);
        composer.pick_row(rows[0].clone(), window, cx);
        assert!(!composer.picker.open);
        assert_eq!(composer.selected_provider_id().as_deref(), Some("codex"));
        composer.commit_step("xhigh", cx);
        assert_eq!(composer.effort_label_id(), "xhigh");
        composer.focus(window, cx);
    });
    cx.simulate_input("continue daqui");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let (_, body) = server.join().unwrap();
    for part in [
        r#""providerId":"codex""#,
        r#""model":"gpt-6.1-sol""#,
        r#""effort":"xhigh""#,
        "dcc_delegate_task",
        "DCC context handoff",
        "claude_code -> codex",
        "Feito o primeiro.",
    ] {
        assert!(body.contains(part), "{part} in {body}");
    }
}

#[gpui::test]
fn the_model_chip_opens_at_once_and_takes_the_desktop_list(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    // No desktop: the built-in list opens the picker anyway.
    let dead = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    composer.update_in(cx, |composer, window, cx| {
        composer.endpoint = Some(DesktopApi::new(dead, "k"));
        composer.set_session(session("a"), cx);
        composer.set_picker_open(true, window, cx);
        assert!(composer.picker.open);
        assert!(
            !composer.picker_rows(cx).is_empty(),
            "the built-in models show"
        );
        composer.set_picker_open(false, window, cx);
    });
    cx.run_until_parked();
    // The desktop answers: its list replaces the built-in one.
    let (port, server) = fake_desktop(
        serde_json::json!({ "providers": catalog(), "repositories": [], "workspaces": [] })
            .to_string(),
    );
    composer.update_in(cx, |composer, window, cx| {
        composer.endpoint = Some(DesktopApi::new(port, "k"));
        composer.set_picker_open(true, window, cx);
    });
    cx.run_until_parked();
    server.join().unwrap();
    composer.read_with(cx, |composer, _| {
        assert!(composer.picker.open);
        assert_eq!(
            composer.catalog.as_ref().unwrap().len(),
            2,
            "the desktop's list"
        );
    });
}

fn limited() -> dcc_core::domain::usage_limit::SessionUsageLimit {
    dcc_core::domain::usage_limit::SessionUsageLimit {
        turn_id: dcc_core::domain::session::TurnId("t1".into()),
        hit_at: "2026-10-24T10:00:00Z".into(),
        resets_at: None,
        auto_resume: false,
        auto_resumes_used: 0,
    }
}

fn goal(status: dcc_core::domain::goal::GoalStatus) -> dcc_core::domain::goal::SessionGoal {
    dcc_core::domain::goal::SessionGoal {
        objective: "Testes verdes".into(),
        status,
        checks: 2,
        last_check: None,
        tokens_used: Some(12_000),
        token_budget: None,
        time_used_seconds: None,
        completes_with_turn: false,
        updated_at: "2026-10-24T10:00:00Z".into(),
    }
}

#[gpui::test]
fn continuar_resumes_a_task_stopped_by_its_limit(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop("{}".into());
    let (composer, cx) = open(cx);
    composer.update(cx, |composer, cx| {
        composer.endpoint = Some(DesktopApi::new(port, "k"));
        composer.catalog = Some(std::sync::Arc::new(catalog()));
        composer.catalog_at = Some(std::time::Instant::now());
        composer.set_session(
            ComposerSession {
                usage_limit: Some(limited()),
                ..session("a")
            },
            cx,
        );
        composer.resume_after_limit(cx);
        assert_eq!(
            composer.strip_busy,
            Some(super::strips::StripAction::Resume)
        );
        // A second click while it's on its way does nothing.
        composer.resume_after_limit(cx);
    });
    cx.run_until_parked();
    let (request, _) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/sessions/a/usage-limit/resume");
    composer.read_with(cx, |composer, _| assert_eq!(composer.strip_busy, None));
}

#[gpui::test]
fn goal_buttons_send_the_providers_command_or_dismiss(cx: &mut TestAppContext) {
    use dcc_core::domain::goal::GoalStatus;
    let (port, server) = fake_desktop("{}".into());
    let (composer, cx) = open(cx);
    composer.update(cx, |composer, cx| {
        composer.endpoint = Some(DesktopApi::new(port, "k"));
        composer.catalog = Some(std::sync::Arc::new(catalog()));
        composer.catalog_at = Some(std::time::Instant::now());
        composer.set_session(
            ComposerSession {
                provider_id: Some("codex".into()),
                goal: Some(goal(GoalStatus::Paused)),
                ..session("a")
            },
            cx,
        );
        composer.goal_action(super::strips::StripAction::GoalResume, cx);
    });
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/sessions/a/turns");
    assert!(body.contains(r#""prompt":"/goal resume""#), "{body}");
    assert!(!body.contains(r#""planMode":true"#), "{body}");
    // A finished goal closes through the desktop, no turn.
    let (port, server) = fake_desktop("{\"ok\":true}".into());
    composer.update(cx, |composer, cx| {
        composer.endpoint = Some(DesktopApi::new(port, "k"));
        composer.catalog = Some(std::sync::Arc::new(catalog()));
        composer.catalog_at = Some(std::time::Instant::now());
        composer.set_session(
            ComposerSession {
                goal: Some(goal(GoalStatus::Complete)),
                ..session("a")
            },
            cx,
        );
        composer.goal_action(super::strips::StripAction::GoalDismiss, cx);
    });
    cx.run_until_parked();
    let (request, _) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/sessions/a/goal/dismiss");
}

#[gpui::test]
fn the_auto_resume_toggle_saves_and_goes_back_on_failure(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop("{}".into());
    let (composer, cx) = open(cx);
    composer.update(cx, |composer, cx| {
        composer.endpoint = Some(DesktopApi::new(port, "k"));
        composer.catalog = Some(std::sync::Arc::new(catalog()));
        composer.catalog_at = Some(std::time::Instant::now());
        composer.set_session(session("a"), cx);
        composer.toggle_auto_resume(cx);
        assert!(composer.session.auto_resume, "flips at once");
    });
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "PUT /api/v1/sessions/a/usage-limit/auto-resume");
    assert_eq!(body, r#"{"enabled":true}"#);
    composer.read_with(cx, |composer, _| assert!(composer.session.auto_resume));
    // With the desktop closed the switch goes back.
    let dead = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    composer.update(cx, |composer, cx| {
        composer.endpoint = Some(DesktopApi::new(dead, "k"));
        composer.toggle_auto_resume(cx);
        assert!(!composer.session.auto_resume);
    });
    cx.run_until_parked();
    composer.read_with(cx, |composer, _| {
        assert!(composer.session.auto_resume, "back to on");
        assert_eq!(composer.strip_busy, None);
    });
}

fn objective(generation: u64) -> dcc_core::domain::objective::SessionObjective {
    let draft = dcc_core::domain::objective::SessionObjectiveDraft {
        intent: "Checkout verde".into(),
        done_when: "testes passam".into(),
        max_consecutive_failures: None,
        max_turns: Some(5),
    };
    let mut objective = dcc_core::domain::objective::SessionObjective::new(
        dcc_core::domain::session::SessionId("a".into()),
        draft.validate().unwrap(),
        "2026-10-24T10:00:00Z",
    );
    objective.generation = generation;
    objective
}

pub(super) fn with_catalog(composer: &mut Composer, port: u16) {
    composer.endpoint = Some(DesktopApi::new(port, "k"));
    composer.catalog = Some(std::sync::Arc::new(catalog()));
    composer.catalog_at = Some(std::time::Instant::now());
}

#[gpui::test]
fn the_objective_panel_creates_one_from_the_form(cx: &mut TestAppContext) {
    let created = serde_json::json!({ "objective": objective(1) }).to_string();
    let (port, server) = fake_desktop(created);
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, port);
        composer.set_session(session("a"), cx);
        composer.set_objective_open(true, window, cx);
        assert!(composer.objective.open);
        assert_eq!(composer.objective.form(cx).max_consecutive_failures, "3");
        assert!(!composer.objective.dirty(cx));
    });
    // The intent field has the focus; Enter is a new line, not a send.
    cx.simulate_input("Checkout");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("verde");
    composer.update(cx, |composer, cx| {
        assert!(composer.objective.dirty(cx));
        assert_eq!(composer.objective.form(cx).intent, "Checkout\nverde");
        assert_eq!(composer.busy, None, "no turn went out");
        composer.save_objective(cx);
    });
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "PUT /api/v1/sessions/a/objective");
    assert!(body.contains(r#""intent":"Checkout\nverde""#), "{body}");
    assert!(body.contains(r#""maxConsecutiveFailures":3"#), "{body}");
    assert!(body.contains(r#""expectedGeneration":null"#), "{body}");
    composer.read_with(cx, |composer, cx| {
        assert_eq!(
            composer.session.objective.as_ref().map(|o| o.generation),
            Some(1)
        );
        assert_eq!(composer.objective.form(cx).intent, "Checkout verde");
        assert!(!composer.objective.dirty(cx));
        assert_eq!(composer.objective.busy, None);
    });
    // The open panel lays out with its progress and the transitions.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds("objective-panel")
        .expect("the panel painted");
    assert_eq!(f32::from(bounds.size.width), 520.0);
}

#[gpui::test]
fn the_objective_form_refuses_a_bad_draft_before_the_desktop(cx: &mut TestAppContext) {
    let dead = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, dead);
        composer.set_session(session("a"), cx);
        composer.set_objective_open(true, window, cx);
        composer
            .objective
            .max_failures
            .update(cx, |field, cx| field.set_text("50", cx));
        composer.save_objective(cx);
        // Refused here: nothing on its way.
        assert_eq!(composer.objective.busy, None);
    });
}

#[gpui::test]
fn objective_transitions_and_clear_go_with_the_generation(cx: &mut TestAppContext) {
    use dcc_core::domain::objective::{ObjectiveStatus, ObjectiveTransition};
    let mut paused = objective(3);
    paused.transition(ObjectiveTransition::Pause, "2026-10-24T10:01:00Z");
    let (port, server) = fake_desktop(serde_json::json!({ "objective": paused }).to_string());
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, port);
        composer.set_session(
            ComposerSession {
                objective: Some(objective(2)),
                ..session("a")
            },
            cx,
        );
        composer.set_objective_open(true, window, cx);
        composer.transition_objective(ObjectiveTransition::Pause, cx);
        // Closing waits for the answer.
        composer.set_objective_open(false, window, cx);
        assert!(composer.objective.open);
    });
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/sessions/a/objective/transition");
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({ "transition": "pause", "expectedGeneration": 2 })
    );
    composer.read_with(cx, |composer, _| {
        assert_eq!(
            composer.session.objective.as_ref().map(|o| o.status),
            Some(ObjectiveStatus::Paused)
        );
    });
    let (port, server) = fake_desktop(r#"{"objective":null}"#.into());
    composer.update(cx, |composer, cx| {
        composer.endpoint = Some(DesktopApi::new(port, "k"));
        composer.clear_objective(cx);
    });
    cx.run_until_parked();
    let (request, _) = server.join().unwrap();
    assert_eq!(request, "DELETE /api/v1/sessions/a/objective");
    composer.read_with(cx, |composer, cx| {
        assert!(composer.session.objective.is_none());
        assert!(!composer.objective.open, "clearing closes the panel");
        assert_eq!(composer.objective.form(cx).intent, "");
    });
}

#[gpui::test]
fn a_refresh_keeps_what_the_person_is_typing(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    composer.update(cx, |composer, cx| {
        with_catalog(composer, 1);
        composer.set_session(
            ComposerSession {
                objective: Some(objective(2)),
                ..session("a")
            },
            cx,
        );
        assert_eq!(composer.objective.form(cx).intent, "Checkout verde");
        composer
            .objective
            .intent
            .update(cx, |field, cx| field.set_text("Checkout azul", cx));
        assert!(composer.objective.dirty(cx));
        // The turn ended and the counters moved: the typed text stays.
        let mut counted = objective(3);
        counted.turns_used = 1;
        composer.set_session(
            ComposerSession {
                objective: Some(counted),
                ..session("a")
            },
            cx,
        );
        assert_eq!(composer.objective.form(cx).intent, "Checkout azul");
        // Another conversation starts from its own record.
        composer.set_session(session("b"), cx);
        assert_eq!(composer.objective.form(cx).intent, "");
        assert!(!composer.objective.dirty(cx));
    });
}

fn queued(id: &str, prompt: &str) -> dcc_core::domain::session::QueuedTurn {
    serde_json::from_value(serde_json::json!({
        "id": id, "sessionId": "a", "prompt": prompt, "planMode": false,
        "fastMode": false, "createdAt": "2026-10-24T10:00:00Z"
    }))
    .unwrap()
}

#[gpui::test]
fn the_queue_tray_moves_edits_and_sends_the_next(cx: &mut TestAppContext) {
    let queue = std::sync::Arc::new(vec![queued("q1", "primeiro"), queued("q2", "segundo")]);
    let reordered =
        serde_json::json!({ "queue": [queued("q2", "segundo"), queued("q1", "primeiro")] });
    let after_edit = serde_json::json!({ "queue": [queued("q1", "primeiro")] });
    let (port, server) = fake_desktop_answers(vec![
        (200, reordered.to_string()),
        (200, after_edit.to_string()),
        (200, r#"{"dispatched":true}"#.into()),
    ]);
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, port);
        composer.set_session(
            ComposerSession {
                queue: queue.clone(),
                ..session("a")
            },
            cx,
        );
        composer.move_queued(0, true, window, cx);
        // One action at a time.
        composer.dispatch_next(window, cx);
    });
    cx.run_until_parked();
    composer.update_in(cx, |composer, window, cx| {
        assert_eq!(composer.session.queue[0].id, "q2");
        composer.edit_queued("q2".into(), window, cx);
    });
    cx.run_until_parked();
    composer.update_in(cx, |composer, window, cx| {
        assert_eq!(
            composer.input.read(cx).text(),
            "segundo",
            "back in the field"
        );
        assert_eq!(composer.session.queue.len(), 1);
        composer.dispatch_next(window, cx);
    });
    cx.run_until_parked();
    let calls = server.join().unwrap();
    assert_eq!(calls[0].0, "PUT /api/v1/sessions/a/queue/order");
    assert_eq!(calls[0].1, r#"{"queuedTurnIds":["q2","q1"]}"#);
    assert_eq!(calls[1].0, "DELETE /api/v1/sessions/a/queue/q2");
    assert_eq!(calls[2].0, "POST /api/v1/sessions/a/queue/dispatch-next");
    // The tray lays out over the prompt.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("queue-tray").is_some());
}

fn new_task(auto_named: bool) -> ComposerSession {
    ComposerSession {
        task_id: Some("ws-1".into()),
        project_id: Some("checkout".into()),
        start: Some(crate::data::conversation::ThreadStart {
            workspace_id: "ws-1".into(),
            project_id: "checkout".into(),
            additional_workspace_ids: vec!["ws-2".into()],
            name: (!auto_named).then(|| "Pagamentos".to_string()),
            auto_named,
        }),
        ..ComposerSession::default()
    }
}

#[gpui::test]
fn the_first_prompt_starts_the_conversation_and_names_the_task(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop_answers(vec![
        (200, r#"{"session":{"id":"s-new"}}"#.into()),
        (200, "{}".into()),
        (200, r#"{"applied":true}"#.into()),
    ]);
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, port);
        composer.set_session(new_task(true), cx);
        assert!(composer.session.usable());
        assert_eq!(
            composer.selected_provider_id().as_deref(),
            Some("claude_code")
        );
        composer.focus(window, cx);
    });
    cx.simulate_input("Preciso que você corrija o login do checkout");
    composer.read_with(cx, |composer, cx| assert!(composer.can_send(cx)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let calls = server.join().unwrap();
    assert_eq!(calls[0].0, "POST /api/v1/sessions/start");
    let start: serde_json::Value = serde_json::from_str(&calls[0].1).unwrap();
    assert_eq!(start["workspaceId"], "ws-1");
    assert_eq!(start["additionalWorkspaceIds"], serde_json::json!(["ws-2"]));
    assert_eq!(start["projectId"], "checkout");
    assert_eq!(start["providerId"], "claude_code");
    assert_eq!(start["model"], "claude-sonnet-5-5");
    assert_eq!(start["title"], "Corrija o login do checkout");
    assert_eq!(calls[1].0, "POST /api/v1/sessions/s-new/turns");
    assert!(
        calls[1]
            .1
            .contains(r#""prompt":"Preciso que você corrija o login do checkout""#)
    );
    assert!(!calls[1].1.contains("providerId"), "{}", calls[1].1);
    assert_eq!(calls[2].0, "POST /api/v1/sessions/s-new/task-title");
    assert_eq!(
        calls[2].1,
        r#"{"workspaceId":"ws-1","title":"Corrija o login do checkout"}"#
    );
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.input.read(cx).text(), "");
        assert_eq!(composer.busy, None);
    });
}

#[gpui::test]
fn a_named_task_keeps_its_name(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop_answers(vec![
        (200, r#"{"session":{"id":"s-new"}}"#.into()),
        (200, "{}".into()),
    ]);
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, port);
        composer.set_session(new_task(false), cx);
        // Picking a model makes it this project's and the global default.
        composer.select_model("codex".into(), Some("gpt-6.1-sol".into()), cx);
        assert_eq!(composer.selected_provider_id().as_deref(), Some("codex"));
        let pick = composer.settings.project_pick("checkout").cloned().unwrap();
        assert_eq!(
            (pick.provider_id.as_str(), pick.model_id.as_str()),
            ("codex", "gpt-6.1-sol")
        );
        assert_eq!(composer.settings.global_pick, Some(pick));
        composer.focus(window, cx);
    });
    cx.simulate_input("rode os testes");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let calls = server.join().unwrap();
    assert_eq!(calls.len(), 2);
    let start: serde_json::Value = serde_json::from_str(&calls[0].1).unwrap();
    assert_eq!(start["providerId"], "codex");
    assert_eq!(start["title"], "Pagamentos");
}

#[gpui::test]
fn the_setup_warning_opens_the_terminal(cx: &mut TestAppContext) {
    use dcc_core::domain::workspace::{WorkspaceSetupReport, WorkspaceSetupStatus};
    let (composer, cx) = open(cx);
    let opened = std::rc::Rc::new(std::cell::Cell::new(false));
    let sink = opened.clone();
    composer.update(cx, |composer, cx| {
        composer.set_session(
            ComposerSession {
                context: Some(crate::data::conversation::ExecutionContext {
                    project_label: Some("Pagamentos".into()),
                    project_icon: None,
                    project_color: None,
                    project_seed: None,
                    base_branch: Some("main".into()),
                    current_branch: None,
                    isolated: true,
                    setup_report: Some(WorkspaceSetupReport {
                        status: WorkspaceSetupStatus::Failed,
                        steps: vec![],
                        message: Some("yarn install failed".into()),
                    }),
                }),
                ..session("a")
            },
            cx,
        );
        cx.subscribe(
            &cx.entity(),
            move |_, _, event: &super::ComposerEvent, _| {
                if matches!(event, super::ComposerEvent::OpenTerminal) {
                    sink.set(true);
                }
            },
        )
        .detach();
    });
    cx.run_until_parked();
    let button = cx
        .debug_bounds("setup-open-terminal")
        .expect("the warning's button");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(opened.get());
}

#[gpui::test]
fn a_running_turn_keeps_its_model_approval_and_plan(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.set_session(
            ComposerSession {
                running: true,
                ..session("a")
            },
            cx,
        );
        assert!(composer.turn_settings_disabled());
        composer.set_picker_open(true, window, cx);
        assert!(!composer.picker.open, "the picker waits for the turn");
        let plan = composer.plan_mode();
        composer.toggle_plan(cx);
        assert_eq!(composer.plan_mode(), plan, "Planejar waits too");
        composer.set_session(session("a"), cx);
        assert!(!composer.turn_settings_disabled());
    });
}
