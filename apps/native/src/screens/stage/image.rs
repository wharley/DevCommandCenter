//! An image opened as a tab (`ImageFileView`): it fits the view (never
//! enlarging a small one), zooms in steps or around the pointer with ⌘ and
//! the wheel, pans by dragging when it overflows, and double-click flips
//! between real size and fit. A bar under it tells the zoom, the size in
//! pixels and in bytes.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, ClickEvent, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, Render, ScrollHandle, ScrollWheelEvent, SharedString, Task, Window, canvas, div,
    img, point, prelude::*, px,
};

use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, spinner};

const PADDING: f32 = 24.0;
/// Room under the image so the bar never covers it.
const BOTTOM_PADDING: f32 = 56.0;
pub const MIN_ZOOM: f32 = 0.05;
pub const MAX_ZOOM: f32 = 16.0;
const STEPS: [f32; 17] = [
    0.05, 0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0,
];

/// `isViewableImagePath`: the formats the app reads back as an image.
pub fn is_viewable_image(path: &str) -> bool {
    let lower = path.trim().to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

pub fn clamp_zoom(zoom: f32) -> f32 {
    zoom.clamp(MIN_ZOOM, MAX_ZOOM)
}

/// `fitImageZoom`: the whole image in the box, never enlarged.
pub fn fit_zoom(image: (f32, f32), area: (f32, f32)) -> f32 {
    if image.0 <= 0.0 || image.1 <= 0.0 || area.0 <= 0.0 || area.1 <= 0.0 {
        return 1.0;
    }
    clamp_zoom(1.0f32.min(area.0 / image.0).min(area.1 / image.1))
}

/// `stepImageZoom`: the next preset in or out.
pub fn step_zoom(zoom: f32, inward: bool) -> f32 {
    if inward {
        STEPS
            .iter()
            .copied()
            .find(|step| *step > zoom + 1e-6)
            .unwrap_or(MAX_ZOOM)
    } else {
        STEPS
            .iter()
            .rev()
            .copied()
            .find(|step| *step < zoom - 1e-6)
            .unwrap_or(MIN_ZOOM)
    }
}

/// `formatImageBytes`
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Zoom {
    Fit,
    Manual(f32),
}

pub(crate) enum Picture {
    Loading(#[allow(dead_code)] Task<()>),
    Ready {
        path: PathBuf,
        width: u32,
        height: u32,
        bytes: u64,
    },
    Unavailable,
}

/// `read_image_preview`'s rules: an absolute path to a known image of at
/// most 15 MB; then its size in pixels.
fn read(path: &Path) -> Option<(u32, u32, u64)> {
    use dcc_infra::prompt_attachments::{IMAGE_PREVIEW_MAX_BYTES, image_media_type};
    if !path.is_absolute() {
        return None;
    }
    image_media_type(path)?;
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > IMAGE_PREVIEW_MAX_BYTES {
        return None;
    }
    let (width, height) = image::image_dimensions(path).ok()?;
    Some((width, height, metadata.len()))
}

pub struct ImageView {
    pub(crate) picture: Picture,
    pub(crate) zoom: Zoom,
    scroll: ScrollHandle,
    /// The scroller's size at the last frame (the fit follows it).
    area: (f32, f32),
    /// A zoom around a point: the point in the view and in the image.
    anchor: Option<((f32, f32), (f32, f32))>,
    drag: Option<(Point<Pixels>, Point<Pixels>)>,
}

impl ImageView {
    pub fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let read_path = path.clone();
        let task = cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { read(&read_path) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.picture = match found {
                    Some((width, height, bytes)) => Picture::Ready {
                        path,
                        width,
                        height,
                        bytes,
                    },
                    None => Picture::Unavailable,
                };
                cx.notify();
            });
        });
        Self {
            picture: Picture::Loading(task),
            zoom: Zoom::Fit,
            scroll: ScrollHandle::new(),
            area: (0.0, 0.0),
            anchor: None,
            drag: None,
        }
    }

    fn natural(&self) -> Option<(f32, f32)> {
        match &self.picture {
            Picture::Ready { width, height, .. } => Some((*width as f32, *height as f32)),
            _ => None,
        }
    }

    fn fit(&self) -> f32 {
        self.natural().map_or(1.0, |natural| {
            fit_zoom(
                natural,
                (
                    self.area.0 - PADDING * 2.0,
                    self.area.1 - PADDING - BOTTOM_PADDING,
                ),
            )
        })
    }

    pub(crate) fn scale(&self) -> f32 {
        match self.zoom {
            Zoom::Fit => self.fit(),
            Zoom::Manual(value) => value,
        }
    }

    /// Zoom around a point of the view (its center without one), so what is
    /// under it stays put.
    pub(crate) fn zoom_to(&mut self, next: f32, at: Option<(f32, f32)>, cx: &mut Context<Self>) {
        let scale = self.scale();
        let at = at.unwrap_or((self.area.0 / 2.0, self.area.1 / 2.0));
        let offset = self.scroll.offset();
        let (left, top) = (-f32::from(offset.x), -f32::from(offset.y));
        self.anchor = Some((
            at,
            (
                (left + at.0 - PADDING) / scale,
                (top + at.1 - PADDING) / scale,
            ),
        ));
        self.zoom = Zoom::Manual(clamp_zoom(next));
        cx.notify();
    }

    fn overflows(&self) -> bool {
        let scale = self.scale();
        self.natural().is_some_and(|(width, height)| {
            width * scale + PADDING * 2.0 > self.area.0
                || height * scale + PADDING + BOTTOM_PADDING > self.area.1
        })
    }

    fn double_click(&mut self, at: (f32, f32), cx: &mut Context<Self>) {
        match self.zoom {
            Zoom::Manual(value) if (value - 1.0).abs() < 0.01 => {
                self.zoom = Zoom::Fit;
                cx.notify();
            }
            _ => self.zoom_to(1.0, Some(at), cx),
        }
    }
}

/// The transparent checkerboard behind the image (`bg-[length:16px_16px]`
/// with `--muted` squares).
fn checkerboard(color: gpui::Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let mut y = 0.0;
            let mut row = 0;
            while y < height {
                let mut x = if row % 2 == 0 { 0.0 } else { 8.0 };
                while x < width {
                    let square = gpui::Bounds::new(
                        point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
                        gpui::size(px(8.0f32.min(width - x)), px(8.0f32.min(height - y))),
                    );
                    window.paint_quad(gpui::fill(square, color));
                    x += 16.0;
                }
                y += 8.0;
                row += 1;
            }
        },
    )
    .absolute()
    .inset_0()
}

fn bar_button(
    id: &'static str,
    icon: &'static str,
    active: bool,
    on_click: impl Fn(&mut ImageView, &mut Context<ImageView>) + 'static,
    cx: &mut Context<ImageView>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let hover = colors.muted.opacity(0.6);
    let foreground = colors.foreground;
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .rounded(px(4.0))
        .p(px(4.0))
        .cursor_pointer()
        .when(active, |button| button.bg(hover).text_color(foreground))
        .hover(move |style| style.bg(hover).text_color(foreground))
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| on_click(view, cx)))
        .child(Icon::new(icon).size(14.0).stroke(1.8))
        .into_any_element()
}

impl Render for ImageView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        // The fit follows the scroller's size, known after a frame.
        let bounds = self.scroll.bounds();
        let area = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        if area != self.area && area.0 > 0.0 {
            self.area = area;
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
        let scale = self.scale();
        if let Some((at, image)) = self.anchor.take() {
            let left = image.0 * scale + PADDING - at.0;
            let top = image.1 * scale + PADDING - at.1;
            self.scroll
                .set_offset(point(px(-left.max(0.0)), px(-top.max(0.0))));
        }
        let overflows = self.overflows();
        let content: AnyElement = match &self.picture {
            Picture::Ready {
                path,
                width,
                height,
                ..
            } => {
                let (w, h) = (*width as f32 * scale, *height as f32 * scale);
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .min_w(px(self.area.0))
                    .min_h(px(self.area.1))
                    .w(px(w + PADDING * 2.0))
                    .h(px(h + PADDING + BOTTOM_PADDING))
                    .pt(px(PADDING))
                    .px(px(PADDING))
                    .pb(px(BOTTOM_PADDING))
                    .child(
                        div()
                            .debug_selector(|| "image-view-picture".into())
                            .relative()
                            .flex_shrink_0()
                            .w(px(w))
                            .h(px(h))
                            .border_1()
                            .border_color(colors.border.opacity(0.6))
                            .shadow_sm()
                            .child(checkerboard(colors.muted))
                            .child(img(path.clone()).size_full()),
                    )
                    .into_any_element()
            }
            Picture::Unavailable => div()
                .flex()
                .flex_col()
                .size_full()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .px(px(24.0))
                .text_center()
                .text_size(px(14.0))
                .text_color(colors.muted_foreground)
                .child(Icon::new("image-off").size(20.0).stroke(1.6))
                .child(cx.t("imageView.unavailable"))
                .into_any_element(),
            Picture::Loading(_) => div()
                .flex()
                .size_full()
                .items_center()
                .justify_center()
                .text_color(colors.muted_foreground)
                .child(spinner("image-view-loading", 16.0))
                .into_any_element(),
        };
        let bar = match &self.picture {
            Picture::Ready {
                width,
                height,
                bytes,
                ..
            } => {
                let hover = colors.muted.opacity(0.6);
                let foreground = colors.foreground;
                let percent: SharedString = format!("{}%", (scale * 100.0).round()).into();
                let details = format!("{width} × {height} · {}", format_bytes(*bytes));
                Some(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(px(12.0))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .debug_selector(|| "image-view-bar".into())
                                .flex()
                                .items_center()
                                .gap(px(2.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(colors.border)
                                .bg(colors.background.opacity(0.95))
                                .px(px(4.0))
                                .py(px(2.0))
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .shadow_sm()
                                .child(bar_button(
                                    "image-zoom-out",
                                    "minus",
                                    false,
                                    |view, cx| {
                                        let next = step_zoom(view.scale(), false);
                                        view.zoom_to(next, None, cx)
                                    },
                                    cx,
                                ))
                                .child(
                                    div()
                                        .id("image-zoom-actual")
                                        .min_w(px(48.0))
                                        .rounded(px(4.0))
                                        .px(px(6.0))
                                        .py(px(4.0))
                                        .text_center()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(hover).text_color(foreground))
                                        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                                            view.zoom_to(1.0, None, cx)
                                        }))
                                        .child(percent),
                                )
                                .child(bar_button(
                                    "image-zoom-in",
                                    "plus",
                                    false,
                                    |view, cx| {
                                        let next = step_zoom(view.scale(), true);
                                        view.zoom_to(next, None, cx)
                                    },
                                    cx,
                                ))
                                .child(bar_button(
                                    "image-zoom-fit",
                                    "maximize",
                                    self.zoom == Zoom::Fit,
                                    |view, cx| {
                                        view.zoom = Zoom::Fit;
                                        cx.notify()
                                    },
                                    cx,
                                ))
                                .child(
                                    div()
                                        .ml(px(4.0))
                                        .border_l_1()
                                        .border_color(colors.border)
                                        .pl(px(8.0))
                                        .pr(px(6.0))
                                        .child(details),
                                ),
                        ),
                )
            }
            _ => None,
        };
        let ready = matches!(self.picture, Picture::Ready { .. });
        let origin = bounds.origin;
        let local = move |position: Point<Pixels>| {
            (
                f32::from(position.x - origin.x),
                f32::from(position.y - origin.y),
            )
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(colors.background)
            .child(
                div()
                    .id("image-view")
                    .debug_selector(|| "image-view".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_scroll()
                    .track_scroll(&self.scroll)
                    .when(overflows, |scroller| {
                        scroller.cursor(if self.drag.is_some() {
                            gpui::CursorStyle::ClosedHand
                        } else {
                            gpui::CursorStyle::OpenHand
                        })
                    })
                    // Pinch and ⌘ + wheel zoom around the pointer.
                    .on_scroll_wheel(cx.listener(
                        move |view, event: &ScrollWheelEvent, window, cx| {
                            if !(event.modifiers.platform || event.modifiers.control) || !ready {
                                return;
                            }
                            let delta = f32::from(event.delta.pixel_delta(window.line_height()).y);
                            let factor = (delta.clamp(-50.0, 50.0) * 0.006).exp();
                            let next = view.scale() * factor;
                            view.zoom_to(next, Some(local(event.position)), cx);
                            cx.stop_propagation();
                        },
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                            if event.click_count == 2 && ready {
                                view.double_click(local(event.position), cx);
                                return;
                            }
                            if view.overflows() {
                                view.drag = Some((event.position, view.scroll.offset()));
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                        let Some((start, offset)) = view.drag else {
                            return;
                        };
                        view.scroll.set_offset(offset + (event.position - start));
                        cx.notify();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseUpEvent, _, cx| {
                            view.drag = None;
                            cx.notify();
                        }),
                    )
                    .child(content),
            )
            .children(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_told_apart_by_their_extension() {
        assert!(is_viewable_image("shots/home.PNG"));
        assert!(is_viewable_image("a.jpeg"));
        assert!(!is_viewable_image("logo.svg"));
    }

    #[test]
    fn the_fit_never_enlarges_and_steps_follow_the_presets() {
        assert_eq!(fit_zoom((100.0, 100.0), (800.0, 600.0)), 1.0);
        assert_eq!(fit_zoom((1600.0, 600.0), (800.0, 600.0)), 0.5);
        assert_eq!(fit_zoom((0.0, 10.0), (800.0, 600.0)), 1.0);
        assert_eq!(step_zoom(1.0, true), 1.25);
        assert_eq!(step_zoom(1.0, false), 0.75);
        assert_eq!(step_zoom(16.0, true), MAX_ZOOM);
        assert_eq!(step_zoom(0.6, false), 0.5);
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MB");
    }
}
