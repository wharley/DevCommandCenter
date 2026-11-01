//! The images in the prompt shown as themselves above the field, as the
//! Monocode's composer does: a 48px thumbnail per image chip, in text order,
//! with a close button that takes the chip out and a click that opens the
//! image full size in the conversation. The chip in the text keeps where the
//! image goes in the prompt.

use std::path::{Path, PathBuf};

use gpui::{AnyElement, Context, ObjectFit, SharedString, div, img, prelude::*, px};

use super::{Composer, ComposerEvent};
use crate::theme::ActiveTheme;
use crate::ui::Icon;
use crate::ui::text_area::Chip;

const THUMB: f32 = 48.0;
const CLOSE: f32 = 18.0;

/// Where a chip's image is on disk: stored relative to the workspace when it
/// lies inside it, as `prompt::relative_to_workspace` keeps it.
fn on_disk(stored: &str, workspace: Option<&str>) -> PathBuf {
    match workspace {
        Some(root) if !Path::new(stored).is_absolute() => Path::new(root).join(stored),
        _ => PathBuf::from(stored),
    }
}

/// Each image chip in the prompt: its offset in the text and its file.
fn images(composer: &Composer, cx: &gpui::App) -> Vec<(usize, PathBuf)> {
    let workspace = composer.session.workspace_path.as_deref();
    composer
        .input
        .read(cx)
        .chips()
        .into_iter()
        .filter_map(|(offset, chip)| match chip {
            Chip::Image(path) => Some((offset, on_disk(&path, workspace))),
            _ => None,
        })
        .collect()
}

pub(super) fn image_strip(composer: &Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let images = images(composer, cx);
    if images.is_empty() {
        return None;
    }
    let theme = cx.theme().clone();
    let colors = &theme.colors;
    let (close_bg, close_fg) = (colors.foreground.opacity(0.75), colors.background);
    Some(
        div()
            .debug_selector(|| "composer-images".into())
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .pt(px(4.0))
            .pb(px(8.0))
            .children(images.into_iter().map(|(offset, path)| {
                let open = path.to_string_lossy().to_string();
                div()
                    .id(SharedString::from(format!("composer-image-{offset}")))
                    .debug_selector(move || format!("composer-image-{offset}"))
                    .relative()
                    .flex_shrink_0()
                    .size(px(THUMB))
                    .rounded(theme.radius.md)
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .bg(colors.background.opacity(0.7))
                    .cursor_pointer()
                    .hover(|style| style.opacity(0.9))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(ComposerEvent::OpenImage(open.clone()))
                    }))
                    .child(
                        div()
                            .size_full()
                            .overflow_hidden()
                            .rounded(theme.radius.md)
                            .child(
                                img(path)
                                    .size_full()
                                    .object_fit(ObjectFit::Cover)
                                    .with_fallback(|| {
                                        div()
                                            .size_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(Icon::new("image").size(16.0))
                                            .into_any_element()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "composer-image-remove-{offset}"
                            )))
                            .debug_selector(move || format!("composer-image-remove-{offset}"))
                            .absolute()
                            .top(px(-6.0))
                            .right(px(-6.0))
                            .size(px(CLOSE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(close_bg)
                            .text_color(close_fg)
                            .cursor_pointer()
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_click(cx.listener(move |composer, _, _, cx| {
                                cx.stop_propagation();
                                composer
                                    .input
                                    .update(cx, |input, cx| input.remove_chip(offset, cx));
                            }))
                            .child(Icon::new("x").size(11.0).stroke(2.2)),
                    )
                    .into_any_element()
            }))
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use gpui::{ClipboardItem, Image, ImageFormat, Modifiers, TestAppContext};

    use super::on_disk;
    use crate::screens::composer::ComposerEvent;
    use crate::screens::composer::tests::{open, session};
    use crate::ui::text_area::Chip;

    #[test]
    fn an_image_inside_the_workspace_is_found_from_its_root() {
        assert_eq!(
            on_disk("shots/a.png", Some("/repo")),
            PathBuf::from("/repo/shots/a.png")
        );
        assert_eq!(
            on_disk("/tmp/dcc_img_1.png", Some("/repo")),
            PathBuf::from("/tmp/dcc_img_1.png")
        );
        assert_eq!(on_disk("a.png", None), PathBuf::from("a.png"));
    }

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbImage::new(8, 6)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[gpui::test]
    fn a_pasted_image_goes_in_at_the_caret_and_shows_above_the_field(cx: &mut TestAppContext) {
        let (composer, cx) = open(cx);
        composer.update_in(cx, |composer, window, cx| {
            composer.set_session(session("a"), cx);
            composer.focus(window, cx);
        });
        cx.run_until_parked();
        cx.simulate_input("antes depois");
        let opened = Arc::new(Mutex::new(None));
        let seen = opened.clone();
        cx.update(|_, cx| {
            cx.subscribe(&composer, move |_, event: &ComposerEvent, _| {
                if let ComposerEvent::OpenImage(path) = event {
                    *seen.lock().unwrap() = Some(path.clone());
                }
            })
            .detach();
        });
        let image = Image::from_bytes(ImageFormat::Png, png());
        cx.write_to_clipboard(ClipboardItem::new_image(&image));
        cx.simulate_keystrokes("alt-left cmd-v");
        cx.run_until_parked();
        let chips = composer.read_with(cx, |composer, cx| composer.input.read(cx).chips());
        let [(offset, Chip::Image(path))] = chips.as_slice() else {
            panic!("one image chip: {chips:?}");
        };
        assert_eq!(*offset, "antes ".len());

        // Selectors are `'static`.
        let selector = |name: String| -> &'static str { Box::leak(name.into_boxed_str()) };
        let thumb = cx
            .debug_bounds(selector(format!("composer-image-{offset}")))
            .expect("thumbnail");
        cx.simulate_click(thumb.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(opened.lock().unwrap().as_deref(), Some(path.as_str()));

        let remove = cx
            .debug_bounds(selector(format!("composer-image-remove-{offset}")))
            .expect("close");
        cx.simulate_click(remove.center(), Modifiers::none());
        cx.run_until_parked();
        composer.read_with(cx, |composer, cx| {
            assert!(composer.input.read(cx).chips().is_empty());
            assert_eq!(composer.input.read(cx).text(), "antes depois");
        });
        assert!(cx.debug_bounds("composer-images").is_none());
    }
}
