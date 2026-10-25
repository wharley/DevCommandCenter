//! macOS menu bar and its shortcuts, matching the desktop's default
//! application, Edit and Window menus.
//!
//! Edit's items carry AppKit's own selectors (`copy:`, `paste:`…): GPUI's
//! fields keep their own ⌘C/⌘V (their bindings win), and a page in the
//! Browser — a native view GPUI does not see into — gets them through the
//! responder chain, the only way WebKit takes them.

use gpui::{App, KeyBinding, Menu, MenuItem, OsAction, SystemMenuType, actions};

actions!(
    dcc_app,
    [
        Quit,
        Hide,
        HideOthers,
        ShowAll,
        Minimize,
        Zoom,
        CloseWindow,
        EditUndo,
        EditRedo,
        EditCut,
        EditCopy,
        EditPaste,
        EditSelectAll
    ]
);

pub const APP_NAME: &str = "Dev Command Center";

pub fn install(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Minimize, cx| with_active_window(cx, |window| window.minimize_window()));
    cx.on_action(|_: &Zoom, cx| with_active_window(cx, |window| window.zoom_window()));
    cx.on_action(|_: &CloseWindow, cx| with_active_window(cx, |window| window.remove_window()));

    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        // The menu shows these keys; nothing in GPUI handles the actions,
        // so a key no field takes goes on to the focused native view.
        KeyBinding::new("cmd-z", EditUndo, None),
        KeyBinding::new("cmd-shift-z", EditRedo, None),
        KeyBinding::new("cmd-x", EditCut, None),
        KeyBinding::new("cmd-c", EditCopy, None),
        KeyBinding::new("cmd-v", EditPaste, None),
        KeyBinding::new("cmd-a", EditSelectAll, None),
    ]);

    cx.set_menus([
        Menu::new(APP_NAME).items([
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action(format!("Hide {APP_NAME}"), Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action(format!("Quit {APP_NAME}"), Quit),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", EditUndo, OsAction::Undo),
            MenuItem::os_action("Redo", EditRedo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", EditCut, OsAction::Cut),
            MenuItem::os_action("Copy", EditCopy, OsAction::Copy),
            MenuItem::os_action("Paste", EditPaste, OsAction::Paste),
            MenuItem::os_action("Select All", EditSelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
    ]);
}

fn with_active_window(cx: &mut App, action: impl FnOnce(&mut gpui::Window)) {
    if let Some(handle) = cx.active_window() {
        handle.update(cx, |_, window, _| action(window)).ok();
    }
}
