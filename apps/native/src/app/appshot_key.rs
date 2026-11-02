//! A press of the Appshots key, on the UI side: the main window comes
//! forward and the capture goes into the composer of the task on screen,
//! as the desktop's `appshots-ready` does; a failure shows as a toast.

use futures::StreamExt;
use gpui::App;

use crate::data::appshot_key::{Captured, captures};

pub fn listen(cx: &mut App) {
    if cfg!(test) {
        return;
    }
    let Some(mut captured) = captures() else {
        return;
    };
    cx.spawn(async move |cx| {
        while let Some(capture) = captured.next().await {
            cx.update(|cx| deliver(capture, cx));
        }
    })
    .detach();
}

fn deliver(capture: Captured, cx: &mut App) {
    let Some(handle) = super::menu_bar::show_main(cx) else {
        if let Ok(path) = capture {
            let _ = std::fs::remove_file(path);
        }
        return;
    };
    let _ = handle.update(cx, |shell, _, cx| shell.take_appshot(capture, cx));
}
