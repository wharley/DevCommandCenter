//! "Novo feedback": the kind (three buttons), the title, the description
//! and, for a bug, the reproduction details; or the issue as it will read
//! once "Revisar feedback" is pressed. Under both: the diagnostics box, the
//! public notice and the footer with the draft's state and the actions.

use gpui::{AnyElement, Context, Entity, FocusHandle, FontWeight, Window, div, prelude::*, px};

use super::Feedback;
use crate::data::feedback::Category;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_area::TextArea;
use crate::ui::{Button, ButtonSize, ButtonVariant, Checkbox, Icon, Tracked, spinner};

/// `.dcc-feedback-field`: the label (12px medium) over its control.
fn field(label: gpui::SharedString, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .child(label)
        .child(control)
        .into_any_element()
}

/// The box around a control: `border-input rounded-[10px]`, the ring when
/// it has the focus.
fn frame(focus: &FocusHandle, window: &Window, cx: &Context<Feedback>) -> gpui::Div {
    let colors = &cx.theme().colors;
    let focused = focus.contains_focused(window, cx);
    div()
        .w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(if focused { colors.ring } else { colors.input })
        .font_weight(FontWeight::NORMAL)
}

impl Feedback {
    fn text_box(
        &self,
        area: &Entity<TextArea>,
        min_height: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let focus = gpui::Focusable::focus_handle(area.read(cx), cx);
        frame(&focus, window, cx)
            .min_h(px(min_height))
            .px(px(12.0))
            .py(px(10.0))
            .text_size(px(13.0))
            .line_height(px(20.8))
            .child(area.clone())
            .into_any_element()
    }

    fn fields(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let categories = Category::ALL.iter().map(|category| {
            let category = *category;
            let pressed = self.draft.category == category;
            div()
                .id(("feedback-category", category as usize))
                .debug_selector(move || format!("feedback-category-{}", category.key()))
                .flex()
                .flex_1()
                .flex_basis(px(0.0))
                .min_h(px(44.0))
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .p(px(8.0))
                .border_1()
                .rounded(px(10.0))
                .cursor_pointer()
                .map(|button| {
                    if pressed {
                        button
                            .text_color(colors.foreground)
                            .border_color(colors.foreground.opacity(0.28))
                            .bg(colors.foreground.opacity(0.06))
                    } else {
                        button
                            .text_color(colors.muted_foreground)
                            .border_color(colors.border)
                            .hover(|style| style.bg(colors.accent))
                    }
                })
                .on_click(
                    cx.listener(move |feedback, _, _, cx| feedback.set_category(category, cx)),
                )
                .child(Icon::new(category.icon()).size(17.0).stroke(1.7))
                .child(cx.t(&format!("feedback.categories.{}", category.key())))
        });
        let title_focus = gpui::Focusable::focus_handle(self.title.read(cx), cx);
        let title = frame(&title_focus, window, cx)
            .h(px(32.0))
            .flex()
            .items_center()
            .px(px(10.0))
            .text_size(px(14.0))
            .child(self.title.clone())
            .into_any_element();
        let reproduction = (self.draft.category == Category::Bug).then(|| {
            div()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(
                    div()
                        .id("feedback-reproduction")
                        .debug_selector(|| "feedback-reproduction".into())
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .cursor_pointer()
                        .on_click(cx.listener(|feedback, _, _, cx| {
                            feedback.details_open = !feedback.details_open;
                            cx.notify();
                        }))
                        .child(
                            Icon::new("chevron-right")
                                .size(12.0)
                                .rotate(if self.details_open { 90.0 } else { 0.0 }),
                        )
                        .child(cx.t("feedback.reproduction")),
                )
                .when(self.details_open, |details| {
                    details.child(
                        div()
                            .mt(px(12.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(field(
                                cx.t("feedback.steps"),
                                self.text_box(&self.steps, 80.0, window, cx),
                            ))
                            .child(field(
                                cx.t("feedback.expected"),
                                self.text_box(&self.expected, 80.0, window, cx),
                            )),
                    )
                })
        });
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(16.0))
            .text_color(colors.foreground)
            .child(div().flex().gap(px(8.0)).children(categories))
            .child(field(cx.t("feedback.subject"), title))
            .child(field(
                cx.t("feedback.description"),
                self.text_box(&self.description, 132.0, window, cx),
            ))
            .children(reproduction)
            .into_any_element()
    }

    fn review(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let bug_parts = (self.draft.category == Category::Bug).then(|| {
            [
                ("feedback.steps", self.draft.steps.trim().to_string()),
                ("feedback.expected", self.draft.expected.trim().to_string()),
            ]
            .into_iter()
            .filter(|(_, text)| !text.is_empty())
            .map(|(heading, text)| {
                div()
                    .child(
                        div()
                            .mb(px(4.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t(heading)),
                    )
                    .child(div().text_color(colors.muted_foreground).child(text))
            })
            .collect::<Vec<_>>()
        });
        div()
            .debug_selector(|| "feedback-review".into())
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(16.0))
            .border_1()
            .border_color(colors.border)
            .rounded(px(12.0))
            .text_size(px(13.0))
            .child(
                div()
                    .text_color(colors.muted_foreground)
                    .child(Tracked::new(
                        cx.t("feedback.preview").to_uppercase(),
                        10.0,
                        0.1,
                    )),
            )
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.draft.report_title()),
            )
            .child(
                div()
                    .line_height(px(21.1))
                    .child(self.draft.description.trim().to_string()),
            )
            .children(bug_parts.into_iter().flatten())
            .into_any_element()
    }

    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let pending = self.draft.pending.clone();
        let show_review = self.show_review();
        let state = cx.t(if pending.is_some() {
            "feedback.pending"
        } else if self.saved {
            "feedback.draftSaved"
        } else {
            "feedback.draftUnsaved"
        });
        let blocked = self.sending
            || !self.can_review()
            || pending
                .as_ref()
                .is_some_and(|pending| Some(pending.login.as_str()) != self.login().as_deref());
        let label = cx.t(if self.sending {
            "feedback.sending"
        } else if pending.is_some() {
            "feedback.checkDelivery"
        } else if show_review {
            "feedback.publish"
        } else {
            "feedback.review"
        });
        let submit = Button::new("feedback-submit")
            .variant(ButtonVariant::Default)
            .disabled(blocked)
            .on_click(cx.listener(|feedback, _, _, cx| feedback.submit_form(cx)))
            .child(if self.sending {
                spinner("feedback-sending", 14.0).into_any_element()
            } else {
                Icon::new("send").size(14.0).into_any_element()
            })
            .child(label);
        div()
            .child(
                div()
                    .mt(px(20.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .border_t_1()
                    .border_color(colors.border)
                    .pt(px(16.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child(state),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .when(show_review && pending.is_none(), |row| {
                                row.child(
                                    Button::new("feedback-edit")
                                        .variant(ButtonVariant::Ghost)
                                        .disabled(self.sending)
                                        .on_click(cx.listener(|feedback, _, _, cx| {
                                            feedback.reviewing = false;
                                            cx.notify();
                                        }))
                                        .child(cx.t("feedback.edit")),
                                )
                            })
                            .when(pending.is_some(), |row| {
                                row.child(
                                    Button::new("feedback-github")
                                        .variant(ButtonVariant::Outline)
                                        .disabled(self.sending)
                                        .on_click(cx.listener(|feedback, _, _, cx| {
                                            feedback.open_issue(None, cx)
                                        }))
                                        .child(cx.t("feedback.github")),
                                )
                            })
                            .child(submit),
                    ),
            )
            .children(pending.filter(|_| !self.sending).map(|pending| {
                div()
                    .mt(px(12.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t_with(
                        "feedback.pendingAccount",
                        &Args::default().with("login", pending.login),
                    ))
                    .child(
                        Button::new("feedback-another")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .on_click(cx.listener(|feedback, _, _, cx| feedback.start_new(cx)))
                            .child(cx.t("feedback.another")),
                    )
            }))
            .into_any_element()
    }

    pub(super) fn form(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locked = self.sending || self.draft.pending.is_some();
        let body = if self.show_review() {
            self.review(cx)
        } else {
            self.fields(window, cx)
        };
        let environment = self
            .account
            .context
            .as_ref()
            .filter(|_| self.draft.include_diagnostics)
            .map(|context| {
                div()
                    .mt(px(8.0))
                    .text_color(colors.muted_foreground)
                    .child(format!(
                        "DCC {} · {} · {}",
                        context.version, context.platform, context.architecture
                    ))
            });
        let entity = cx.entity().downgrade();
        div()
            .child(body)
            .child(
                div()
                    .my(px(16.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.7))
                    .p(px(12.0))
                    .text_size(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                Checkbox::new(
                                    "feedback-diagnostics",
                                    self.draft.include_diagnostics,
                                )
                                .disabled(locked)
                                .on_toggle(
                                    move |checked, _, cx| {
                                        let _ = entity.update(cx, |feedback, cx| {
                                            feedback.edit(
                                                |draft| draft.include_diagnostics = checked,
                                                cx,
                                            )
                                        });
                                    },
                                ),
                            )
                            .child(cx.t("feedback.diagnostics")),
                    )
                    .children(environment),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(19.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("feedback.publicNotice")),
            )
            .child(self.footer(cx))
            .into_any_element()
    }
}
