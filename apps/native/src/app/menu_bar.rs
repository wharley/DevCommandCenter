//! The menu bar item and its panel: a click opens the panel under the item
//! (368×480, centered on it) or closes it; the panel goes when it loses the
//! focus, like the desktop's transient `NSPopover`. Off macOS there is no
//! item.

use gpui::{
    App, AppContext, Bounds, Global, WindowBounds, WindowHandle, WindowKind, WindowOptions, point,
    px, size,
};

use crate::screens::menu_bar::{HEIGHT, MenuBarPanel, WIDTH};
use crate::screens::shell::Shell;

/// The main window, for the panel's "Abrir DCC" and its tasks.
pub struct MainWindow(pub WindowHandle<Shell>);

impl Global for MainWindow {}

/// The panel while it shows, and when it last went away on its own.
#[derive(Default)]
struct OpenPanel(
    Option<WindowHandle<MenuBarPanel>>,
    Option<std::time::Instant>,
);

impl Global for OpenPanel {}

/// Puts the item in the bar; its clicks reach the app through a channel.
pub fn install(cx: &mut App) {
    #[cfg(target_os = "macos")]
    {
        let (send, mut receive) = futures::channel::mpsc::unbounded();
        if !super::menu_bar_item::install(move |place| {
            let _ = send.unbounded_send(place);
        }) {
            eprintln!("[dcc-native] could not create the menu bar item");
            return;
        }
        cx.spawn(async move |cx| {
            use futures::StreamExt;
            while let Some(place) = receive.next().await {
                cx.update(|cx| toggle(place, cx));
            }
        })
        .detach();
    }
    let _ = cx;
}

/// Whether the window should hide on close instead of quitting.
pub fn hides_on_close() -> bool {
    #[cfg(target_os = "macos")]
    {
        super::menu_bar_item::installed()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn toggle(place: super::menu_bar_item::ButtonPlace, cx: &mut App) {
    if close_panel(cx) {
        return;
    }
    // Clicking the item while the panel shows first takes its focus away,
    // which closes it; that click must not open it again.
    if cx
        .try_global::<OpenPanel>()
        .and_then(|open| open.1)
        .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(300))
    {
        return;
    }
    let x = place.x + place.width / 2.0 - WIDTH / 2.0;
    let y = place.y + place.height + 4.0;
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(x), px(y)),
            size(px(WIDTH), px(HEIGHT)),
        ))),
        titlebar: None,
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        app_id: Some("com.devcommandcenter.native".into()),
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| {
        cx.new(|cx| MenuBarPanel::new(window, cx))
    }) {
        Ok(handle) => cx.set_global(OpenPanel(Some(handle), None)),
        Err(error) => eprintln!("[dcc-native] could not open the menu bar panel: {error:#}"),
    }
}

/// `dcc_menu_bar_hide`: true when a panel was open.
pub fn close_panel(cx: &mut App) -> bool {
    let Some(handle) = cx.try_global::<OpenPanel>().and_then(|open| open.0) else {
        return false;
    };
    cx.set_global(OpenPanel(None, None));
    handle
        .update(cx, |_, window, _| window.remove_window())
        .is_ok()
}

/// The panel closed itself (an action): nothing to close any more.
pub fn forget_panel(cx: &mut App) {
    cx.set_global(OpenPanel(None, None));
}

/// The panel lost the focus and closed itself.
pub fn panel_left(cx: &mut App) {
    cx.set_global(OpenPanel(None, Some(std::time::Instant::now())));
}

/// The main window in front (it may have been hidden by closing it).
pub fn show_main(cx: &mut App) -> Option<WindowHandle<Shell>> {
    let handle = cx.try_global::<MainWindow>()?.0;
    cx.activate(true);
    let _ = handle.update(cx, |_, window, _| window.activate_window());
    Some(handle)
}
