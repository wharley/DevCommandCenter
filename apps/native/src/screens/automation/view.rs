//! The dialog's frame (`min(96vw,1080px)` × `min(88vh,920px)`): the header,
//! the setup and delivery policy cards, the tasks, the run's report and the
//! footer with where the file lives.

use gpui::{
    AnyElement, Context, Entity, FontWeight, Render, SharedString, Window, div, prelude::*, px,
};

use super::{Loaded, ProjectAutomation};
use crate::data::automation::REQUIREMENTS;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::theme::tailwind as tw;
use crate::ui::text_input::TextInput;
use crate::ui::{
    Button, ButtonVariant, Checkbox, Dialog, Icon, dialog_footer, dialog_header, spinner,
};

/// shadcn's `Input`, mono for commands and paths.
pub(super) fn field(
    id: String,
    input: &Entity<TextInput>,
    mono: bool,
    cx: &gpui::App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .debug_selector(move || id.clone())
        .flex()
        .items_center()
        .w_full()
        .h(px(36.0))
        .px(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(colors.input)
        .text_size(px(if mono { 12.0 } else { 14.0 }))
        .when(mono, |field| {
            field.font_family(cx.theme().fonts.mono.clone())
        })
        .child(input.clone())
        .into_any_element()
}

/// shadcn's `Label`.
pub(super) fn label(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.foreground)
        .child(text)
        .into_any_element()
}

fn heading(title: SharedString, body: SharedString, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.foreground)
                .child(title),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .line_height(px(19.5))
                .text_color(colors.muted_foreground)
                .child(body),
        )
        .into_any_element()
}

impl ProjectAutomation {
    fn setup_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.15))
            .p(px(16.0))
            .child(div().mb(px(12.0)).child(heading(
                cx.t("automation.setupTitle"),
                cx.t("automation.setupDescription"),
                cx,
            )))
            .child(label(cx.t("automation.command"), cx))
            .child(
                div()
                    .mt(px(6.0))
                    .child(field("automation-setup".into(), &self.setup, true, cx)),
            )
            .into_any_element()
    }

    fn delivery_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let policy = &self.draft.policy;
        let switches = REQUIREMENTS.iter().map(|key| {
            let key = *key;
            let this = cx.entity().downgrade();
            div()
                .flex()
                .items_start()
                .gap(px(8.0))
                .text_size(px(12.0))
                .text_color(colors.foreground)
                .child(
                    div().mt(px(2.0)).child(
                        Checkbox::new(
                            SharedString::from(format!("automation-{key}")),
                            policy.required(key),
                        )
                        .on_toggle(move |on, _, cx| {
                            let _ = this.update(cx, |this, cx| {
                                this.draft.policy.set_required(key, on);
                                cx.notify();
                            });
                        }),
                    ),
                )
                .child(cx.t(&format!("automation.{key}")))
        });
        div()
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.15))
            .p(px(16.0))
            .child(
                div()
                    .mb(px(12.0))
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(
                        div()
                            .mt(px(2.0))
                            .child(Icon::new("shield-check").size(16.0).color(colors.primary)),
                    )
                    .child(heading(
                        cx.t("automation.deliveryTitle"),
                        cx.t("automation.deliveryDescription"),
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(12.0))
                    .child(
                        div()
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(colors.border.opacity(0.5))
                            .bg(colors.background.opacity(0.7))
                            .p(px(12.0))
                            .child(label(cx.t("automation.minimumApprovals"), cx))
                            .child(div().mt(px(6.0)).child(field(
                                "automation-approvals".into(),
                                &self.approvals,
                                false,
                                cx,
                            )))
                            .child(
                                div()
                                    .mt(px(6.0))
                                    .text_size(px(11.0))
                                    .line_height(px(17.9))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("automation.minimumApprovalsHint")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(colors.border.opacity(0.5))
                            .bg(colors.background.opacity(0.7))
                            .p(px(12.0))
                            .children(switches),
                    ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .text_size(px(11.0))
                    .line_height(px(17.9))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("automation.deliveryPolicyHint")),
            )
            .into_any_element()
    }

    fn report(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let result = self.result.as_ref()?;
        let colors = cx.theme().colors.clone();
        let passed = result.passed();
        let (tone, icon) = if passed {
            (tw::EMERALD_500.hsla(), "circle-check")
        } else {
            (colors.destructive, "circle-alert")
        };
        let steps = result.report.steps.iter().enumerate().map(|(index, step)| {
            // `open={!step.success}`, then as the person leaves it.
            let open = step.success == self.toggled.contains(&index);
            let output = if step.output.is_empty() {
                cx.t("automation.noOutput").to_string()
            } else {
                step.output.clone()
            };
            div()
                .rounded(px(10.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.background.opacity(0.8))
                .p(px(12.0))
                .child(
                    div()
                        .id(SharedString::from(format!("automation-step-{index}")))
                        .debug_selector(move || format!("automation-step-{index}"))
                        .cursor_pointer()
                        .font_family(cx.theme().fonts.mono.clone())
                        .text_size(px(12.0))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.toggled.remove(&index) {
                                this.toggled.insert(index);
                            }
                            cx.notify();
                        }))
                        .child(format!(
                            "{} {}",
                            if step.success { "✓" } else { "✕" },
                            step.command
                        )),
                )
                .when(open, |step| {
                    step.child(
                        div()
                            .id(SharedString::from(format!(
                                "automation-step-output-{index}"
                            )))
                            .mt(px(12.0))
                            .max_h(px(256.0))
                            .overflow_y_scroll()
                            .rounded(px(8.0))
                            .bg(colors.muted.opacity(0.6))
                            .p(px(12.0))
                            .font_family(cx.theme().fonts.mono.clone())
                            .text_size(px(11.0))
                            .line_height(px(20.0))
                            .child(output),
                    )
                })
        });
        Some(
            div()
                .debug_selector(|| "automation-report".into())
                .rounded(px(14.0))
                .border_1()
                .border_color(tone.opacity(0.3))
                .bg(tone.opacity(0.05))
                .p(px(16.0))
                .child(
                    div()
                        .mb(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(Icon::new(icon).size(16.0).color(tone))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(cx.t(if passed {
                                    "automation.reportPassed"
                                } else {
                                    "automation.reportFailed"
                                })),
                        )
                        .when(result.changed_files, |row| {
                            row.child(
                                div()
                                    .h(px(20.0))
                                    .px(px(8.0))
                                    .rounded(px(26.0))
                                    .border_1()
                                    .border_color(colors.border)
                                    .text_size(px(12.0))
                                    .child(cx.t("automation.filesChanged")),
                            )
                        }),
                )
                .child(div().flex().flex_col().gap(px(8.0)).children(steps))
                .into_any_element(),
        )
    }

    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let config = self.config();
        let busy =
            self.saving || matches!(self.loaded, Loaded::Loading) || !self.running.is_empty();
        let tracked = config.map(|config| config.tracked_in_git);
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.5))
            .px(px(20.0))
            .py(px(12.0))
            .child(
                div()
                    .mr_auto()
                    .min_w_0()
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .items_center()
                            .gap(px(8.0))
                            .children(
                                config.map(|config| {
                                    div().truncate().child(config.source_path.clone())
                                }),
                            )
                            .children(tracked.map(|tracked| {
                                div()
                                    .flex_none()
                                    .h(px(20.0))
                                    .px(px(8.0))
                                    .rounded(px(26.0))
                                    .border_1()
                                    .border_color(if tracked {
                                        gpui::transparent_black()
                                    } else {
                                        colors.border
                                    })
                                    .when(tracked, |badge| {
                                        badge.bg(mix_oklab(colors.success, 0.2, colors.secondary))
                                    })
                                    .text_size(px(10.0))
                                    .text_color(colors.foreground)
                                    .child(cx.t(if tracked {
                                        "automation.trackedInGit"
                                    } else {
                                        "automation.notTrackedInGit"
                                    }))
                            })),
                    )
                    .child(cx.t("automation.noPolling")),
            )
            .child(
                Button::new("automation-close")
                    .variant(ButtonVariant::Outline)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
                    .child(cx.t("automation.close")),
            )
            .child(
                Button::new("automation-save")
                    .disabled(busy || config.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .children(self.saving.then(|| spinner("automation-saving", 16.0)))
                            .child(cx.t("automation.save")),
                    ),
            )
            .into_any_element()
    }

    /// `window.confirm(automation.confirmFix)`
    fn confirm(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ids = self.confirm_fix.clone()?;
        let close = cx.entity().downgrade();
        Some(
            Dialog::new("automation-confirm-fix", move |_, cx| {
                let _ = close.update(cx, |this, cx| {
                    this.confirm_fix = None;
                    cx.notify();
                });
            })
            .close_button(false)
            .child(dialog_header(cx.t("automation.confirmFix"), None, cx))
            .child(dialog_footer(
                [
                    Button::new("automation-confirm-cancel")
                        .variant(ButtonVariant::Outline)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_fix = None;
                            cx.notify();
                        }))
                        .child(cx.t("sidebar.cancel"))
                        .into_any_element(),
                    Button::new("automation-confirm-run")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_now(ids.clone(), window, cx)
                        }))
                        .child(cx.t("automation.runTask"))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element(),
        )
    }
}

impl Render for ProjectAutomation {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) * 0.96).min(1080.0);
        let height = (f32::from(viewport.height) * 0.88).min(920.0);
        let body: AnyElement = match &self.loaded {
            Loaded::Loading => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_color(colors.muted_foreground)
                .child(spinner("automation-loading", 16.0))
                .child(cx.t("automation.loading"))
                .into_any_element(),
            Loaded::Failed(error) => div()
                .rounded(px(14.0))
                .border_1()
                .border_color(colors.destructive.opacity(0.3))
                .bg(colors.destructive.opacity(0.05))
                .p(px(16.0))
                .text_color(colors.destructive)
                .child(error.clone())
                .into_any_element(),
            Loaded::Ready(_) => div()
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(self.setup_card(cx))
                .child(self.delivery_card(cx))
                .child(self.tasks_section(cx))
                .children(self.report(cx))
                .into_any_element(),
        };
        let close = cx.entity().downgrade();
        let header = div()
            .flex_none()
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(20.0))
            .py(px(16.0))
            .pr(px(48.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(32.0))
                            .rounded(px(10.0))
                            .bg(colors.primary.opacity(0.1))
                            .child(Icon::new("settings-2").size(16.0).color(colors.primary)),
                    )
                    .child(dialog_header(
                        cx.t("automation.title"),
                        Some(cx.t("automation.description")),
                        cx,
                    )),
            );
        div()
            .child(
                Dialog::new("project-automation", move |_, cx| {
                    let _ = close.update(cx, |this, cx| this.close(cx));
                })
                .max_width(width)
                .max_height(height)
                .padding(0.0)
                .gap(0.0)
                .child(
                    div()
                        .debug_selector(|| "project-automation".into())
                        .flex()
                        .flex_col()
                        .w_full()
                        .h(px(height))
                        .overflow_hidden()
                        .child(header)
                        .child(
                            div()
                                .id("automation-body")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .px(px(20.0))
                                .py(px(16.0))
                                .child(body),
                        )
                        .child(self.footer(cx)),
                ),
            )
            .children(self.confirm(cx))
    }
}
