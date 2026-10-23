//! Boot: fonts, theme, menus and the main window.

mod color_space;
mod menus;
mod window;

use crate::prefs::{self, Prefs};
use crate::theme::{self, Theme};

pub fn run() {
    let prefs_path = prefs::default_path();
    let prefs = Prefs::load(&prefs_path);

    gpui_platform::application().run(move |cx| {
        if let Err(error) = theme::fonts::register(cx) {
            eprintln!("[dcc-native] fonts unavailable, using the system font: {error:#}");
        }
        Theme::init(prefs.theme, cx);
        menus::install(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        if let Err(error) = window::open_main(prefs, prefs_path, cx) {
            eprintln!("[dcc-native] could not open the window: {error:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}
