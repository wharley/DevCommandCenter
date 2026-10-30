//! `ImageAttachment`: an attached image shows as itself in the person's
//! message (`max-h-28 max-w-[14rem]`, its proportions kept), not as its
//! generated file name; a click opens it full size in a dialog. The chip when
//! the file is gone or cannot be read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::{AnyElement, App, Context, Global, Window, div, img, prelude::*, px};

use super::ConversationView;
use super::user_row::chip;
use crate::theme::ActiveTheme;
use crate::ui::Dialog;

/// `max-h-28`, `max-w-[14rem]`.
const THUMB_HEIGHT: f32 = 112.0;
const THUMB_WIDTH: f32 = 224.0;

/// Each attached image's size in pixels, read once from its header.
#[derive(Default)]
struct Sizes(HashMap<String, Option<(f32, f32)>>);

impl Global for Sizes {}

fn size_of(path: &str, cx: &mut App) -> Option<(f32, f32)> {
    if let Some(size) = cx.default_global::<Sizes>().0.get(path) {
        return *size;
    }
    let size = Path::new(path)
        .is_absolute()
        .then(|| image::image_dimensions(path).ok())
        .flatten()
        .filter(|(width, height)| *width > 0 && *height > 0)
        .map(|(width, height)| (width as f32, height as f32));
    cx.global_mut::<Sizes>().0.insert(path.to_string(), size);
    size
}

/// The size that fits `width`×`height` in `max_width`×`max_height`, never
/// enlarged.
fn fit(size: (f32, f32), max_width: f32, max_height: f32) -> (f32, f32) {
    let scale = (max_width / size.0).min(max_height / size.1).min(1.0);
    (size.0 * scale, size.1 * scale)
}

/// The thumbnail for an attached image, or `None` when it cannot be read.
pub(super) fn thumbnail(path: &str, cx: &mut Context<ConversationView>) -> Option<AnyElement> {
    let (width, height) = fit(size_of(path, cx)?, THUMB_WIDTH, THUMB_HEIGHT);
    let theme = cx.theme().clone();
    let colors = &theme.colors;
    let fallback_path = path.to_string();
    let open_path = path.to_string();
    Some(
        div()
            .id(gpui::SharedString::from(format!("attachment-image-{path}")))
            .debug_selector(|| "user-message-image".into())
            .overflow_hidden()
            .rounded(theme.radius.lg)
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background.opacity(0.7))
            .cursor_pointer()
            .hover(|style| style.opacity(0.9))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |view, _, _, cx| {
                view.image_open = Some(open_path.clone());
                cx.notify();
            }))
            .child(
                img(PathBuf::from(path))
                    .w(px(width))
                    .h(px(height))
                    .with_fallback(move || chip(&fallback_path, &theme)),
            )
            .into_any_element(),
    )
}

impl ConversationView {
    /// The opened image, as large as the window allows, in a dialog.
    pub(super) fn image_viewer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let path = self.image_open.clone()?;
        let size = size_of(&path, cx)?;
        let viewport = window.viewport_size();
        // `max-h-[90vh] sm:max-w-[min(90vw,72rem)] p-2`.
        let max_width = (f32::from(viewport.width) * 0.9).min(1152.0);
        let max_height = f32::from(viewport.height) * 0.9;
        let (width, height) = fit(size, max_width - 16.0, max_height - 16.0);
        let this = cx.entity().downgrade();
        Some(
            Dialog::new("conversation-image", move |_, cx| {
                let _ = this.update(cx, |view, cx| {
                    view.image_open = None;
                    cx.notify();
                });
            })
            .max_width(width + 16.0)
            .padding(8.0)
            .child(
                div()
                    .debug_selector(|| "conversation-image-viewer".into())
                    .overflow_hidden()
                    .rounded(cx.theme().radius.md)
                    .child(img(PathBuf::from(path)).w(px(width)).h(px(height))),
            )
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::fit;

    #[test]
    fn a_thumbnail_keeps_its_proportions() {
        assert_eq!(fit((1600.0, 800.0), 224.0, 112.0), (224.0, 112.0));
        assert_eq!(fit((400.0, 800.0), 224.0, 112.0), (56.0, 112.0));
        assert_eq!(fit((2240.0, 560.0), 224.0, 112.0), (224.0, 56.0));
        assert_eq!(
            fit((50.0, 40.0), 224.0, 112.0),
            (50.0, 40.0),
            "never enlarged"
        );
    }
}
