//! The preview as the desktop draws it: `w-[min(320px,100%-2rem)]`,
//! `rounded-xl`, `bottom-4 right-4`; the frame on black, `max-h-52`, or up
//! to `min(60vh, 520px)` expanded; hidden, a pill brings it back.

use std::time::Duration;

use gpui::{
    AnimationExt, FontWeight, IntoElement, ObjectFit, Render, StyledImage, Window, div, img,
    prelude::*, px,
};

use super::preview::{ComputerPreview, activity_label_key};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

const WIDTH: f32 = 320.0;

impl Render for ComputerPreview {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self.session.is_none() || (self.frame.is_none() && self.activity.is_none()) {
            return div().into_any_element();
        }
        let colors = cx.theme().colors.clone();
        if self.hidden {
            if self.frame.is_none() {
                return div().into_any_element();
            }
            return div()
                .id("computer-preview-reopen")
                .debug_selector(|| "computer-preview-reopen".into())
                .absolute()
                .bottom(px(16.0))
                .right(px(16.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded_full()
                .border_1()
                .border_color(colors.border)
                .bg(colors.background.opacity(0.95))
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .shadow_lg()
                .cursor_pointer()
                .on_click(cx.listener(|preview, _, _, cx| preview.set_hidden(false, cx)))
                .child(Icon::new("monitor").size(14.0).color(colors.primary))
                .child(cx.t("settings.computerUse.preview.reopen"))
                .into_any_element();
        }
        let phase = self
            .activity
            .as_ref()
            .map(|activity| activity.phase.as_str());
        let tool = match &self.activity {
            Some(activity) => cx.t(&format!(
                "settings.computerUse.preview.tools.{}",
                activity_label_key(&activity.tool)
            )),
            None => cx.t("settings.computerUse.preview.ready"),
        };
        let state = match phase {
            Some("started") => cx.t("settings.computerUse.preview.working"),
            Some("failed") => cx.t("settings.computerUse.preview.failed"),
            Some(_) => cx.t("settings.computerUse.preview.done"),
            None if self.turn_active => cx.t("settings.computerUse.preview.working"),
            None => cx.t("settings.computerUse.preview.ready"),
        };
        let dot_color = match phase {
            Some("failed") => colors.destructive,
            Some("started") => colors.primary,
            _ => tw::EMERALD_500.hsla(),
        };
        let dot = div()
            .relative()
            .flex_shrink_0()
            .size(px(8.0))
            .when(phase == Some("started"), |dot| {
                dot.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded_full()
                        .bg(colors.primary)
                        .with_animation(
                            "computer-preview-ping",
                            crate::ui::looping(Duration::from_secs(1)),
                            |ping, delta| ping.opacity(0.6 * (1.0 - delta)),
                        ),
                )
            })
            .child(div().absolute().inset_0().rounded_full().bg(dot_color));
        let title = self
            .frame
            .as_ref()
            .map(|frame| frame.app.clone().into())
            .unwrap_or_else(|| cx.t("settings.computerUse.preview.title"));
        let running = self.turn_active || phase == Some("started");
        let bar = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.7))
            .px(px(12.0))
            .py(px(8.0))
            .child(dot)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .line_height(px(14.0))
                            .text_color(colors.muted_foreground)
                            .child(format!("{tool} · {state}")),
                    ),
            )
            .when(running, |bar| {
                bar.child(
                    Button::new("computer-preview-stop")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .height(22.0)
                        .padding_x(8.0)
                        .text_size(10.0)
                        .text_color(colors.destructive)
                        .on_click(cx.listener(|preview, _, _, cx| preview.stop(cx)))
                        .child(cx.t("settings.computerUse.preview.stop")),
                )
            })
            .child(
                Button::new("computer-preview-expand")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .muted()
                    .on_click(cx.listener(|preview, _, _, cx| preview.toggle_expanded(cx)))
                    .child(
                        Icon::new(if self.expanded {
                            "minimize-2"
                        } else {
                            "maximize-2"
                        })
                        .size(14.0),
                    ),
            )
            .child(
                Button::new("computer-preview-hide")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .muted()
                    .on_click(cx.listener(|preview, _, _, cx| preview.set_hidden(true, cx)))
                    .child(Icon::new("x").size(14.0)),
            );
        let max_height = if self.expanded {
            (f32::from(window.viewport_size().height) * 0.6).min(520.0)
        } else {
            208.0
        };
        let body = match &self.frame {
            Some(frame) => {
                let natural = if frame.width > 0 {
                    WIDTH * frame.height as f32 / frame.width as f32
                } else {
                    max_height
                };
                div()
                    .flex()
                    .justify_center()
                    .bg(gpui::black())
                    .child(
                        img(frame.image.clone())
                            .w(px(WIDTH))
                            .h(px(natural.min(max_height)))
                            .object_fit(ObjectFit::Contain),
                    )
                    .into_any_element()
            }
            None => div()
                .flex()
                .h(px(96.0))
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .bg(colors.muted.opacity(0.25))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(colors.muted_foreground)
                .child(Icon::new("monitor").size(16.0))
                .child(cx.t("settings.computerUse.preview.waitingFrame"))
                .into_any_element(),
        };
        div()
            .id("computer-preview")
            .debug_selector(|| "computer-preview".into())
            .occlude()
            .absolute()
            .bottom(px(16.0))
            .right(px(16.0))
            .w(px(WIDTH))
            .overflow_hidden()
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.8))
            .bg(colors.background.opacity(0.95))
            .text_color(colors.foreground)
            .shadow_2xl()
            .child(bar)
            .child(body)
            .into_any_element()
    }
}
