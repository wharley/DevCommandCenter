//! The composer driven headlessly: typing, Enter sending, the draft kept per
//! conversation, and a send that finds no desktop.

use gpui::{TestAppContext, WindowAppearance};

use super::{Composer, ComposerSession, continuation_prompt};
use crate::data::desktop_api::DesktopApi;
use crate::theme::{Theme, ThemePreference};

fn open(cx: &mut TestAppContext) -> (gpui::Entity<Composer>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_area::bind_keys(cx);
    });
    cx.add_window_view(Composer::new)
}

fn session(id: &str) -> ComposerSession {
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
fn running_turns_show_stop_and_keep_enter_from_sending(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.set_session(
            ComposerSession {
                running: true,
                ..session("a")
            },
            cx,
        );
        composer.focus(window, cx);
    });
    cx.simulate_input("depois");
    cx.simulate_keystrokes("enter");
    composer.read_with(cx, |composer, cx| {
        assert!(!composer.can_send(cx));
        assert_eq!(composer.busy, None);
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
            vec!["src/payment.ts".to_string(), "src/app.rs".to_string()]
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

fn catalog() -> Vec<dcc_core::domain::provider::ProviderDescriptor> {
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
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::Arc;

    use dcc_core::domain::provider::ProviderApprovalPolicy;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
            if line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let mut stream = stream;
        write!(stream, "HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{{}}").unwrap();
        String::from_utf8(body).unwrap()
    });
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
    let body = server.join().unwrap();
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
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::Arc;

    use crate::data::model_picker::FAVORITES_TAB;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
            if line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let mut stream = stream;
        write!(stream, "HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{{}}").unwrap();
        String::from_utf8(body).unwrap()
    });
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
    let body = server.join().unwrap();
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
