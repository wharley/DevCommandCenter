//! "Roda em" driven headlessly, the way it failed before: open it, go to
//! Codex, pick a model and "Alto" — the agent keeps all three.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::editor::AgentEditor;
use crate::theme::{Theme, ThemePreference};

fn catalog() -> Vec<dcc_core::domain::provider::ProviderDescriptor> {
    let provider = |id: &str, label: &str, model: &str, model_label: &str| {
        let accounts = id == "claude_code";
        serde_json::json!({
            "id": id, "label": label, "description": "",
            "models": [{ "id": model, "label": model_label, "description": "",
                         "recommended": true, "effortLevels": ["low", "medium", "high"] }],
            "capabilities": {
                "streaming": true, "mcpSupport": "unsupported", "tools": true,
                "vision": true, "resumable": true, "experimental": false,
                "canBeDelegationTarget": true, "canRequestDelegation": true,
                "supportsReadOnlyDelegation": true, "supportsEditDelegation": true,
                "approvalPolicies": ["read_only", "ask", "auto", "full_access"],
                "planModeSupport": "native",
                "supportsAccounts": accounts
            },
            "health": "Healthy", "stable": true
        })
    };
    serde_json::from_value(serde_json::json!([
        provider("claude_code", "Claude Code", "claude-opus-5-5", "Opus 5.5"),
        provider("codex", "Codex", "sol-6.1", "Sol 6.1"),
    ]))
    .unwrap()
}

#[gpui::test]
fn a_model_and_its_effort_are_picked_from_the_panel(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (editor, cx) = cx.add_window_view(|window, cx| {
        AgentEditor::new(super::editor_role::blank_agent(), catalog(), window, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.0), gpui::px(2400.0)));
    cx.run_until_parked();
    let click = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
        let at = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector}"));
        cx.simulate_click(at.center(), Modifiers::none());
        cx.run_until_parked();
    };
    click(cx, "agent-editor-runs-on");
    assert!(cx.debug_bounds("agent-runs-panel").is_some());
    click(cx, "agent-runs-tab-codex");
    click(cx, "agent-runs-model-0");
    click(cx, "agent-runs-effort-high");
    editor.update(cx, |editor, _| {
        assert_eq!(editor.provider_id.as_deref(), Some("codex"));
        assert_eq!(editor.model.as_deref(), Some("sol-6.1"));
        assert_eq!(editor.effort.as_deref(), Some("high"));
    });
    // Codex has no accounts here: no strip.
    assert!(cx.debug_bounds("agent-runs-account-0").is_none());

    // On Claude, the account strip: "Padrão" and Zedy.
    editor.update(cx, |editor, _| {
        editor.accounts = vec![dcc_core::domain::provider::ProviderAccount {
            id: "acc-zedy".into(),
            provider_id: dcc_core::domain::provider::ProviderId("claude_code".into()),
            label: "Zedy".into(),
            config_dir: "/x".into(),
            managed: true,
            created_at: "2026-11-09T10:00:00Z".into(),
        }];
    });
    // The strip follows the tab shown: the account picked before the model
    // goes with it, though the agent was on Codex.
    click(cx, "agent-runs-tab-claude_code");
    click(cx, "agent-runs-account-1");
    editor.update(cx, |editor, _| assert_eq!(editor.account_id, None));
    click(cx, "agent-runs-model-0");
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.provider_id.as_deref(), Some("claude_code"));
        assert_eq!(editor.account_id.as_deref(), Some("acc-zedy"));
        assert!(editor.summary(cx).ends_with("Zedy"));
    });
    // Back on Codex, the Claude account does not follow.
    click(cx, "agent-runs-tab-codex");
    click(cx, "agent-runs-model-0");
    editor.update(cx, |editor, _| assert_eq!(editor.account_id, None));
}
