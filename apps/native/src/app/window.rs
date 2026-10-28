//! The main window, sized like the desktop's (`tauri.conf.json`).

use std::path::PathBuf;

use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

use super::color_space;
use super::menus::APP_NAME;
use crate::prefs::Prefs;
use crate::screens::shell::Shell;

const WIDTH: f32 = 1400.0;
const HEIGHT: f32 = 900.0;
const MIN_WIDTH: f32 = 1000.0;
const MIN_HEIGHT: f32 = 700.0;

pub fn open_main(prefs: Prefs, prefs_path: PathBuf, cx: &mut App) -> anyhow::Result<()> {
    let bounds = Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(MIN_WIDTH), px(MIN_HEIGHT))),
            // Standard AppKit title bar, like the desktop window.
            titlebar: Some(TitlebarOptions {
                title: Some(APP_NAME.into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            app_id: Some("com.devcommandcenter.native".into()),
            ..Default::default()
        },
        |window, cx| {
            color_space::tag_as_srgb(window);
            // With the menu bar item, closing hides the app (`prevent_close`).
            window.on_window_should_close(cx, |_, cx| {
                if super::menu_bar::hides_on_close() {
                    cx.hide();
                    false
                } else {
                    true
                }
            });
            cx.new(|cx| Shell::new(prefs, prefs_path, window, cx))
        },
    )?;
    cx.set_global(super::menu_bar::MainWindow(handle));
    Ok(())
}
