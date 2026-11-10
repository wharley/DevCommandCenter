//! Images in the team room, the composer's way (`composer::attachments`):
//! one pasted is saved to the temp folder and one dropped is taken as it
//! is, each a chip in the field; what goes to the agent is the text with
//! `@/path.png` where each chip was, which both providers send as the image
//! itself. The person's bubble shows them as thumbnails; a click opens one.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, ClipboardEntry, Context, ImageFormat, SharedString, div, img, prelude::*, px,
};

use super::room::TeamRoom;
use crate::data::prompt::is_image_path;
use crate::i18n::Translate;
use crate::screens::composer::prompt::serialize;
use crate::screens::conversation::image::{fit, size_of};
use crate::theme::ActiveTheme;
use crate::ui::Toaster;
use crate::ui::text_area::Chip;

/// The thumbnail's bounds in the bubble (the timeline's `max-h-28`).
const THUMB_WIDTH: f32 = 224.0;
const THUMB_HEIGHT: f32 = 112.0;

fn extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Gif => "gif",
        ImageFormat::Webp => "webp",
        _ => "png",
    }
}

impl TeamRoom {
    /// What goes to the agent: the field's text with each chip as `@path`.
    pub(crate) fn prompt_text(&self, cx: &gpui::App) -> String {
        let input = self.input.read(cx);
        serialize(input.text(), &input.chips()).trim().to_string()
    }

    /// Files dropped on the field go in as chips, images as images.
    pub(crate) fn add_paths(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let chips: Vec<Chip> = paths
            .iter()
            .map(|path| {
                let text = path.to_string_lossy().to_string();
                if is_image_path(&text) {
                    // Without spaces, so the provider attaches it.
                    let image = dcc_infra::prompt_attachments::image_for_prompt(path);
                    Chip::Image(image.to_string_lossy().to_string())
                } else {
                    Chip::File(text)
                }
            })
            .collect();
        self.input.update(cx, |input, cx| {
            for chip in chips {
                input.insert_chip(chip, None, cx);
            }
        });
    }

    /// An image on the clipboard: saved to the temp folder, then a chip.
    pub(crate) fn paste_image(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let images: Vec<(Vec<u8>, &'static str)> = item
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::Image(image) => {
                    Some((image.bytes.clone(), extension(image.format)))
                }
                _ => None,
            })
            .collect();
        if images.is_empty() {
            return;
        }
        let failure = cx.t("composer.attachments.error");
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    images
                        .into_iter()
                        .map(|(bytes, ext)| {
                            dcc_infra::prompt_attachments::save_temp_image(&bytes, ext)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                for result in saved {
                    match result {
                        Ok(path) => room.add_paths(&[path], cx),
                        Err(error) => Toaster::error_with(failure.clone(), error.to_string(), cx),
                    }
                }
            });
        })
        .detach();
    }

    /// The person's images under their words, or nothing.
    pub(super) fn image_thumbs(
        &self,
        key: &str,
        images: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if images.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let thumbs: Vec<AnyElement> = images
            .into_iter()
            .enumerate()
            .filter_map(|(index, path)| {
                // A file gone (the temp folder was cleaned) shows nothing.
                let (width, height) = fit(size_of(&path, cx)?, THUMB_WIDTH, THUMB_HEIGHT);
                let url = format!("file://{path}");
                Some(
                    div()
                        .id(SharedString::from(format!("{key}-image-{index}")))
                        .debug_selector({
                            let key = key.to_string();
                            move || format!("{key}-image-{index}")
                        })
                        .overflow_hidden()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.6))
                        .cursor_pointer()
                        .hover(|thumb| thumb.opacity(0.9))
                        .on_click(move |_, _, cx| cx.open_url(&url))
                        .child(
                            img(Path::new(&path).to_path_buf())
                                .w(px(width))
                                .h(px(height)),
                        )
                        .into_any_element(),
                )
            })
            .collect();
        (!thumbs.is_empty()).then(|| {
            div()
                .flex()
                .flex_wrap()
                .justify_end()
                .gap(px(6.0))
                .max_w(px(520.0))
                .children(thumbs)
                .into_any_element()
        })
    }
}
