//! The help dialog as the desktop draws it: 1040×700 at most (94vw, 84vh),
//! `rounded-2xl` on `bg-background` with `shadow-2xl`; a 260px sidebar
//! with the title, the search and the topics, and the card on the right.

use gpui::{
    AnyElement, Context, FontWeight, HighlightStyle, IntoElement, Render, SharedString, StyledText,
    Window, div, prelude::*, px,
};

use super::topics::{self, TopicCopy};
use super::{Help, HelpEvent};
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, shortcut_keys};

/// `lg:px-8` from 1024px of window on.
const WIDE: f32 = 1024.0;

/// `HelpRichText`: plain text with the `[Rótulo]` chips (`font-medium
/// text-foreground`). GPUI runs keep one size and have no border, so the
/// chip is a tinted run at the paragraph's size.
fn rich_text(text: &str, cx: &Context<Help>) -> AnyElement {
    let colors = &cx.theme().colors;
    let (text, ranges) = topics::chips(text);
    let chip = HighlightStyle {
        color: Some(colors.foreground),
        background_color: Some(colors.muted),
        font_weight: Some(FontWeight::MEDIUM),
        ..Default::default()
    };
    let highlights = ranges
        .into_iter()
        .map(|range| (range, chip))
        .collect::<Vec<_>>();
    StyledText::new(SharedString::from(text))
        .with_highlights(highlights)
        .into_any_element()
}

fn section(title: SharedString, body: AnyElement, cx: &Context<Help>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().colors.muted_foreground)
                .child(title),
        )
        .child(body)
        .into_any_element()
}

/// `text-[13px] leading-6 text-foreground/90`
fn paragraph(text: &str, cx: &Context<Help>) -> AnyElement {
    div()
        .text_size(px(13.0))
        .line_height(px(24.0))
        .text_color(cx.theme().colors.foreground.opacity(0.9))
        .child(rich_text(text, cx))
        .into_any_element()
}

impl Help {
    fn render_nav(
        &self,
        current: Option<&'static str>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let field_focused =
            gpui::Focusable::focus_handle(self.input.read(cx), cx).is_focused(window);
        let visible = self.visible(cx);
        let topics = visible.iter().map(|topic| {
            let id = topic.id;
            let active = current == Some(id);
            div()
                .id(SharedString::from(format!("help-topic-{id}")))
                .debug_selector(move || format!("help-topic-{id}"))
                .flex()
                .w_full()
                .items_start()
                .gap(px(12.0))
                .rounded(px(10.0))
                .px(px(12.0))
                .py(px(8.0))
                .cursor_pointer()
                .map(|row| {
                    if active {
                        row.bg(colors.accent.opacity(0.7))
                            .text_color(colors.foreground)
                    } else {
                        row.text_color(colors.muted_foreground).hover(|style| {
                            style
                                .bg(colors.accent.opacity(0.5))
                                .text_color(colors.foreground)
                        })
                    }
                })
                .on_click(cx.listener(move |help, _, _, cx| help.select(id, cx)))
                .child(
                    div()
                        .mt(px(2.0))
                        .flex_none()
                        .child(Icon::new(topics::icon(id)).size(16.0).stroke(1.9)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(16.25))
                                .font_weight(FontWeight::MEDIUM)
                                .child(topic.label.clone()),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(11.0))
                                .line_height(px(13.75))
                                .text_color(colors.muted_foreground.opacity(0.8))
                                .child(topic.summary.clone()),
                        ),
                )
                .into_any_element()
        });
        let empty = visible.is_empty().then(|| {
            div()
                .px(px(12.0))
                .py(px(24.0))
                .text_center()
                .text_size(px(12.0))
                .line_height(px(20.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("help.empty"))
        });
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(260.0))
            .h_full()
            .overflow_hidden()
            .border_r_1()
            .border_color(colors.sidebar_border)
            .rounded_l(crate::ui::inner_radius(cx.theme().radius.xxl, false))
            .bg(colors.sidebar)
            .py(px(20.0))
            .child(
                div()
                    .px(px(16.0))
                    .pb(px(12.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .line_height(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(cx.t("help.title")),
                    )
                    .child(
                        div()
                            .debug_selector(|| "help-search".into())
                            .mt(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .h(px(32.0))
                            .pl(px(10.0))
                            .pr(px(10.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(if field_focused {
                                colors.ring
                            } else {
                                colors.input
                            })
                            .bg(colors.background)
                            .text_size(px(13.0))
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(colors.muted_foreground)
                                    .child(Icon::new("search").size(14.0).stroke(1.9)),
                            )
                            .child(div().flex_1().min_w_0().child(self.input.clone())),
                    ),
            )
            .child(
                div()
                    .id("help-topics")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(empty)
                    .children(topics),
            )
            .child(
                div()
                    .mt_auto()
                    .border_t_1()
                    .border_color(colors.sidebar_border)
                    .px(px(12.0))
                    .pt(px(12.0))
                    .child(
                        Button::new("help-open-shortcuts")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .muted()
                            .stretch()
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(HelpEvent::OpenShortcuts)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .child(Icon::new("keyboard").size(16.0).stroke(1.9))
                                    .child(cx.t("help.actions.openShortcuts")),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_card(&self, topic: &TopicCopy, wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let side = if wide { 32.0 } else { 24.0 };
        let shortcut = topics::shortcut(topic.id).map(|keys| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("help.sections.shortcut"))
                .child(shortcut_keys(&keys, cx))
        });
        let steps = (!topic.steps.is_empty()).then(|| {
            let items = topic.steps.iter().enumerate().map(|(index, step)| {
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .px(px(16.0))
                    .py(px(12.0))
                    .child(
                        div()
                            .mt(px(2.0))
                            .flex()
                            .flex_none()
                            .size(px(20.0))
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(colors.accent)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .font_features(crate::ui::tabular_nums())
                            .child((index + 1).to_string()),
                    )
                    .child(div().flex_1().min_w_0().child(paragraph(step, cx)))
                    .into_any_element()
            });
            let list = div().flex().flex_col().gap(px(8.0)).children(items);
            section(cx.t("help.sections.steps"), list.into_any_element(), cx)
        });
        let prompt = topic.prompt.as_ref().map(|prompt| {
            let block = div()
                .rounded(px(14.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.muted.opacity(0.3))
                .px(px(16.0))
                .py(px(12.0))
                .font_family(fonts::MONO)
                .text_size(px(12.0))
                .line_height(px(24.0))
                .text_color(colors.foreground.opacity(0.9))
                .child(prompt.clone());
            section(cx.t("help.sections.prompt"), block.into_any_element(), cx)
        });
        let tip = topic.tip.as_ref().map(|tip| {
            div()
                .rounded(px(14.0))
                .border_1()
                .border_dashed()
                .border_color(colors.border.opacity(0.6))
                .px(px(16.0))
                .py(px(12.0))
                .text_size(px(12.0))
                .line_height(px(20.0))
                .text_color(colors.muted_foreground)
                .child(rich_text(tip, cx))
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .border_b_1()
                    .border_color(colors.border.opacity(0.4))
                    .px(px(side))
                    .py(px(16.0))
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .debug_selector(|| "help-card-title".into())
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .child(topic.label.clone()),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_size(px(12.0))
                                    .text_color(colors.muted_foreground)
                                    .child(topic.summary.clone()),
                            ),
                    )
                    .children(shortcut),
            )
            .child(
                div()
                    .id("help-card-body")
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(side))
                    .pt(px(20.0))
                    .pb(px(24.0))
                    .child(
                        div()
                            .mx_auto()
                            .w_full()
                            .max_w(px(640.0))
                            .flex()
                            .flex_col()
                            .gap(px(24.0))
                            .child(section(
                                cx.t("help.sections.whatIs"),
                                paragraph(&topic.what_is, cx),
                                cx,
                            ))
                            .child(section(
                                cx.t("help.sections.whenToUse"),
                                paragraph(&topic.when_to_use, cx),
                                cx,
                            ))
                            .children(steps)
                            .children(prompt)
                            .children(tip),
                    ),
            )
            .into_any_element()
    }
}

impl Render for Help {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let (width, height) = (f32::from(viewport.width), f32::from(viewport.height));
        let colors = cx.theme().colors.clone();
        let current = {
            let visible = self.visible(cx);
            visible
                .iter()
                .find(|topic| topic.id == self.active)
                .or(visible.first())
                .map(|topic| (*topic).clone())
        };
        let nav = self.render_nav(current.as_ref().map(|topic| topic.id), window, cx);
        let card = match &current {
            Some(topic) => self.render_card(topic, width >= WIDE, cx),
            None => div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .px(px(24.0))
                .text_center()
                .text_size(px(13.0))
                .line_height(px(24.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("help.emptyHint"))
                .into_any_element(),
        };
        let box_height = (height * 0.84).min(700.0);
        let close = cx.entity().downgrade();
        let input_focus = gpui::Focusable::focus_handle(self.input.read(cx), cx);
        Dialog::new("help-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |_, cx| cx.emit(HelpEvent::Closed));
        })
        .max_width((width * 0.94).min(1040.0))
        .max_height(box_height)
        .padding(0.0)
        .gap(0.0)
        .radius(f32::from(cx.theme().radius.xxl))
        .surface(colors.background)
        .shadow_2xl()
        .initial_focus(input_focus)
        .child(
            div()
                .debug_selector(|| "help-dialog".into())
                .flex()
                .w_full()
                .h(px(box_height))
                .overflow_hidden()
                .child(nav)
                .child(card),
        )
    }
}
