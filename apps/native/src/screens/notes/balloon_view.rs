//! How a balloon looks (`note-balloon`): 340px (260 folded), the note's
//! fill and line, 24px corners and the long shadow. The header — the grip
//! with "IDEIA EM ABERTO" (or the title when folded), pin, fold, close —
//! then the project and the four colors, the title, the text, where it
//! came from, the saved context, how saving goes, and the actions.

use gpui::{
    AnyElement, BoxShadow, Context, FontWeight, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, Render, SharedString, Window, canvas, div, point, prelude::*, px, relative,
};

use super::balloon::{BalloonEvent, NoteBalloon};
use super::palette::note_colors;
use crate::data::notes::COLORS;
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Icon, Tracked, spinner};

type Click = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static>;

/// `.note-icon`: 26px, 9px corners, the accent on hover or when on; its
/// label as the tooltip (`title`).
#[allow(clippy::too_many_arguments)]
fn icon_action(
    id: String,
    icon: Icon,
    label: SharedString,
    active: bool,
    on_click: Option<Click>,
    accent: gpui::Hsla,
    muted: gpui::Hsla,
) -> AnyElement {
    let selector = id.clone();
    let button = div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || selector)
        .flex()
        .flex_none()
        .size(px(26.0))
        .items_center()
        .justify_center()
        .rounded(px(9.0))
        .text_color(muted)
        .when(active, |button| {
            button.bg(accent.opacity(0.12)).text_color(accent)
        })
        .map(|button| match on_click {
            None => button.opacity(0.4),
            Some(handler) => button
                .cursor_pointer()
                .hover(move |button| button.bg(accent.opacity(0.12)).text_color(accent))
                .on_click(handler),
        })
        .child(icon);
    crate::ui::Tooltip::text(SharedString::from(format!("{id}-tip")), button, label)
        .into_any_element()
}

impl Render for NoteBalloon {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(note) = self.note(cx) else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let dark = theme.appearance == Appearance::Dark;
        let palette = note_colors(&note.color, dark);
        let colors = &theme.colors;
        let (accent, muted) = (palette.accent, colors.muted_foreground);
        let mini = self.state.minimized;
        let busy = self.busy;
        let viewport = window.viewport_size();
        let title_text = if note.title.is_empty() {
            cx.t("notes.untitled").to_string()
        } else {
            note.title.clone()
        };
        let label = if mini {
            title_text.clone()
        } else {
            cx.t(if note.status == "completed" {
                "notes.realizedIdea"
            } else {
                "notes.idea"
            })
            .to_string()
        };
        let id = note.id.clone();
        let handle = div()
            .id("note-handle")
            .debug_selector({
                let id = id.clone();
                move || format!("note-handle-{id}")
            })
            .track_focus(&self.handle)
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(7.0))
            .py(px(3.0))
            .cursor_grab()
            .text_size(px(11.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(accent)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|balloon, event: &MouseDownEvent, window, cx| {
                    window.focus(&balloon.handle, cx);
                    cx.emit(BalloonEvent::Grab(event.position));
                }),
            )
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                let step = if event.keystroke.modifiers.shift {
                    40.0
                } else {
                    12.0
                };
                let (dx, dy) = match event.keystroke.key.as_str() {
                    "left" => (-step, 0.0),
                    "right" => (step, 0.0),
                    "up" => (0.0, -step),
                    "down" => (0.0, step),
                    _ => return,
                };
                cx.stop_propagation();
                cx.emit(BalloonEvent::Nudge(dx, dy));
            }))
            .child(
                div()
                    .flex_none()
                    .opacity(0.7)
                    .child(Icon::new("grip").size(15.0)),
            )
            .child(div().min_w_0().truncate().child(label));
        let header = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .map(|header| {
                if mini {
                    header.p(px(10.0))
                } else {
                    header.pt(px(10.0)).px(px(12.0)).pb(px(6.0))
                }
            })
            .child(handle)
            .child(icon_action(
                format!("note-pin-{id}"),
                Icon::new("pin").size(13.0),
                cx.t(if note.pinned {
                    "notes.unpin"
                } else {
                    "notes.pin"
                }),
                note.pinned,
                Some(Box::new(
                    cx.listener(|balloon, _, _, cx| balloon.toggle_pin(cx)),
                )),
                accent,
                muted,
            ))
            .child(icon_action(
                format!("note-fold-{id}"),
                if mini {
                    Icon::new("arrow-up-right").size(14.0)
                } else {
                    Icon::new("minus").size(14.0)
                },
                cx.t(if mini {
                    "notes.expand"
                } else {
                    "notes.minimize"
                }),
                false,
                Some(Box::new(
                    cx.listener(|balloon, _, _, cx| balloon.toggle_minimized(cx)),
                )),
                accent,
                muted,
            ))
            .child(icon_action(
                format!("note-close-{id}"),
                Icon::new("x").size(14.0),
                cx.t("notes.close"),
                false,
                (!busy).then(|| -> Click {
                    Box::new(cx.listener(|balloon, _, _, cx| balloon.close(cx)))
                }),
                accent,
                muted,
            ));
        let body = (!mini).then(|| self.body(&note, viewport.height.into(), cx));
        let record = self.size.clone();
        div()
            .id(SharedString::from(format!("note-balloon-{id}")))
            .debug_selector({
                let id = id.clone();
                move || format!("note-balloon-{id}")
            })
            .relative()
            .flex()
            .flex_col()
            .w(px(
                if mini { 260.0_f32 } else { 340.0 }.min(f32::from(viewport.width) - 16.0)
            ))
            .max_h(px(f32::from(viewport.height) - 56.0))
            .text_color(colors.foreground)
            .bg(palette.fill)
            .border_1()
            .border_color(palette.line)
            .rounded(px(if mini { 22.0 } else { 24.0 }))
            .shadow(vec![
                BoxShadow {
                    color: gpui::black().opacity(0x55 as f32 / 255.0),
                    offset: point(px(0.0), px(24.0)),
                    blur_radius: px(70.0),
                    spread_radius: px(-22.0),
                    inset: false,
                },
                BoxShadow {
                    color: gpui::black().opacity(0x22 as f32 / 255.0),
                    offset: point(px(0.0), px(4.0)),
                    blur_radius: px(14.0),
                    spread_radius: px(-5.0),
                    inset: false,
                },
            ])
            .occlude()
            .child(crate::ui::occlusion::marker())
            .child(
                canvas(
                    move |bounds, _, _| {
                        record.set(Some((
                            f32::from(bounds.size.width),
                            f32::from(bounds.size.height),
                        )))
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(header)
            .children(body)
            .into_any_element()
    }
}

impl NoteBalloon {
    fn body(
        &self,
        note: &crate::data::notes::ProjectNote,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let dark = theme.appearance == Appearance::Dark;
        let palette = note_colors(&note.color, dark);
        let colors = &theme.colors;
        let (accent, muted) = (palette.accent, colors.muted_foreground);
        let store = self.store.read(cx);
        let (saving, failed) = (
            store.saving.contains(&note.id),
            store.failed.contains(&note.id),
        );
        let busy = self.busy;
        let id = note.id.clone();
        let swatches: Vec<AnyElement> = COLORS
            .iter()
            .map(|color| {
                let swatch = note_colors(color, dark);
                let chosen = note.color == *color;
                let color: &'static str = color;
                div()
                    .id(SharedString::from(format!("note-color-{color}")))
                    .debug_selector(move || format!("note-color-{color}"))
                    .flex()
                    .size(px(19.0))
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(if chosen {
                        swatch.accent
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |balloon, _, _, cx| balloon.set_color(color, cx)))
                    .child(
                        div()
                            .flex()
                            .size(px(15.0))
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(swatch.accent)
                            .text_color(swatch.fill)
                            .when(chosen, |dot| dot.child(Icon::new("check").size(10.0))),
                    )
                    .into_any_element()
            })
            .collect();
        let status: AnyElement = if failed {
            div()
                .id("note-retry")
                .debug_selector(|| "note-retry".into())
                .text_color(colors.destructive)
                .underline()
                .cursor_pointer()
                .on_click(cx.listener(|balloon, _, _, cx| balloon.retry(cx)))
                .child(cx.t("notes.retrySave"))
                .into_any_element()
        } else if saving {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(spinner("note-saving", 11.0))
                .child(cx.t("notes.saving"))
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(Icon::new("check-check").size(12.0))
                .child(cx.t("notes.saved"))
                .into_any_element()
        };
        let empty = note.title.trim().is_empty() && note.content.trim().is_empty();
        let done = note.status == "completed";
        let use_off = self.use_reason.is_some() || busy;
        div()
            .id("note-body")
            .pt(px(8.0))
            .px(px(18.0))
            .pb(px(15.0))
            .max_h(px(viewport_height - 104.0))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(px(10.0))
                    .text_color(muted)
                    .child(Icon::new("folder-open").size(12.0))
                    .child(
                        div()
                            .max_w(px(170.0))
                            .truncate()
                            .child(note.project_name.clone()),
                    )
                    .child(div().ml_auto().flex().gap(px(2.0)).children(swatches)),
            )
            .child(
                div()
                    .debug_selector(|| "note-title".into())
                    .mt(px(17.0))
                    .mb(px(9.0))
                    .text_size(px(19.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .debug_selector(|| "note-content".into())
                    .min_h(px(150.0))
                    .max_h(px(viewport_height * 0.36))
                    .text_size(px(13.0))
                    .line_height(relative(1.8))
                    .child(self.content.clone()),
            )
            .when(!note.source_task_title.is_empty(), |body| {
                body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .pt(px(12.0))
                        .text_size(px(10.0))
                        .text_color(muted)
                        .child(Icon::new("message-circle").size(12.0))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(note.source_task_title.clone()),
                        )
                        .when(note.source_workspace_id.is_none(), |row| {
                            row.child(
                                div()
                                    .flex_none()
                                    .opacity(0.65)
                                    .child(cx.t("notes.sourceDeleted")),
                            )
                        }),
                )
            })
            .when(!note.context_snapshot.is_empty(), |body| {
                let open = self.context_open;
                body.child(
                    div()
                        .mt(px(10.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(palette.line)
                        .bg(accent.opacity(0.06))
                        .child(
                            div()
                                .id("note-context-toggle")
                                .debug_selector(|| "note-context-toggle".into())
                                .flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .px(px(9.0))
                                .py(px(7.0))
                                .text_size(px(10.0))
                                .text_color(accent)
                                .cursor_pointer()
                                .on_click(cx.listener(|balloon, _, _, cx| {
                                    balloon.context_open = !balloon.context_open;
                                    cx.notify();
                                }))
                                .child(cx.t("notes.savedContext"))
                                .child(
                                    Icon::new(if open { "chevron-up" } else { "chevron-down" })
                                        .size(13.0),
                                ),
                        )
                        .when(open, |context| {
                            context.child(
                                div()
                                    .id("note-context")
                                    .debug_selector(|| "note-context".into())
                                    .px(px(9.0))
                                    .pb(px(9.0))
                                    .max_h(px(150.0))
                                    .overflow_y_scroll()
                                    .text_size(px(11.0))
                                    .line_height(relative(1.6))
                                    .child(note.context_snapshot.clone()),
                            )
                        }),
                )
            })
            .child(
                div()
                    .min_h(px(30.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(4.0))
                    .text_size(px(9.0))
                    .text_color(muted)
                    .child(status),
            )
            .children(self.use_reason.clone().map(|reason| {
                div()
                    .mb(px(10.0))
                    .text_size(px(11.0))
                    .line_height(relative(1.5))
                    .text_color(muted)
                    .child(reason)
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .border_t_1()
                    .border_color(palette.line)
                    .pt(px(11.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("note-task-{id}")))
                            .debug_selector(|| "note-task".into())
                            .mr_auto()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(5.0))
                            .rounded(px(10.0))
                            .bg(accent.opacity(0.14))
                            .text_color(accent)
                            .px(px(12.0))
                            .py(px(8.0))
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .map(|button| {
                                if busy || empty {
                                    button.opacity(0.45)
                                } else {
                                    button
                                        .cursor_pointer()
                                        .hover(move |button| button.bg(accent.opacity(0.24)))
                                        .on_click(
                                            cx.listener(|balloon, _, _, cx| {
                                                balloon.create_task(cx)
                                            }),
                                        )
                                }
                            })
                            .child(Icon::new("plus").size(14.0))
                            .child(Tracked::new(cx.t("notes.createTask"), 11.0, 0.0)),
                    )
                    .child(icon_action(
                        format!("note-use-{id}"),
                        Icon::new("arrow-down-left").size(16.0),
                        cx.t("notes.useHere"),
                        false,
                        (!use_off).then(|| -> Click {
                            Box::new(cx.listener(|balloon, _, _, cx| balloon.use_here(cx)))
                        }),
                        accent,
                        muted,
                    ))
                    .child(icon_action(
                        format!("note-done-{id}"),
                        if done {
                            Icon::new("rotate-ccw").size(15.0)
                        } else {
                            Icon::new("check").size(16.0)
                        },
                        cx.t(if done {
                            "notes.reopen"
                        } else {
                            "notes.complete"
                        }),
                        false,
                        (!busy).then(|| -> Click {
                            Box::new(cx.listener(|balloon, _, _, cx| balloon.toggle_done(cx)))
                        }),
                        accent,
                        muted,
                    ))
                    .child(icon_action(
                        format!("note-delete-{id}"),
                        Icon::new("trash-2").size(14.0),
                        cx.t("notes.delete"),
                        false,
                        (!busy).then(|| -> Click {
                            Box::new(cx.listener(|_, _, _, cx| cx.emit(BalloonEvent::Delete)))
                        }),
                        accent,
                        muted,
                    )),
            )
            .into_any_element()
    }
}
