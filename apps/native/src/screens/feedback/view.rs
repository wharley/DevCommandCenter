//! The dialog's frame (`.dcc-feedback-dialog`: 680px, 24px in, the card
//! itself scrolls): the mark with the title, the two tabs, the repository
//! and the account, the notices, then the tab's content.

use gpui::{
    AnyElement, Context, FontWeight, Hsla, Render, SharedString, Window, div, prelude::*, px,
};

use super::{Feedback, FeedbackEvent, Tab};
use crate::data::feedback::REPOSITORY;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, Tabs, spinner};

/// `.dcc-feedback-notice`: a bordered line of 12px, wrapping its buttons.
pub(super) fn notice(color: Option<Hsla>, cx: &Context<Feedback>) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .my(px(12.0))
        .px(px(12.0))
        .py(px(10.0))
        .border_1()
        .border_color(colors.border)
        .rounded(px(10.0))
        .text_size(px(12.0))
        .line_height(px(19.2))
        .when_some(color, |notice, color| notice.text_color(color))
}

impl Feedback {
    fn header(&self, cx: &Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .pr(px(32.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .size(px(42.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(13.0))
                    .text_color(colors.foreground)
                    .bg(colors.foreground.opacity(0.06))
                    .child(Icon::new("message-square-more").size(21.0).stroke(1.7)),
            )
            .child(
                div()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .line_height(px(16.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t("feedback.title")),
                    )
                    .child(
                        div()
                            .mt(px(6.0))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("feedback.subtitle")),
                    ),
            )
            .into_any_element()
    }

    fn notices(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let mut notices = Vec::new();
        if self.account.fetching && self.account.context.is_none() && !self.account.failed {
            notices.push(
                notice(None, cx)
                    .child(spinner("feedback-connecting", 14.0))
                    .child(cx.t("feedback.connecting"))
                    .into_any_element(),
            );
        }
        if self.account.failed {
            notices.push(
                notice(None, cx)
                    .debug_selector(|| "feedback-connect-hint".into())
                    .child(div().child(cx.t("feedback.connectHint")))
                    .child(
                        Button::new("feedback-settings")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .on_click(
                                cx.listener(|_, _, _, cx| cx.emit(FeedbackEvent::OpenSettings)),
                            )
                            .child(cx.t("feedback.settings")),
                    )
                    .child(
                        Button::new("feedback-retry")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .disabled(self.account.fetching)
                            .on_click(cx.listener(|feedback, _, _, cx| feedback.load_account(cx)))
                            .child(cx.t("feedback.retry")),
                    )
                    .into_any_element(),
            );
        }
        if let Some(error) = self.error {
            notices.push(
                notice(Some(colors.destructive), cx)
                    .debug_selector(|| "feedback-error".into())
                    .child(cx.t(&format!("feedback.errors.{error}")))
                    .into_any_element(),
            );
        }
        if !self.saved {
            notices.push(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(tw::AMBER_600.hsla())
                    .child(cx.t("feedback.storageFailed"))
                    .into_any_element(),
            );
        }
        notices
    }
}

impl Render for Feedback {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let height = f32::from(window.viewport_size().height);
        let max_height = (height - 32.0).min(860.0);
        let tabs: Vec<SharedString> = vec![cx.t("feedback.new"), cx.t("feedback.mine")];
        let active = usize::from(self.tab == Tab::History);
        let entity = cx.entity().downgrade();
        let tab_bar = Tabs::new("feedback-tab", tabs, active)
            .padding_y(8.0)
            .disabled(self.sending)
            .on_select(move |index, _, cx| {
                let tab = if index == 0 { Tab::New } else { Tab::History };
                let _ = entity.update(cx, |feedback, cx| feedback.set_tab(tab, cx));
            });
        let meta =
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(4.0))
                .pt(px(8.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(colors.muted_foreground)
                .child(REPOSITORY)
                .children(self.login().map(|login| {
                    cx.t_with("feedback.account", &Args::default().with("login", login))
                }));
        let notices = self.notices(cx);
        let content = match self.tab {
            Tab::New => self.form(window, cx),
            Tab::History => self.history_tab(cx),
        };
        let close = cx.entity().downgrade();
        let focus = gpui::Focusable::focus_handle(self.title.read(cx), cx);
        Dialog::new("feedback-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |_, cx| cx.emit(FeedbackEvent::Closed));
        })
        .max_width(680.0)
        .max_height(max_height)
        .padding(0.0)
        .gap(0.0)
        // Nothing closes it while a report goes out.
        .close_button(!self.sending)
        .initial_focus(focus)
        .child(
            div()
                .id("feedback-scroll")
                .debug_selector(|| "feedback-dialog".into())
                .max_h(px(max_height))
                .overflow_y_scroll()
                .p(px(24.0))
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(self.header(cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(tab_bar)
                        .child(meta)
                        .children(notices)
                        .child(div().mt(px(12.0)).child(content)),
                ),
        )
    }
}
