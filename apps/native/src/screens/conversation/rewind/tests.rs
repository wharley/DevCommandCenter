//! "Editar a partir daqui" on screen: the dialog offers restoring only
//! restorable files, a stop replaces the choices, and the message goes back
//! to the composer, replacing the draft.

use dcc_tauri::conversation_rewind::{RewindFilePreview, RewindFilesPlan, RewindProviderPlan};

use gpui::{TestAppContext, WindowAppearance};

use super::super::ConversationView;
use crate::data::rewind::{Outcome, Plan, Stopped};
use crate::theme::{Theme, ThemePreference};

fn open(cx: &mut TestAppContext) -> (gpui::Entity<ConversationView>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::Toaster::init(cx);
    });
    cx.add_window_view(ConversationView::new)
}

fn plan(files: RewindFilesPlan) -> Plan {
    Plan {
        session_id: "s1".into(),
        anchor_turn_id: "t2".into(),
        removed_turn_ids: vec!["t2".into(), "t3".into()],
        provider: RewindProviderPlan::Native,
        files,
    }
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui::test]
fn restoring_is_offered_only_for_restorable_files(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let restorable = RewindFilesPlan::Restorable {
        turn_count: 2,
        file_count: 1,
        files: vec![RewindFilePreview {
            turn_id: "t3".into(),
            project: None,
            display_path: "src/app.rs".into(),
            binary: false,
            preview: None,
        }],
    };
    view.update(cx, |view, cx| {
        view.rewind.target = Some(plan(restorable));
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("edit-from-here-body").is_some());
    assert!(cx.debug_bounds("edit-from-here-restore").is_some());
    assert!(cx.debug_bounds("edit-from-here-keep").is_some());

    view.update(cx, |view, cx| {
        view.rewind.target = Some(plan(RewindFilesPlan::NothingToRestore));
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("edit-from-here-restore").is_none());
    assert!(cx.debug_bounds("edit-from-here-keep").is_some());

    view.update(cx, |view, cx| view.cancel_rewind(cx));
    draw(cx);
    assert!(cx.debug_bounds("edit-from-here-body").is_none());
}

#[gpui::test]
fn a_stop_replaces_the_choices_and_a_rewind_fills_the_composer(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.rewind.target = Some(plan(RewindFilesPlan::NothingToRestore));
        view.rewound(
            Ok(Outcome::FilesStopped(Stopped {
                restored_turns: 1,
                result_key: "turnReview.guardedUndo.results.blocked".into(),
                reason_key: None,
                project: None,
                recovery_required: false,
            })),
            cx,
        );
    });
    draw(cx);
    assert!(cx.debug_bounds("edit-from-here-stopped").is_some());
    assert!(
        cx.debug_bounds("edit-from-here-keep").is_none(),
        "no choices after a stop"
    );

    view.update(cx, |view, cx| {
        view.rewound(
            Ok(Outcome::Rewound {
                prompt: "Refaça o teste".into(),
                restored_turns: 0,
            }),
            cx,
        );
    });
    draw(cx);
    assert!(cx.debug_bounds("edit-from-here-body").is_none());
    let draft = view.read_with(cx, |view, cx| {
        view.composer.read(cx).input.read(cx).text().to_string()
    });
    assert_eq!(draft, "Refaça o teste");
}

/// Without a session the message is only copied back.
#[gpui::test]
fn a_prompt_without_a_turn_goes_straight_to_the_composer(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.edit_from_here(None, "Só isso".into(), cx)
    });
    draw(cx);
    let draft = view.read_with(cx, |view, cx| {
        view.composer.read(cx).input.read(cx).text().to_string()
    });
    assert_eq!(draft, "Só isso");
}
