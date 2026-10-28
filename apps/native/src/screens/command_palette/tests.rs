//! The palette driven headlessly: with no task it only has the global
//! actions; typing filters them and brings the help topics; with a task it
//! lists the workbench commands, the tasks, the conversations and the
//! files; an `@` hides the recent conversations until the search answers.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

use super::{CommandPalette, PaletteCommand, PaletteContext, PaletteEvent, WorkbenchCommand};
use crate::data::command_palette::{PaletteSession, PaletteTask};
use crate::theme::{Theme, ThemePreference};

type Ran = Rc<RefCell<Vec<Option<PaletteCommand>>>>;

fn open(
    context: PaletteContext,
    cx: &mut TestAppContext,
) -> (Entity<CommandPalette>, &mut VisualTestContext, Ran) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (palette, cx) =
        cx.add_window_view(move |window, cx| CommandPalette::new(context, window, cx));
    let ran: Ran = Rc::default();
    let sink = ran.clone();
    cx.update(|_, cx| {
        cx.subscribe(&palette, move |_, event: &PaletteEvent, _| {
            sink.borrow_mut().push(match event {
                PaletteEvent::Run(command) => Some(command.clone()),
                PaletteEvent::Closed => None,
            })
        })
        .detach();
    });
    cx.run_until_parked();
    (palette, cx, ran)
}

fn empty_context() -> PaletteContext {
    PaletteContext {
        selected_task: None,
        tasks: Vec::new(),
        recent: Vec::new(),
        root: None,
        db_path: std::path::PathBuf::from("/nonexistent/database.sqlite"),
    }
}

fn labels(palette: &Entity<CommandPalette>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| palette.read(cx).labels())
}

#[gpui::test]
fn without_a_task_only_the_global_actions_and_typing_filters(cx: &mut TestAppContext) {
    let (palette, cx, ran) = open(empty_context(), cx);
    assert_eq!(labels(&palette, cx).len(), 8);
    assert!(cx.debug_bounds("command-palette-field").is_some());
    cx.simulate_input("atalhos");
    cx.run_until_parked();
    assert_eq!(labels(&palette, cx), ["Atalhos de teclado"]);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    // A help topic only shows while typing, labelled "Ajuda: …".
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("terminal");
    cx.run_until_parked();
    let found = labels(&palette, cx);
    assert!(
        found.iter().any(|label| label == "Ajuda: Terminal"),
        "{found:?}"
    );
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("nada que exista zzz");
    cx.run_until_parked();
    assert!(labels(&palette, cx).is_empty());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        ran.borrow().as_slice(),
        [Some(PaletteCommand::Shortcuts), None]
    );
}

#[gpui::test]
fn with_a_task_it_lists_commands_tasks_sessions_and_files(cx: &mut TestAppContext) {
    let repo = crate::test_support::git_checkout(&[("src/main.rs", "fn main() {}\n")]);
    let session = PaletteSession {
        session_id: "s-1".into(),
        task_id: "t-1".into(),
        title: "Retry no checkout".into(),
        task_name: Some("Checkout".into()),
        branch: Some("main".into()),
        snippet: String::new(),
        archived: false,
        updated_at: "2026-10-26T10:00:00Z".into(),
    };
    let context = PaletteContext {
        selected_task: Some("t-1".into()),
        tasks: vec![
            PaletteTask {
                id: "t-1".into(),
                name: "Checkout".into(),
                branch: "main".into(),
            },
            PaletteTask {
                id: "t-2".into(),
                name: "Login".into(),
                branch: "feat/login".into(),
            },
        ],
        recent: vec![session.clone()],
        root: Some(repo.path().to_string_lossy().into_owned()),
        db_path: std::path::PathBuf::from("/nonexistent/database.sqlite"),
    };
    let (palette, cx, ran) = open(context, cx);
    let found = labels(&palette, cx);
    // 10 workbench commands + 8 global, 2 tasks, 1 conversation, 1 file.
    assert_eq!(found.len(), 22, "{found:?}");
    assert!(found.contains(&"Abrir detalhes do workspace".to_string()));
    assert!(found.contains(&"Abrir atividade da sessão".to_string()));
    assert!(found.contains(&"Retry no checkout".to_string()));
    assert!(found.contains(&"main.rs".to_string()));
    // Its branch narrows to that task.
    cx.simulate_input("feat/login");
    cx.run_until_parked();
    let found = labels(&palette, cx);
    assert_eq!(
        found.first().map(String::as_str),
        Some("Login"),
        "{found:?}"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    // `@` hides the recent conversations; the search waits 180ms.
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("@re");
    cx.run_until_parked();
    assert!(!labels(&palette, cx).contains(&"Retry no checkout".to_string()));
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| palette.read(cx).hits.is_some()));
    // The first item is the composer's focus; a click runs it.
    cx.simulate_keystrokes("cmd-a backspace");
    cx.run_until_parked();
    let bounds = cx.debug_bounds("palette-item-0").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        ran.borrow().as_slice(),
        [
            Some(PaletteCommand::OpenTask("t-2".into())),
            Some(PaletteCommand::Workbench(WorkbenchCommand::FocusComposer)),
        ]
    );
}
