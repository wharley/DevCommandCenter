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
/// The bar and the margins around the screen.
const CHROME: f32 = 56.0 + 2.0 * MARGIN;
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
/// corners are painted over it: a ring in the panel's color whose inner edge
/// is the rounded screen (a border's inner radius is its outer radius less
/// its width, as in CSS), and a hairline on that edge.
fn corner_mask(radius: f32, background: gpui::Hsla, line: gpui::Hsla) -> AnyElement {
    // Reaching √2−1 of the radius past the screen covers its corners.
    let reach = (radius * 0.42).ceil() + 1.0;
    let ring = px(reach);
    let mut mask = div()
        .absolute()
        .top(-ring)
        .left(-ring)
        .right(-ring)
        .bottom(-ring)
        .rounded(px(radius + reach))
        .border_color(background);
    let width: gpui::AbsoluteLength = ring.into();
    mask.style().border_widths = gpui::EdgesRefinement {
        top: Some(width),
        right: Some(width),
        bottom: Some(width),
        left: Some(width),
    };
    div()
        .absolute()
        .size_full()
        .child(mask)
        .child(
            div()
                .absolute()
                .size_full()
                .rounded(px(radius))
                .border_1()
                .border_color(line),
        )
        .into_any_element()
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
                    if current.as_deref() == Some(sim.udid.as_str()) {
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
            .h(px(44.0))
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
        let room_width = WIDTH - 2.0 * MARGIN;
        let room_height = (f32::from(window.viewport_size().height) - CHROME - 48.0).max(200.0);
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
                    .size_full(),
            )
            .into_any_element()
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
                rows.push(
                    div()
                        .id(SharedString::from(format!("device-sim-{}", sim.udid)))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .rounded(px(6.0))
                        .px(px(8.0))
                        .py(px(6.0))
                        .cursor_pointer()
                        .hover(|style| style.bg(colors.accent.opacity(0.6)))
                        .on_click(
                            cx.listener(move |panel, _, _, cx| panel.open(chosen.clone(), cx)),
                        )
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
                                .child(if sim.booted {
                                    format!("{} · {}", sim.runtime, cx.t("device.running"))
                                } else {
                                    sim.runtime.clone()
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
        let body = match self.phase.clone() {
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
            Phase::Live => div()
                .flex()
                .justify_center()
                .pt(px(MARGIN))
                .child(self.screen(window, cx))
                .into_any_element(),
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
