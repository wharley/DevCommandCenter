//! "Mesclar" on a GitHub pull request: the methods the repository allows,
//! off with the reason when it cannot (permission, draft, conflicts, no
//! reviewed commit…); then "Mesclar este pull request?" with the
//! repository, the target, the method, the commit and the checks, and the
//! forge's answer as a toast. The merge is of the commit reviewed.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::PrHub;
use super::list::check_icon;
use crate::data::pr_hub::{Item, MERGE_METHODS, merge_blocked, merge_toast};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, Menu, MenuAlign, MenuEntry, MenuItem, Toaster,
    Tooltip, dialog_footer, dialog_header, spinner,
};

impl PrHub {
    pub(super) fn merge_detail(
        &self,
        item: &Item,
    ) -> Option<&Result<crate::data::pr_hub::Detail, String>> {
        self.detail
            .as_ref()
            .filter(|(id, _, _)| *id == item.id)
            .and_then(|(_, _, result)| result.as_ref())
    }

    fn head_sha(&self, item: &Item) -> Option<String> {
        self.merge_detail(item)?
            .as_ref()
            .ok()?
            .merge_capabilities
            .as_ref()?
            .head_sha
            .clone()
    }

    /// The method picked: the confirmation opens.
    pub(crate) fn ask_merge(&mut self, method: &'static str, cx: &mut Context<Self>) {
        self.pending_merge = Some(method);
        self.merge_error = None;
        cx.notify();
    }

    pub(crate) fn cancel_merge(&mut self, cx: &mut Context<Self>) {
        if self.merging {
            return;
        }
        self.pending_merge = None;
        self.merge_error = None;
        cx.notify();
    }

    pub(crate) fn confirm_merge(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(item), Some(method)) =
            (self.api.clone(), self.selected_item(cx), self.pending_merge)
        else {
            return;
        };
        let Some(sha) = self.head_sha(&item) else {
            self.merge_error = Some(cx.t("pullRequests.merge.commitUnavailable").to_string());
            cx.notify();
            return;
        };
        if self.merging {
            return;
        }
        self.merging = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let merged = cx
                .background_executor()
                .spawn(async move { api.pr_hub_merge(&item, method, &sha) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.merging = false;
                match merged {
                    Ok(status) => {
                        hub.pending_merge = None;
                        Toaster::success(cx.t(merge_toast(&status)), cx);
                        hub.detail = None;
                        hub.ensure_detail(cx);
                        hub.refresh(cx);
                    }
                    Err(error) => {
                        let text = match error {
                            crate::data::desktop_api::ApiError::Failed(message)
                                if !message.is_empty() =>
                            {
                                message
                            }
                            _ => cx.t("pullRequests.merge.error").to_string(),
                        };
                        Toaster::error(text.clone(), cx);
                        hub.merge_error = Some(text);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Mesclar ⌄", GitHub only.
    /// `primary`: the merge is the next step, so its button is the filled one.
    pub(super) fn merge_button(
        &self,
        item: &Item,
        primary: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if item.provider != "github" {
            return None;
        }
        let blocked = merge_blocked(item, self.merge_detail(item));
        let merging = self.merging;
        let label = |cx: &gpui::App| {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(if merging {
                    spinner("pr-merging", 14.0).into_any_element()
                } else {
                    Icon::new("git-merge").size(14.0).into_any_element()
                })
                .child(cx.t("pullRequests.merge.action"))
                .child(Icon::new("chevron-down").size(12.0))
                .into_any_element()
        };
        if let Some(reason) = blocked {
            let reason = cx.t(reason);
            return Some(
                Tooltip::new(
                    "pr-merge-blocked-tip",
                    div().debug_selector(|| "pr-merge".into()).child(
                        Button::new("pr-merge")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .text_size(12.0)
                            .disabled(true)
                            .child(label(cx)),
                    ),
                    move |_, _| div().child(reason.clone()).into_any_element(),
                )
                .into_any_element(),
            );
        }
        let allowed: Vec<&'static str> = self
            .merge_detail(item)
            .and_then(|detail| detail.as_ref().ok())
            .and_then(|detail| detail.merge_capabilities.as_ref())
            .map(|merge| {
                MERGE_METHODS
                    .into_iter()
                    .filter(|method| {
                        merge
                            .allowed_methods
                            .iter()
                            .any(|allowed| allowed == method)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let trigger = label(cx);
        let colors = cx.theme().colors.clone();
        let this = cx.entity().downgrade();
        Some(
            Menu::dropdown(
                "pr-merge-menu",
                move |_| {
                    div()
                        .debug_selector(|| "pr-merge".into())
                        .flex()
                        .h(px(28.0))
                        .items_center()
                        .rounded(px(8.0))
                        .border_1()
                        .px(px(10.0))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .map(|button| {
                            if primary {
                                button
                                    .border_color(colors.primary)
                                    .bg(colors.primary)
                                    .text_color(colors.primary_foreground)
                                    .hover(|button| button.opacity(0.9))
                            } else {
                                button
                                    .border_color(colors.border)
                                    .bg(colors.background)
                                    .hover(|button| button.bg(colors.accent))
                            }
                        })
                        .when(merging, |button| button.opacity(0.5))
                        .child(trigger)
                        .into_any_element()
                },
                move |_, cx| {
                    allowed
                        .iter()
                        .map(|method| {
                            let method: &'static str = method;
                            let this = this.clone();
                            MenuEntry::Item(
                                MenuItem::new(
                                    cx.t(&format!("pullRequests.merge.methods.{method}.label")),
                                    move |_, cx| {
                                        let _ =
                                            this.update(cx, |hub, cx| hub.ask_merge(method, cx));
                                    },
                                )
                                .detail(
                                    cx.t(&format!(
                                        "pullRequests.merge.methods.{method}.description"
                                    )),
                                ),
                            )
                        })
                        .collect()
                },
            )
            .align(MenuAlign::End)
            .min_width(224.0)
            .into_any_element(),
        )
    }

    /// "Mesclar este pull request?"
    pub(super) fn merge_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let method = self.pending_merge?;
        let item = self.selected_item(cx)?;
        let colors = cx.theme().colors.clone();
        let sha: String = self
            .head_sha(&item)
            .unwrap_or_default()
            .chars()
            .take(12)
            .collect();
        let check_key = format!("pullRequests.checkStates.{}", item.checks_state);
        let check_text = {
            let text = cx.t(&check_key);
            if text.as_ref() == check_key {
                item.checks_state.clone()
            } else {
                text.to_string()
            }
        };
        let fact = |label: SharedString, value: AnyElement| {
            [
                div()
                    .text_color(colors.muted_foreground)
                    .child(label)
                    .into_any_element(),
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(value)
                    .into_any_element(),
            ]
        };
        let close = cx.entity().downgrade();
        let merging = self.merging;
        Some(
            Dialog::new("pr-merge-dialog", move |_, cx| {
                let _ = close.update(cx, |hub, cx| hub.cancel_merge(cx));
            })
            .child(
                div()
                    .debug_selector(|| "pr-merge-dialog".into())
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(dialog_header(
                        cx.t("pullRequests.merge.confirmTitle"),
                        Some(cx.t("pullRequests.merge.confirmDescription")),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.muted.opacity(0.3))
                            .p(px(12.0))
                            .text_size(px(11.0))
                            .child(
                                div()
                                    .grid()
                                    .grid_cols(2)
                                    .gap_x(px(12.0))
                                    .gap_y(px(8.0))
                                    .children(fact(
                                        cx.t("pullRequests.merge.repository"),
                                        div()
                                            .child(item.repository_name.clone())
                                            .into_any_element(),
                                    ))
                                    .children(fact(
                                        cx.t("pullRequests.merge.target"),
                                        div()
                                            .child(format!(
                                                "{} → {}",
                                                item.head_branch, item.base_branch
                                            ))
                                            .into_any_element(),
                                    ))
                                    .children(fact(
                                        cx.t("pullRequests.merge.method"),
                                        div()
                                            .child(cx.t(&format!(
                                                "pullRequests.merge.methods.{method}.label"
                                            )))
                                            .into_any_element(),
                                    ))
                                    .children(fact(
                                        cx.t("pullRequests.merge.commit"),
                                        div()
                                            .font_family(cx.theme().fonts.mono.clone())
                                            .text_size(px(10.0))
                                            .child(sha)
                                            .into_any_element(),
                                    ))
                                    .children(fact(
                                        cx.t("pullRequests.checks"),
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(check_icon(&item.checks_state))
                                            .child(check_text)
                                            .into_any_element(),
                                    )),
                            )
                            .child(
                                div()
                                    .border_t_1()
                                    .border_color(colors.border.opacity(0.7))
                                    .pt(px(8.0))
                                    .text_size(px(10.0))
                                    .line_height(px(16.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("pullRequests.merge.protectionNotice")),
                            ),
                    )
                    .children(self.merge_error.clone().map(|error| {
                        div()
                            .text_size(px(11.0))
                            .text_color(tw::RED_500.hsla())
                            .child(error)
                    }))
                    .child(dialog_footer(
                        [
                            Button::new("pr-merge-cancel")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Default)
                                .text_size(14.0)
                                .disabled(merging)
                                .on_click(cx.listener(|hub, _, _, cx| hub.cancel_merge(cx)))
                                .child(cx.t("pullRequests.merge.cancel"))
                                .into_any_element(),
                            Button::new("pr-merge-confirm")
                                .variant(ButtonVariant::Default)
                                .size(ButtonSize::Default)
                                .text_size(14.0)
                                .disabled(merging)
                                .on_click(cx.listener(|hub, _, _, cx| hub.confirm_merge(cx)))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .when(merging, |label| {
                                            label.child(spinner("pr-merge-confirming", 14.0))
                                        })
                                        .child(cx.t("pullRequests.merge.confirm")),
                                )
                                .into_any_element(),
                        ],
                        cx,
                    )),
            )
            .into_any_element(),
        )
    }
}
