//! The Browser as the desktop draws it: the 44px header (the address bar
//! with the reload inside it, "Sessões", send to the agent, the agent's
//! control or the activity, close), the 28px strip (an error, the
//! evidence capture, loading, or the page's title and the control's mark) and the white viewport the page is laid over.

use gpui::{
    AnyElement, App, Bounds, Context, Element, ElementId, Entity, FontWeight, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, MouseDownEvent, Pixels, Render, Style, Window, div,
    prelude::*, px,
};

use super::surface::BrowserSurface;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner};

/// Where the page goes: it follows this element's bounds, and hides while
/// an overlay covers it or the splitter is dragged.
struct Viewport {
    surface: Entity<BrowserSurface>,
}

impl IntoElement for Viewport {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Viewport {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: gpui::Size::full(),
            flex_shrink: 1.0,
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        cx: &mut App,
    ) {
        let covered = crate::ui::occlusion::page_covered(bounds, cx);
        self.surface.update(cx, |surface, _| {
            let visible = !covered && !surface.force_hidden;
            if visible && let Some(page) = &surface.page {
                page.set_bounds(bounds);
            }
            surface.show_page(visible);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        // A click elsewhere gives the keyboard back to the app.
        let surface = self.surface.downgrade();
        window.on_mouse_event(move |event: &MouseDownEvent, _, _, cx| {
            if !bounds.contains(&event.position)
                && let Some(surface) = surface.upgrade()
                && let Some(page) = &surface.read(cx).page
            {
                page.blur();
            }
        });
    }
}

impl BrowserSurface {
    fn address_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let reload_label = cx.t("browser.reload");
        let reload = Button::new("browser-reload")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconXs)
            .muted()
            .disabled(self.loading || self.page.is_none())
            .on_click(cx.listener(|surface, _, _, cx| surface.reload(cx)))
            .child(if self.loading {
                spinner("browser-reload-busy", 14.0).into_any_element()
            } else {
                Icon::new("refresh-cw").size(14.0).into_any_element()
            });
        div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .child(Icon::new("globe-2").size(14.0).color(tw::CYAN_400.hsla())),
            )
            .child(
                div()
                    .debug_selector(|| "browser-address".into())
                    .flex()
                    .h(px(28.0))
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.7))
                    .bg(colors.muted.opacity(0.25))
                    .pl(px(32.0))
                    .pr(px(32.0))
                    .text_size(px(12.0))
                    .text_color(colors.foreground)
                    .child(self.address.clone()),
            )
            .child(div().absolute().right(px(2.0)).child(Tooltip::new(
                "browser-reload-tip",
                reload,
                move |_, _| div().child(reload_label.clone()).into_any_element(),
            )))
            .into_any_element()
    }

    /// `browser.sendToAgent`: `Sparkles`, turning while the page is read.
    fn send_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let label = cx.t("browser.sendToAgent");
        let button = Button::new("browser-send-agent")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm)
            .muted()
            .disabled(!self.can_send())
            .on_click(cx.listener(|surface, _, _, cx| surface.send_to_agent(cx)))
            .child(if self.sending {
                spinner("browser-send-busy", 14.0).into_any_element()
            } else {
                Icon::new("sparkles").size(14.0).into_any_element()
            });
        Tooltip::new("browser-send-agent-tip", button, move |_, _| {
            div().child(label.clone()).into_any_element()
        })
        .into_any_element()
    }

    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let close_label = cx.t("browser.close");
        let close = Button::new("browser-close")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm)
            .on_click(cx.listener(|surface, _, _, cx| surface.close(cx)))
            .child(Icon::new("x").size(14.0));
        div()
            .flex()
            .h(px(44.0))
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background.opacity(0.95))
            .px(px(10.0))
            .child(self.address_bar(cx))
            .child(self.sessions_button(cx))
            .child(self.send_button(cx))
            .children(self.agent_pill(cx))
            .children(self.audit_button(cx))
            .child(Tooltip::new("browser-close-tip", close, move |_, _| {
                div().child(close_label.clone()).into_any_element()
            }))
            .into_any_element()
    }

    fn strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let content = if let Some(error) = &self.error {
            div()
                .truncate()
                .text_color(colors.destructive)
                .child(cx.t_with(
                    "browser.error",
                    &Args::default().with("error", error.clone()),
                ))
                .into_any_element()
        } else if let Some(capture) = self.evidence_strip(cx) {
            capture
        } else if self.loading {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_color(colors.muted_foreground)
                .child(
                    div()
                        .text_color(tw::CYAN_500.hsla())
                        .child(spinner("browser-loading", 12.0)),
                )
                .child(cx.t("browser.loading"))
                .into_any_element()
        } else {
            let active = (self.request.session_id.is_some() && self.control.armed).then(|| {
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(4.0))
                    .text_color(tw::EMERALD_500.hsla())
                    .child(Icon::new("shield-check").size(12.0))
                    .child(cx.t("browser.controlActiveShort"))
            });
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(colors.muted_foreground)
                        .children(self.title.clone()),
                )
                .children(active)
                .into_any_element()
        };
        div()
            .debug_selector(|| "browser-strip".into())
            .flex()
            .min_h(px(28.0))
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.5))
            .bg(colors.background)
            .px(px(12.0))
            .text_size(px(12.0))
            .font_weight(FontWeight::NORMAL)
            .child(content)
            .into_any_element()
    }
}

impl Render for BrowserSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_page(window, cx);
        let colors = cx.theme().colors.clone();
        div()
            .id("browser-surface")
            .debug_selector(|| "browser-surface".into())
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex_col()
            .overflow_hidden()
            .bg(colors.background)
            .child(self.header(cx))
            .child(self.strip(cx))
            .children(self.sessions_dialog(cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_hidden()
                    .bg(gpui::white())
                    .child(Viewport {
                        surface: cx.entity(),
                    }),
            )
    }
}
