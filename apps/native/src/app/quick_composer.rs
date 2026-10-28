//! The quick composer's window: 720×440 in the middle of the screen, over
//! any app without bringing the DCC forward (a non-activating panel that
//! joins every Space, like the desktop's). The desktop keeps the global key
//! until the switch (screen 12): while this app listens on its stream, the
//! key (and Settings' "Abrir") open this one. A launch that went, Esc and ✕
//! close it; the draft stays in the composer.

use std::time::Duration;

use gpui::{
    App, AppContext, Bounds, Focusable, Global, TitlebarOptions, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, px, size,
};

use crate::screens::quick_composer::{HEIGHT, QuickComposer, QuickComposerEvent, WIDTH};

#[derive(Default)]
struct OpenQuick(Option<WindowHandle<QuickComposer>>);

impl Global for OpenQuick {}

/// Listens for the key; reconnects while the desktop comes and goes.
pub fn listen(cx: &mut App) {
    if cfg!(test) {
        return;
    }
    let (send, mut receive) = futures::channel::mpsc::unbounded::<()>();
    std::thread::spawn(move || {
        loop {
            if let Some(api) = crate::data::desktop_api::DesktopApi::load()
                && let Ok(toggles) = api.quick_composer_toggles()
            {
                for () in toggles {
                    if send.unbounded_send(()).is_err() {
                        return;
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
    cx.spawn(async move |cx| {
        use futures::StreamExt;
        while receive.next().await.is_some() {
            cx.update(toggle);
        }
    })
    .detach();
}

/// `dcc_quick_composer_toggle`
pub fn toggle(cx: &mut App) {
    if !close(cx) {
        show(cx);
    }
}

/// `dcc_quick_composer_show`: opens it, or brings it forward.
pub fn show(cx: &mut App) {
    if let Some(handle) = cx.try_global::<OpenQuick>().and_then(|open| open.0)
        && cx
            .windows()
            .iter()
            .any(|window| window.window_id() == handle.window_id())
        && handle
            .update(cx, |quick, window, cx| {
                window.activate_window();
                quick
                    .composer
                    .update(cx, |composer, cx| composer.focus(window, cx));
            })
            .is_ok()
    {
        return;
    }
    let bounds = Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("DCC".into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: false,
        app_id: Some("com.devcommandcenter.native".into()),
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        let quick = cx.new(|cx| QuickComposer::new(window, cx));
        window.focus(&quick.focus_handle(cx), cx);
        cx.subscribe(&quick, |_, event, cx| match event {
            QuickComposerEvent::Close => {
                close(cx);
            }
            QuickComposerEvent::OpenTask {
                workspace_id,
                session_id,
            } => {
                let (workspace_id, session_id) = (workspace_id.clone(), session_id.clone());
                close(cx);
                if let Some(main) = super::menu_bar::show_main(cx)
                    && let Some(workspace_id) = workspace_id
                {
                    let _ = main.update(cx, |shell, _, cx| {
                        shell.open_from_menu_bar(workspace_id, session_id, cx)
                    });
                }
            }
        })
        .detach();
        quick
    });
    match opened {
        Ok(handle) => cx.set_global(OpenQuick(Some(handle))),
        Err(error) => eprintln!("[dcc-native] could not open the quick composer: {error:#}"),
    }
}

/// `dcc_quick_composer_hide`: true when it was open.
pub fn close(cx: &mut App) -> bool {
    let Some(handle) = cx.try_global::<OpenQuick>().and_then(|open| open.0) else {
        return false;
    };
    cx.set_global(OpenQuick(None));
    let id = handle.window_id();
    if !cx.windows().iter().any(|window| window.window_id() == id) {
        return false;
    }
    // From inside its own events the window is busy: close it next.
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    });
    true
}
