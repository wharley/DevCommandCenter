//! macOS menu bar and its shortcuts, matching the desktop's default
//! application and Window menus. Edit arrives with the first text field.

use gpui::{App, KeyBinding, Menu, MenuItem, SystemMenuType, actions};

actions!(
    dcc_app,
    [Quit, Hide, HideOthers, ShowAll, Minimize, Zoom, CloseWindow]
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
