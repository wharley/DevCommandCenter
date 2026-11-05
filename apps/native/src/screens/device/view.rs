//! The panel: a bar with the simulator (a menu of the others), Home, the
//! app switcher, lock, power and close; below it the screen in a rounded
//! frame at its own proportions, as large as the panel allows. The list of
//! simulators while none is open; a spinner while one starts.

use gpui::{
    AnyElement, Context, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ScrollWheelEvent, SharedString, Window, canvas, div, prelude::*, px, surface,
};

use super::{DevicePanel, Phase};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::menu::{Menu, MenuAlign, MenuEntry, MenuItem};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip};

/// The panel's width beside the conversation.
pub const WIDTH: f32 = 400.0;
/// Room around the screen; the corner mask reaches into it.
const MARGIN: f32 = 24.0;
/// An iPhone's display corners: ~55pt on a 402pt wide screen.
const CORNER: f32 = 0.135;

fn icon_button(id: &'static str, icon: &'static str) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconSm)
        .muted()
        .child(Icon::new(icon).size(15.0))
}

/// GPUI clips a video surface only to a rectangle, so the display's rounded
/// corners are painted over it: in each corner, the bit outside the arc in
/// the panel's color, and a hairline along the rounded edge.
fn corner_mask(radius: f32, background: gpui::Hsla, line: gpui::Hsla) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    for path in corner_paths(bounds, px(radius)) {
                        window.paint_path(path, background);
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded(px(radius))
                .border_1()
                .border_color(line),
        )
        .into_any_element()
}

/// The four pieces between a rectangle's corners and its rounded edge (a
/// quarter circle as one cubic Bézier, k = 0.5523).
fn corner_paths(
    bounds: gpui::Bounds<gpui::Pixels>,
    radius: gpui::Pixels,
) -> Vec<gpui::Path<gpui::Pixels>> {
    let k = radius * 0.5523;
    let (left, top) = (bounds.origin.x, bounds.origin.y);
    let (right, bottom) = (left + bounds.size.width, top + bounds.size.height);
    // (corner, start of the arc on the horizontal edge, end on the vertical,
    // and which way each runs from the corner).
    [
        (gpui::point(left, top), 1.0, 1.0),
        (gpui::point(right, top), -1.0, 1.0),
        (gpui::point(right, bottom), -1.0, -1.0),
        (gpui::point(left, bottom), 1.0, -1.0),
    ]
    .into_iter()
    .filter_map(|(corner, dx, dy)| {
        let along = |x: f32, y: f32| gpui::point(corner.x + radius * x, corner.y + radius * y);
        let mut path = gpui::PathBuilder::fill();
        path.move_to(corner);
        path.line_to(along(dx, 0.0));
        path.cubic_bezier_to(
            along(0.0, dy),
            gpui::point(corner.x + (radius - k) * dx, corner.y),
            gpui::point(corner.x, corner.y + (radius - k) * dy),
        );
        path.close();
        path.build().ok()
    })
    .collect()
}

impl DevicePanel {
    fn bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let title: SharedString = match &self.current {
            Some(sim) => format!("{} · {}", sim.name, sim.runtime).into(),
            None => cx.t("device.choose"),
        };
        let panel = cx.entity().downgrade();
        let simulators = self.simulators.clone();
        let current = self.current.as_ref().map(|sim| sim.udid.clone());
        let taken = self.taken_labels(cx);
        let muted = colors.muted_foreground;
        let trigger = move |_open: bool| {
            Button::new("device-current")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .min_w_0()
                        .child(Icon::new("smartphone").size(14.0))
                        .child(div().truncate().child(title))
                        .child(Icon::new("chevron-down").size(12.0).color(muted)),
                )
                .into_any_element()
        };
        let menu = Menu::dropdown("device-menu", trigger, move |_, _| {
            simulators
                .iter()
                .map(|sim| {
                    let panel = panel.clone();
                    let chosen = sim.clone();
                    let mut item = MenuItem::new(sim.name.clone(), move |_, cx| {
                        let chosen = chosen.clone();
                        let _ = panel.update(cx, |panel, cx| panel.open(chosen, cx));
                    })
                    .detail(sim.runtime.clone());
                    if let Some(label) = taken.get(&sim.udid) {
                        item = item
                            .detail(format!("{} · {label}", sim.runtime))
                            .disabled(true);
                    } else if current.as_deref() == Some(sim.udid.as_str()) {
                        item = item.trailing("✓");
                    } else if sim.booted {
                        item = item.trailing("●");
                    }
                    MenuEntry::from(item)
                })
                .collect()
        })
        .align(MenuAlign::Start)
        .min_width(240.0);
        let live = self.phase == Phase::Live;
        let control = |id: &'static str,
                       icon: &'static str,
                       key: &str,
                       button: &'static str,
                       cx: &mut Context<Self>| {
            let label = cx.t(key);
            Tooltip::text(
                id,
                icon_button(id, icon)
                    .disabled(!live)
                    .on_click(cx.listener(move |panel, _, _, _| panel.button(button))),
                label,
            )
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(4.0))
            .debug_selector(|| "device-bar".into())
            .h(px(crate::screens::conversation::header::HEIGHT))
            .px(px(8.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .child(div().flex_1().min_w_0().child(menu))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(control("device-home", "house", "device.home", "home", cx))
                    .child(control(
                        "device-switcher",
                        "layers",
                        "device.switcher",
                        "app_switcher",
                        cx,
                    ))
                    .child(control(
                        "device-lock",
                        "lock-keyhole",
                        "device.lock",
                        "lock",
                        cx,
                    ))
                    .child(Tooltip::text(
                        "device-power",
                        icon_button("device-power", "power")
                            .disabled(self.current.is_none())
                            .on_click(cx.listener(|panel, _, _, cx| panel.power_off(cx))),
                        cx.t("device.powerOff"),
                    ))
                    .child(Tooltip::text(
                        "device-close",
                        icon_button("device-close", "x")
                            .on_click(cx.listener(|panel, _, _, cx| panel.close(cx))),
                        cx.t("device.close"),
                    )),
            )
            .into_any_element()
    }

    fn screen(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (width, height) = match self.size {
            (0, _) | (_, 0) => (1179.0, 2556.0),
            (width, height) => (width as f32, height as f32),
        };
        // The room measured at the last layout; before it, the window's.
        let room = self.room.get();
        let room_height = if room.height > px(0.0) {
            f32::from(room.height)
        } else {
            f32::from(window.viewport_size().height) - 140.0
        };
        let room_width = WIDTH - 2.0 * MARGIN;
        let room_height = (room_height - 2.0 * MARGIN).max(160.0);
        let scale = (room_width / width).min(room_height / height);
        let (shown_width, shown_height) = (width * scale, height * scale);
        let bounds = self.screen.clone();
        let picture = self.picture.clone();
        let radius = shown_width * CORNER;
        div()
            .id("device-screen")
            .debug_selector(|| "device-screen".into())
            .relative()
            .w(px(shown_width))
            .h(px(shown_height))
            .bg(gpui::black())
            .cursor_pointer()
            .track_focus(&self.focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|panel, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    panel.touch("begin", event.position, window, cx);
                }),
            )
            .on_mouse_move(cx.listener(|panel, event: &MouseMoveEvent, window, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    panel.touch("move", event.position, window, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|panel, event: &MouseUpEvent, window, cx| {
                    panel.touch("end", event.position, window, cx)
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|panel, event: &MouseUpEvent, window, cx| {
                    panel.touch("end", event.position, window, cx)
                }),
            )
            .on_scroll_wheel(cx.listener(|panel, event: &ScrollWheelEvent, window, _| {
                panel.scroll(
                    event.delta.pixel_delta(window.line_height()),
                    event.position,
                );
            }))
            .on_key_down(cx.listener(|panel, event: &gpui::KeyDownEvent, _, cx| {
                if panel.key(&event.keystroke) {
                    cx.stop_propagation();
                }
            }))
            .children(picture.map(|picture| surface(picture).size_full()))
            .child(corner_mask(radius, colors.background, colors.border))
            .child(
                canvas(move |painted, _, _| bounds.set(painted), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .into_any_element()
    }

    /// "em uso por <tarefa>" for each simulator another task holds.
    pub(crate) fn taken_labels(
        &self,
        cx: &Context<Self>,
    ) -> std::collections::HashMap<String, String> {
        let task = self.task.as_ref().map(|task| task.task_id.as_str());
        self.simulators
            .iter()
            .filter_map(|sim| {
                let holder = self.owners.taken(&sim.udid, task)?;
                let label = cx.t_with(
                    "device.inUseBy",
                    &crate::i18n::Args::default().with("task", holder.title.clone()),
                );
                Some((sim.udid.clone(), label.to_string()))
            })
            .collect()
    }

    fn list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        if let Some(error) = &self.list_error {
            return div()
                .p(px(16.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("device.noXcode"))
                .child(div().mt(px(4.0)).text_size(px(11.0)).child(error.clone()))
                .into_any_element();
        }
        let [recent, iphones, ipads] = crate::data::device::sections(&self.simulators);
        let sections = [
            ("device.recent", recent),
            ("device.iphones", iphones),
            ("device.ipads", ipads),
        ];
        let taken = self.taken_labels(cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        for (label, sims) in sections {
            if sims.is_empty() {
                continue;
            }
            rows.push(
                div()
                    .px(px(8.0))
                    .pt(px(if rows.is_empty() { 6.0 } else { 14.0 }))
                    .pb(px(4.0))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground)
                    .child(cx.t(label))
                    .into_any_element(),
            );
            for sim in sims {
                let chosen = sim.clone();
                let held = taken.get(&sim.udid).cloned();
                let free = held.is_none();
                let id = SharedString::from(format!("device-sim-{}", sim.udid));
                rows.push(
                    div()
                        .id(id.clone())
                        .debug_selector(move || id.to_string())
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .rounded(px(6.0))
                        .px(px(8.0))
                        .py(px(6.0))
                        .when(free, |row| {
                            row.cursor_pointer()
                                .hover(|style| style.bg(colors.accent.opacity(0.6)))
                                .on_click(cx.listener(move |panel, _, _, cx| {
                                    panel.open(chosen.clone(), cx)
                                }))
                        })
                        .when(!free, |row| row.opacity(0.55))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .min_w_0()
                                .text_size(px(13.0))
                                .child(
                                    Icon::new(if sim.ipad { "tablet" } else { "smartphone" })
                                        .size(14.0),
                                )
                                .child(div().truncate().child(sim.name.clone())),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child(match (&held, sim.booted) {
                                    (Some(label), _) => format!("{} · {label}", sim.runtime),
                                    (None, true) => {
                                        format!("{} · {}", sim.runtime, cx.t("device.running"))
                                    }
                                    (None, false) => sim.runtime.clone(),
                                }),
                        )
                        .into_any_element(),
                );
            }
        }
        div()
            .id("device-list")
            .debug_selector(|| "device-list".into())
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(8.0))
            .children(rows)
            .into_any_element()
    }

    fn message(&self, text: SharedString, busy: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.0))
            .p(px(24.0))
            .text_size(px(12.0))
            .text_color(colors.muted_foreground)
            .when(busy, |column| {
                column.child(crate::ui::spinner("device-busy", 18.0))
            })
            .child(div().text_center().child(text))
            .into_any_element()
    }
}

impl Render for DevicePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        // Linked but no picture yet (a second or two): still starting, not
        // an empty black screen.
        let phase = match self.phase.clone() {
            Phase::Live if self.picture.is_none() => Phase::Starting,
            phase => phase,
        };
        let body = match phase {
            Phase::Choosing => self.list(cx),
            Phase::Starting => {
                let name = self
                    .current
                    .as_ref()
                    .map(|sim| sim.name.clone())
                    .unwrap_or_default();
                let text = cx.t_with("device.starting", &Args::default().with("name", name));
                self.message(text, true, cx)
            }
            Phase::Live => {
                let room = self.room.clone();
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .justify_center()
                    .pt(px(MARGIN))
                    .child(
                        canvas(move |bounds, _, _| room.set(bounds.size), |_, _, _, _| {})
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                    )
                    .child(self.screen(window, cx))
                    .into_any_element()
            }
            Phase::Failed(reason) => {
                let retry = self.current.clone();
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(self.message(reason.into(), false, cx))
                    .when_some(retry, |column, sim| {
                        column.child(
                            Button::new("device-retry")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .on_click(
                                    cx.listener(move |panel, _, _, cx| panel.open(sim.clone(), cx)),
                                )
                                .child(cx.t("device.retry")),
                        )
                    })
                    .into_any_element()
            }
        };
        div()
            .debug_selector(|| "device-panel".into())
            .flex()
            .flex_col()
            .w(px(WIDTH))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background)
            .child(self.bar(cx))
            .child(div().flex_1().min_h_0().child(body))
    }
}
