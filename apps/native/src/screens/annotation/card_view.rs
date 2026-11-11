//! How the card looks (`code-annotation.css`): 440px, the surface edge and
//! the dialog shadow; the mark, title and path; the lines (and "removido"
//! for old code); the code (98px at most, the first 6000 characters); the
//! instruction field; then one footer row: "Editar no composer" and
//! "Adicionar como evidência" (its icon) on the left, "Nova sessão" and
//! "Enviar ⌘↵" on the right. Over a faint backdrop that closes it; placed
//! over its trigger, under it when that doesn't fit, always whole inside the
//! window (only its body scrolls).

use dcc_core::domain::code_annotation::{AnnotationSide, line_label};
use gpui::{
    BoxShadow, Context, Focusable, FontWeight, InteractiveElement, MouseButton, Pixels, Point,
    Render, SharedString, Size, Window, anchored, canvas, deferred, div, point, prelude::*, px,
    size,
};

use super::card::{AnnotationCard, CONTEXT, CloseAnnotation, SendAnnotation};
use crate::i18n::Translate;
use crate::theme::color::mix_lab;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, Tracked};

const WIDTH: f32 = 440.0;
const MARGIN: f32 = 12.0;
const GAP: f32 = 8.0;
/// The send shortcut, faint inside "Enviar".
const SEND_KEYS: &str = if cfg!(target_os = "macos") {
    "⌘↵"
} else {
    "Ctrl↵"
};
/// `request.snippet.slice(0, 6000)`
const PREVIEW_LIMIT: usize = 6000;

/// Where the card goes (`measure` in `DiffAnnotationPopover`): its left at
/// the trigger's, over it with an 8px gap, else 24px under its bottom;
/// then inside the window, 12px from each edge.
pub fn place(anchor: Point<Pixels>, card: Size<Pixels>, viewport: Size<Pixels>) -> Point<Pixels> {
    let (width, height) = (f32::from(card.width), f32::from(card.height));
    let (view_w, view_h) = (f32::from(viewport.width), f32::from(viewport.height));
    let left = f32::from(anchor.x).min(view_w - width - MARGIN).max(MARGIN);
    let above = f32::from(anchor.y) - GAP - height;
    let below = f32::from(anchor.y) + 24.0;
    let top = if above >= MARGIN { above } else { below };
    let top = top.min(view_h - height - MARGIN).max(MARGIN);
    point(px(left), px(top))
}

fn chip(text: SharedString, foreground: gpui::Hsla, background: gpui::Hsla) -> gpui::Div {
    div()
        .rounded(px(6.0))
        .px(px(6.0))
        .py(px(3.0))
        .bg(background)
        .text_size(px(10.0))
        .line_height(gpui::relative(1.5))
        .text_color(foreground)
        .child(text)
}

impl Render for AnnotationCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let viewport = window.viewport_size();
        let request = &self.request;
        let annotation = &request.annotation;
        let edge = colors.border.opacity(0.8);
        let calm = colors.accent_calm;
        let can_submit = self.can_submit(cx);
        let measured = self.size.get();
        let width = WIDTH.min(f32::from(viewport.width) - 24.0);
        let at = measured.map(|card| place(request.anchor, card, viewport));
        let record = self.size.clone();

        let header = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.0))
            .pt(px(15.0))
            .px(px(16.0))
            .pb(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .size(px(32.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(11.0))
                    .bg(calm.opacity(0.12))
                    .text_color(calm)
                    .child(Icon::new("code-xml").size(16.0)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(px(19.5))
                            .child(Tracked::new(cx.t("diffAnnotate.dialogLabel"), 13.0, -0.015)),
                    )
                    .child(
                        div()
                            .debug_selector(|| "annotation-path".into())
                            .font_family(fonts::MONO)
                            .text_size(px(10.0))
                            .line_height(gpui::relative(1.5))
                            .text_color(colors.muted_foreground)
                            .child(SharedString::from(annotation.path.clone())),
                    ),
            )
            .child(
                Button::new("annotation-close")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .on_click(cx.listener(|card, _, _, cx| card.cancel(cx)))
                    .child(Icon::new("x").size(14.0)),
            );

        let lines = line_label(annotation.start_line, annotation.end_line);
        let context = div()
            .flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .gap(px(6.0))
            .px(px(16.0))
            .pb(px(10.0))
            .child(
                chip(lines.into(), colors.muted_foreground, colors.muted)
                    .font_family(fonts::MONO)
                    .font_features(crate::ui::tabular_nums()),
            )
            .children((annotation.side == AnnotationSide::Original).then(|| {
                chip(
                    cx.t("diffAnnotate.deletedSide"),
                    colors.destructive,
                    colors.destructive.opacity(0.09),
                )
            }));

        let snippet = (!annotation.snippet.is_empty()).then(|| {
            let shown: String = annotation
                .snippet
                .chars()
                .take(PREVIEW_LIMIT)
                .collect::<String>()
                .replace('\t', "  ");
            let limited = annotation.snippet.chars().count() > PREVIEW_LIMIT;
            div()
                .mb(px(12.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(edge)
                .overflow_hidden()
                .bg(mix_lab(colors.muted, 0.45, colors.background))
                .child(
                    div()
                        .id("annotation-snippet")
                        .debug_selector(|| "annotation-snippet".into())
                        .max_h(px(98.0))
                        .overflow_scroll()
                        .px(px(12.0))
                        .py(px(10.0))
                        .font_family(fonts::MONO)
                        .text_size(px(11.0))
                        .line_height(gpui::relative(1.6))
                        .whitespace_nowrap()
                        .children(
                            shown
                                .split('\n')
                                .map(|line| div().child(SharedString::from(line.to_string())))
                                .collect::<Vec<_>>(),
                        ),
                )
                .children(limited.then(|| {
                    div()
                        .px(px(10.0))
                        .py(px(4.0))
                        .border_t_1()
                        .border_color(edge)
                        .text_size(px(9.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("diffAnnotate.previewLimited"))
                }))
        });

        let focused = self
            .instruction
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let field = div()
            .debug_selector(|| "annotation-instruction".into())
            .w_full()
            .min_h(px(88.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(if focused { calm } else { edge })
            .bg(colors.background)
            .when(focused, |field| {
                field.shadow(vec![BoxShadow {
                    color: calm.opacity(0.10),
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(3.0),
                    inset: false,
                }])
            })
            .px(px(10.0))
            .py(px(8.0))
            .text_size(px(12.0))
            .line_height(gpui::relative(1.6))
            .child(self.instruction.clone());

        // One row: the quiet ways out on the left (they give way first),
        // "Nova sessão" and the filled "Enviar ⌘↵" always whole on the right.
        let evidence = Tooltip::text(
            "tip-annotation-add-to-review",
            Button::new("annotation-add-to-review")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .muted()
                .disabled(!can_submit)
                .on_click(cx.listener(|card, _, _, cx| card.add_to_review(cx)))
                .child(Icon::new("list-plus").size(14.0)),
            cx.t("diffAnnotate.addToReview"),
        );
        let footer = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(edge)
            .bg(colors.muted.opacity(0.3))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        Button::new("annotation-edit-in-composer")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Xs)
                            .height(28.0)
                            .padding_x(8.0)
                            .text_size(12.0)
                            .muted()
                            .on_click(cx.listener(|card, _, _, cx| card.edit_in_composer(cx)))
                            .child(Icon::new("square-pen").size(12.0))
                            .child(cx.t("diffAnnotate.editInComposer")),
                    )
                    .child(evidence),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Button::new("annotation-new-session")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .height(28.0)
                            .padding_x(10.0)
                            .text_size(12.0)
                            .disabled(!can_submit)
                            .on_click(cx.listener(|card, _, _, cx| card.submit(true, cx)))
                            .child(cx.t("diffAnnotate.newSession")),
                    )
                    .child(
                        Button::new("annotation-send")
                            .variant(ButtonVariant::Default)
                            .size(ButtonSize::Xs)
                            .height(28.0)
                            .padding_x(10.0)
                            .text_size(12.0)
                            .disabled(!can_submit)
                            .on_click(cx.listener(|card, _, _, cx| card.submit(false, cx)))
                            .child(Icon::new("send").size(12.0))
                            .child(cx.t("diffAnnotate.send"))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors.primary_foreground.opacity(0.6))
                                    .child(SEND_KEYS),
                            ),
                    ),
            );

        let card = div()
            .id("annotation-card")
            .debug_selector(|| "annotation-card".into())
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|card, _: &SendAnnotation, _, cx| card.submit(false, cx)))
            .on_action(cx.listener(|card, _: &CloseAnnotation, _, cx| card.cancel(cx)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .occlude()
            .relative()
            .when(at.is_none(), |card| card.invisible())
            .flex()
            .flex_col()
            .w(px(width))
            .max_h(viewport.height - px(24.0))
            .overflow_hidden()
            .rounded(px(18.0))
            .border_1()
            .border_color(edge)
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .shadow(vec![
                BoxShadow {
                    color: gpui::black().opacity(0.16),
                    offset: point(px(0.0), px(24.0)),
                    blur_radius: px(80.0),
                    spread_radius: px(0.0),
                    inset: false,
                },
                BoxShadow {
                    color: gpui::black().opacity(0.06),
                    offset: point(px(0.0), px(4.0)),
                    blur_radius: px(14.0),
                    spread_radius: px(0.0),
                    inset: false,
                },
            ])
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let next = size(bounds.size.width, bounds.size.height);
                        if record.get() != Some(next) {
                            record.set(Some(next));
                            // A refresh while drawing is dropped: ask for the
                            // next frame once this one is done, so the card is
                            // placed by its real size (and shows).
                            window.defer(cx, |window, _| window.refresh());
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(header)
            .child(context)
            .child(
                div()
                    .id("annotation-body")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(16.0))
                    .pb(px(12.0))
                    .children(snippet)
                    .child(field),
            )
            .child(footer);

        div()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("annotation-backdrop")
                    .debug_selector(|| "annotation-backdrop".into())
                    .absolute()
                    .inset_0()
                    .bg(gpui::black().opacity(0.05))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|card, _, _, cx| card.cancel(cx)),
                    ),
            )
            // In window coordinates over everything, so no column clips it,
            // and snapped back inside the window should it ever leave it.
            .child(
                deferred(
                    anchored()
                        .position(at.unwrap_or(request.anchor))
                        .snap_to_window_with_margin(px(MARGIN))
                        .child(card),
                )
                .priority(crate::ui::layer::POPUP),
            )
    }
}

/// The card for a trigger at `anchor` in a `viewport`, for the tests.
#[cfg(test)]
pub(crate) fn placed(anchor: (f32, f32), card: (f32, f32), viewport: (f32, f32)) -> (f32, f32) {
    let at = place(
        point(px(anchor.0), px(anchor.1)),
        size(px(card.0), px(card.1)),
        size(px(viewport.0), px(viewport.1)),
    );
    (f32::from(at.x), f32::from(at.y))
}
