//! An attached video in the person's message: its first frame with a play
//! mark and its length, its proportions kept like an image's
//! (`max-w-[14rem]`); a click plays it in the system's player. A quiet frame
//! while the poster is made, the file chip when there is none.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::{AnyElement, Context, Global, SharedString, div, img, prelude::*, px, rgba};

use super::ConversationView;
use super::image::fit;
use super::user_row::attachment_chip;
use crate::data::audio_clip::format_time;
use crate::data::video::{self, Video};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

const WIDTH: f32 = 224.0;
const MAX_HEIGHT: f32 = 168.0;

enum Poster {
    Loading,
    Ready(Video),
    Failed,
}

#[derive(Default)]
struct Posters(HashMap<String, Poster>);

impl Global for Posters {}

/// The poster when it is made; otherwise it starts and the view draws again.
fn poster(path: &str, cx: &mut Context<ConversationView>) -> Option<Option<Video>> {
    match cx.default_global::<Posters>().0.get(path) {
        Some(Poster::Ready(video)) => return Some(Some(video.clone())),
        Some(Poster::Failed) => return Some(None),
        Some(Poster::Loading) => return None,
        None => {}
    }
    cx.global_mut::<Posters>()
        .0
        .insert(path.to_string(), Poster::Loading);
    let path = path.to_string();
    cx.spawn(async move |view, cx| {
        let file = PathBuf::from(&path);
        let read = cx
            .background_executor()
            .spawn(async move { video::read(&file) })
            .await;
        let _ = view.update(cx, |_, cx| {
            let state = read.map_or(Poster::Failed, Poster::Ready);
            cx.global_mut::<Posters>().0.insert(path, state);
            cx.notify();
        });
    })
    .detach();
    None
}

/// A poster already made (or none), for the screen's tests.
#[cfg(test)]
pub(super) fn seed(path: &str, video: Option<Video>, cx: &mut gpui::App) {
    let state = video.map_or(Poster::Failed, Poster::Ready);
    cx.default_global::<Posters>()
        .0
        .insert(path.to_string(), state);
}

pub(super) fn card(path: &str, cx: &mut Context<ConversationView>) -> AnyElement {
    let video = match poster(path, cx) {
        Some(Some(video)) => Some(video),
        Some(None) => return attachment_chip(path, cx),
        None => None,
    };
    let theme = cx.theme().clone();
    let colors = &theme.colors;
    let (width, height) = video.as_ref().map_or((WIDTH, WIDTH * 9.0 / 16.0), |video| {
        fit(video.size, WIDTH, MAX_HEIGHT)
    });
    let open = path.to_string();
    let frame = match &video {
        Some(video) => img(video.poster.clone())
            .w(px(width))
            .h(px(height))
            .into_any_element(),
        None => div()
            .w(px(width))
            .h(px(height))
            .bg(colors.muted)
            .into_any_element(),
    };
    let length = video
        .as_ref()
        .and_then(|video| video.seconds)
        .map(|seconds| {
            div()
                .absolute()
                .right(px(6.0))
                .bottom(px(6.0))
                .rounded(px(4.0))
                .bg(rgba(0x0000_0099))
                .px(px(5.0))
                .text_size(px(10.0))
                .line_height(px(16.0))
                .text_color(gpui::white())
                .child(format_time(seconds))
        });
    div()
        .id(SharedString::from(format!("attachment-video-{path}")))
        .debug_selector(|| "user-message-video".into())
        .relative()
        .overflow_hidden()
        .rounded(theme.radius.lg)
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background.opacity(0.7))
        .cursor_pointer()
        .hover(|style| style.opacity(0.9))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| cx.open_with_system(Path::new(&open)))
        .child(frame)
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .size(px(34.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgba(0x0000_008c))
                        .child(Icon::new("play-filled").size(14.0).color(gpui::white())),
                ),
        )
        .children(length)
        .into_any_element()
}
