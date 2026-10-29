//! The dialog's frame (`.skills-dialog`: 860×760 at most, the surface
//! edge): the header washed in the calm accent with the project's folder,
//! then either the form or the three lists between the navigation and the
//! source line.

use gpui::{
    AnyElement, Context, FontWeight, Render, SharedString, Window, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};

use super::{Section, Skills};
use crate::i18n::Translate;
use crate::theme::color::mix_srgb;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon};

impl Skills {
    fn header(&self, cx: &Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = crate::ui::surface_edge(cx.theme());
        let folder = self.root.as_ref().map(|root| {
            let leaf = root
                .split(['/', '\\'])
                .rfind(|part| !part.is_empty())
                .unwrap_or(root)
                .to_string();
            div()
                .mt(px(16.0))
                .ml(px(57.0))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(7.0))
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("skills.design.project"))
                .child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(edge)
                        .bg(colors.muted.opacity(0.6))
                        .font_family(fonts::MONO)
                        .text_color(colors.foreground)
                        .child(leaf),
                )
        });
        div()
            .flex_none()
            .px(px(28.0))
            .pt(px(24.0))
            .pb(px(18.0))
            .rounded_t(crate::ui::inner_radius(cx.theme().radius.dialog, true))
            .bg(linear_gradient(
                125.0,
                linear_color_stop(mix_srgb(colors.accent_calm, 0.06, colors.popover), 0.0),
                linear_color_stop(colors.popover, 0.75),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(13.0))
                    .pr(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .size(px(44.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(13.0))
                            .text_color(colors.accent_calm)
                            .bg(colors.accent_calm.opacity(0.1))
                            .child(Icon::new("sparkles").size(20.0)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .line_height(px(20.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(cx.t("skills.title")),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.0))
                                    .line_height(px(18.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("skills.design.subtitle")),
                            ),
                    ),
            )
            .children(folder)
            .into_any_element()
    }

    fn navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = crate::ui::surface_edge(cx.theme());
        let count = |value: usize| -> SharedString {
            if self.loading {
                "—".into()
            } else {
                value.to_string().into()
            }
        };
        let tabs = [
            (
                Section::Library,
                "sparkles",
                cx.t("skills.design.library"),
                count(self.skills.len()),
            ),
            (
                Section::Detected,
                "folder-search",
                cx.t("skills.detected.title"),
                count(self.detections.len()),
            ),
            (
                Section::Catalog,
                "workflow",
                cx.t("skills.catalog.title"),
                crate::data::skills::catalog().len().to_string().into(),
            ),
        ];
        let buttons = tabs.into_iter().map(|(section, icon, label, count)| {
            let pressed = self.section == section;
            div()
                .id(("skills-section", section as usize))
                .debug_selector(move || format!("skills-section-{}", section as usize))
                .flex()
                .items_center()
                .gap(px(7.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(10.0))
                .border_1()
                .text_size(px(11.0))
                .cursor_pointer()
                .map(|button| {
                    if pressed {
                        button
                            .text_color(colors.foreground)
                            .border_color(edge)
                            .bg(mix_srgb(colors.accent_calm, 0.07, colors.background))
                            .shadow(vec![gpui::BoxShadow {
                                color: gpui::Hsla::black().opacity(0x0b as f32 / 255.0),
                                offset: gpui::point(px(0.0), px(1.0)),
                                blur_radius: px(3.0),
                                spread_radius: px(0.0),
                                inset: false,
                            }])
                    } else {
                        button
                            .text_color(colors.muted_foreground)
                            .border_color(gpui::transparent_black())
                    }
                })
                .on_click(cx.listener(move |skills, _, _, cx| skills.set_section(section, cx)))
                .child(Icon::new(icon).size(14.0).color(if pressed {
                    colors.accent_calm
                } else {
                    colors.muted_foreground
                }))
                .child(label)
                .child(
                    div()
                        .opacity(0.65)
                        .text_size(px(9.0))
                        .font_features(crate::ui::tabular_nums())
                        .child(count),
                )
                .into_any_element()
        });
        div()
            .flex()
            .flex_wrap()
            .flex_none()
            .items_center()
            .gap(px(5.0))
            .px(px(28.0))
            .pb(px(13.0))
            .border_b_1()
            .border_color(edge)
            .children(buttons)
            .child(
                div().ml_auto().flex_none().child(
                    Button::new("skills-refresh")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .disabled(self.loading || self.busy)
                        .on_click(cx.listener(|skills, _, _, cx| skills.refresh(cx)))
                        .child(Icon::new("refresh-cw").size(14.0)),
                ),
            )
            .into_any_element()
    }

    fn source_line(&self, cx: &Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .flex()
            .flex_wrap()
            .flex_none()
            .justify_between()
            .gap(px(5.0))
            .border_t_1()
            .border_color(crate::ui::surface_edge(cx.theme()))
            .px(px(28.0))
            .py(px(12.0))
            .text_size(px(10.0))
            .text_color(colors.muted_foreground)
            .child(cx.t("skills.design.source"))
            .child(
                div()
                    .font_family(fonts::MONO)
                    .child(".devcommandcenter/skills/"),
            )
            .into_any_element()
    }

    fn scroll(&self, content: AnyElement) -> gpui::Stateful<gpui::Div> {
        div()
            .id("skills-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .px(px(28.0))
            .pt(px(8.0))
            .pb(px(24.0))
            .child(content)
    }
}

impl Render for Skills {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let height = (f32::from(window.viewport_size().height) - 40.0).min(760.0);
        let header = self.header(cx);
        let mut body: Vec<AnyElement> = Vec::new();
        if self.root.is_none() {
            body.push(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(180.0))
                    .items_center()
                    .justify_center()
                    .gap(px(13.0))
                    .p(px(30.0))
                    .text_color(colors.muted_foreground)
                    .child(
                        div().opacity(0.7).child(
                            Icon::new("folder-search")
                                .size(30.0)
                                .color(colors.accent_calm),
                        ),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .child(cx.t("skills.selectProject")),
                    )
                    .into_any_element(),
            );
        } else if let Some(form) = self.form.take() {
            body.push(self.form_heading(&form, cx));
            let content = self.form_body(&form, window, cx);
            body.push(self.scroll(content).into_any_element());
            body.push(self.form_footer(&form, cx));
            self.form = Some(form);
        } else {
            body.push(self.navigation(cx));
            if self.section == super::Section::Library {
                body.push(self.library_tools(window, cx));
            }
            let content = if self.loading {
                self.loading_state(cx)
            } else {
                match self.section {
                    Section::Library => self.library(cx),
                    Section::Detected => self.detected(cx),
                    Section::Catalog => self.catalog(cx),
                }
            };
            body.push(self.scroll(content).into_any_element());
            body.push(self.source_line(cx));
        }
        let close = cx.entity().downgrade();
        // The search takes the focus when it shows; else the card does.
        let focus = (self.root.is_some() && self.form.is_none())
            .then(|| gpui::Focusable::focus_handle(self.search.read(cx), cx));
        Dialog::new("skills-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |skills, cx| skills.close(cx));
        })
        .max_width(860.0)
        .max_height(height)
        .padding(0.0)
        .gap(0.0)
        .border(crate::ui::surface_edge(cx.theme()))
        .close_button(!self.busy)
        .when_some(focus, |dialog, focus| dialog.initial_focus(focus))
        .child(
            div()
                .debug_selector(|| "skills-dialog".into())
                .flex()
                .flex_col()
                .h(px(height))
                .overflow_hidden()
                .child(header)
                .children(body),
        )
    }
}
