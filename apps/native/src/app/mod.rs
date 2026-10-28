//! Boot: the login shell's environment, the backend (when no desktop serves
//! it), fonts, theme, menus and the main window.

mod color_space;
#[cfg(target_os = "macos")]
pub mod global_keys;
pub mod menu_bar;
#[cfg(target_os = "macos")]
mod menu_bar_item;
mod menus;
pub mod quick_composer;
mod window;

use crate::assets::Assets;
use crate::i18n::I18n;
use crate::prefs::{self, Prefs};
use crate::theme::{self, Theme};

/// The platform to run on. On Linux, X11 — natively or through XWayland on
/// a Wayland session — since the Browser's WebKitGTK only embeds into an
/// X11 window; with no X server at all, whatever the session offers.
fn platform() -> gpui::Application {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty()) {
        return gpui_platform::linux(gpui::WindowingModes::X11);
    }
    gpui_platform::application()
}

pub fn run() {
    // Before any other thread: the login shell's PATH for the CLIs, the
    // shells and the agents.
    #[cfg(not(windows))]
    if !cfg!(test) {
        dcc_server::shell_env::bootstrap();
    }
    crate::data::host::start();
    crate::data::provider_catalog::locate_claude_helper();
    let prefs_path = prefs::default_path();
    let prefs = Prefs::load(&prefs_path);

    platform().with_assets(Assets).run(move |cx| {
        if let Err(error) = theme::fonts::register(cx) {
            eprintln!("[dcc-native] fonts unavailable, using the system font: {error:#}");
        }
        Theme::init(prefs.theme, cx);
        I18n::init(prefs.locale, cx);
        cx.set_global(prefs.density);
        crate::data::ux_metrics::start();
        crate::data::diagnostics::install();
        menus::install(cx);
        crate::ui::text_input::bind_keys(cx);
        crate::ui::text_area::bind_keys(cx);
        crate::screens::shell::bind_keys(cx);
        crate::screens::conversation::bind_keys(cx);
        crate::screens::stage::bind_keys(cx);
        crate::screens::file_view::bind_keys(cx);
        crate::screens::annotation::bind_keys(cx);
        crate::ui::Toaster::init(cx);
        cx.on_app_quit(|_| {
            crate::data::host::shutdown();
            async {}
        })
        .detach();
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
        menu_bar::install(cx);
        // The quick composer's key while this app hosts the backend.
        #[cfg(target_os = "macos")]
        global_keys::serve(cx);
        quick_composer::listen(cx);
        cx.activate(true);
    });
}
