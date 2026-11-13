//! What the person can do to a GitHub pull request from the hub besides
//! reviewing and merging it: bring its branch up to date with the base,
//! close it, reopen a closed one, delete the branch of a closed one. Each
//! is picked in the "⋯" menu (or on the closed pull request itself) and
//! runs only after its confirmation. What GitHub refuses is shown as
//! GitHub says it. A pull request just closed leaves the list of open ones
//! but stays on screen, closed, with "Reabrir", until another is picked:
//! there is no list of closed ones to read.

use gpui::{AnyElement, AppContext, Context, Entity, SharedString, Window, div, prelude::*, px};

use super::PrHub;
use crate::data::desktop_api::ApiError;
use crate::data::pr_hub::Item;
use crate::data::pr_merge_readiness::PrAction;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, Menu, MenuAlign, MenuEntry, MenuItem, Switch,
    TextArea, Toaster, dialog_footer, dialog_header, spinner,
};

/// The action waiting for its confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ask {
    UpdateBranch,
    Close,
    Reopen,
    DeleteBranch,
}

impl Ask {
    /// `pullRequests.actions.<key>.*`
    pub fn key(self) -> &'static str {
        match self {
            Ask::UpdateBranch => "updateBranch",
            Ask::Close => "close",
            Ask::Reopen => "reopen",
            Ask::DeleteBranch => "deleteBranch",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Ask::UpdateBranch => "refresh-cw",
            Ask::Close => "circle-x",
            Ask::Reopen => "rotate-ccw",
            Ask::DeleteBranch => "trash-2",
        }
    }
}

/// What a pull request offers: an open one can be updated when behind its
/// base and closed; a closed one reopened, and its branch deleted.
pub(crate) fn offered(closed: bool, behind: bool, own_branch: bool) -> Vec<Ask> {
    if closed {
        let mut asks = vec![Ask::Reopen];
        if own_branch {
            asks.push(Ask::DeleteBranch);
        }
        return asks;
    }
    let mut asks = Vec::new();
    if behind {
        asks.push(Ask::UpdateBranch);
    }
    asks.push(Ask::Close);
    asks
}

pub(crate) struct PrActions {
    pub ask: Option<Ask>,
    pub running: bool,
    pub error: Option<String>,
    /// "Apagar o branch" with the close.
    pub delete_branch: bool,
    /// The close's optional comment.
    pub comment: Entity<TextArea>,
    /// The one just closed, kept on screen though no longer listed.
    pub held: Option<Item>,
}

impl PrActions {
    pub fn new(window: &mut Window, cx: &mut Context<PrHub>) -> Self {
        let placeholder = cx.t("pullRequests.actions.close.comment");
        Self {
            ask: None,
            running: false,
            error: None,
            delete_branch: false,
            comment: cx.new(|cx| TextArea::new(window, cx).with_placeholder(placeholder)),
            held: None,
        }
    }
}

impl PrHub {
    /// The selected pull request is closed: the one just closed here, or
    /// what its detail says.
    pub(crate) fn is_closed(&self, item: &Item) -> bool {
        self.actions
            .held
            .as_ref()
            .is_some_and(|held| held.id == item.id)
            || self
                .merge_detail(item)
                .and_then(|detail| detail.as_ref().ok())
                .is_some_and(|detail| detail.state.as_deref() == Some("closed"))
    }

    fn asks_for(&self, item: &Item) -> Vec<Ask> {
        if item.provider != "github" {
            return Vec::new();
        }
        let merge = self.merge_capabilities(item);
        offered(
            self.is_closed(item),
            merge.is_some_and(|merge| merge.merge_state_status.as_deref() == Some("BEHIND")),
            merge.is_some_and(|merge| merge.can_delete_branch),
        )
    }

    pub(crate) fn ask_action(&mut self, ask: Ask, cx: &mut Context<Self>) {
        self.actions.ask = Some(ask);
        self.actions.error = None;
        self.actions.delete_branch = false;
        self.actions
            .comment
            .update(cx, |area, cx| area.set_text("", cx));
        cx.notify();
    }

    pub(crate) fn cancel_action(&mut self, cx: &mut Context<Self>) {
        if !self.actions.running {
            self.actions.ask = None;
            self.actions.error = None;
            cx.notify();
        }
    }

    pub(crate) fn confirm_action(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(item), Some(ask)) =
            (self.api.clone(), self.selected_item(cx), self.actions.ask)
        else {
            return;
        };
        if self.actions.running {
            return;
        }
        let action = match ask {
            Ask::UpdateBranch => {
                let Some(head_sha) = self
                    .merge_capabilities(&item)
                    .and_then(|merge| merge.head_sha.clone())
                else {
                    self.actions.error =
                        Some(cx.t("pullRequests.merge.commitUnavailable").to_string());
                    cx.notify();
                    return;
                };
                PrAction::UpdateBranch { head_sha }
            }
            Ask::Close => PrAction::Close {
                comment: self.actions.comment.read(cx).text().trim().to_string(),
                delete_branch: self.actions.delete_branch,
            },
            Ask::Reopen => PrAction::Reopen,
            Ask::DeleteBranch => PrAction::DeleteBranch,
        };
        self.actions.running = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn({
                    let item = item.clone();
                    async move { api.pr_hub_action(&item, &action) }
                })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.actions.running = false;
                match done {
                    Ok(branch_deleted) => {
                        hub.actions.ask = None;
                        let key = if ask == Ask::Close && branch_deleted {
                            "pullRequests.actions.close.doneWithBranch".to_string()
                        } else {
                            format!("pullRequests.actions.{}.done", ask.key())
                        };
                        Toaster::success(cx.t(&key), cx);
                        // Closed: it stays on screen with its way back.
                        // Reopened: it is among the open ones again.
                        hub.actions.held = match ask {
                            Ask::Close => Some(item.clone()),
                            Ask::Reopen => None,
                            _ => hub.actions.held.take(),
                        };
                        hub.detail = None;
                        hub.refresh(cx);
                        hub.ensure_detail(cx);
                    }
                    Err(error) => {
                        let text = match error {
                            ApiError::Failed(message) if !message.is_empty() => message,
                            other => other.text(cx),
                        };
                        hub.actions.error = Some(text);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "⋯" in the detail's header: what this pull request offers.
    pub(super) fn actions_menu(&self, item: &Item, cx: &mut Context<Self>) -> Option<AnyElement> {
        let asks = self.asks_for(item);
        if asks.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let this = cx.entity().downgrade();
        Some(
            Menu::dropdown(
                "pr-actions-menu",
                move |_| {
                    div()
                        .debug_selector(|| "pr-actions".into())
                        .flex()
                        .size(px(28.0))
                        .items_center()
                        .justify_center()
                        .rounded(px(8.0))
                        .text_color(colors.muted_foreground)
                        .cursor_pointer()
                        .hover(|button| button.bg(colors.accent))
                        .child(Icon::new("ellipsis").size(14.0))
                        .into_any_element()
                },
                move |_, cx| {
                    asks.iter()
                        .map(|ask| {
                            let (ask, this) = (*ask, this.clone());
                            let item = MenuItem::new(
                                cx.t(&format!("pullRequests.actions.{}.label", ask.key())),
                                move |_, cx| {
                                    let _ = this.update(cx, |hub, cx| hub.ask_action(ask, cx));
                                },
                            )
                            .icon(Icon::new(ask.icon()).size(14.0).stroke(1.8));
                            MenuEntry::Item(if ask == Ask::UpdateBranch {
                                item
                            } else {
                                item.destructive()
                            })
                        })
                        .collect()
                },
            )
            .align(MenuAlign::End)
            .min_width(224.0)
            .into_any_element(),
        )
    }

    /// The confirmation of the action picked.
    pub(super) fn action_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ask = self.actions.ask?;
        let item = self.selected_item(cx)?;
        let colors = cx.theme().colors.clone();
        let text = |part: &str, cx: &gpui::App| -> SharedString {
            cx.t(&format!("pullRequests.actions.{}.{part}", ask.key()))
        };
        let running = self.actions.running;
        let close = cx.entity().downgrade();
        let toggle = cx.entity().downgrade();
        let offers_branch = ask == Ask::Close
            && self
                .merge_capabilities(&item)
                .is_some_and(|merge| merge.can_delete_branch);
        Some(
            Dialog::new("pr-action-dialog", move |_, cx| {
                let _ = close.update(cx, |hub, cx| hub.cancel_action(cx));
            })
            .child(
                div()
                    .debug_selector(|| "pr-action-dialog".into())
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(dialog_header(
                        text("title", cx),
                        Some(text("description", cx)),
                        cx,
                    ))
                    .child(
                        div()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.muted.opacity(0.3))
                            .p(px(12.0))
                            .text_size(px(12.0))
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("#{} · {}", item.number, item.title)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(format!(
                                        "{} · {} → {}",
                                        item.repository_name, item.head_branch, item.base_branch
                                    )),
                            ),
                    )
                    .when(ask == Ask::Close, |dialog| {
                        dialog.child(
                            div()
                                .min_h(px(72.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(colors.border)
                                .p(px(8.0))
                                .text_size(px(12.0))
                                .child(self.actions.comment.clone()),
                        )
                    })
                    .when(offers_branch, |dialog| {
                        dialog.child(
                            div()
                                .debug_selector(|| "pr-close-delete-branch".into())
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .text_size(px(12.0))
                                .child(
                                    Switch::new(
                                        "pr-close-delete-branch",
                                        self.actions.delete_branch,
                                    )
                                    .disabled(running)
                                    .on_toggle(
                                        move |on, _, cx| {
                                            let _ = toggle.update(cx, |hub, cx| {
                                                hub.actions.delete_branch = on;
                                                cx.notify();
                                            });
                                        },
                                    ),
                                )
                                .child(cx.t("pullRequests.actions.close.deleteBranch")),
                        )
                    })
                    .children(self.actions.error.clone().map(|error| {
                        div()
                            .text_size(px(11.0))
                            .text_color(tw::RED_500.hsla())
                            .child(error)
                    }))
                    .child(dialog_footer(
                        [
                            Button::new("pr-action-cancel")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Default)
                                .text_size(14.0)
                                .disabled(running)
                                .on_click(cx.listener(|hub, _, _, cx| hub.cancel_action(cx)))
                                .child(cx.t("pullRequests.merge.cancel"))
                                .into_any_element(),
                            Button::new("pr-action-confirm")
                                .variant(if matches!(ask, Ask::Close | Ask::DeleteBranch) {
                                    ButtonVariant::Destructive
                                } else {
                                    ButtonVariant::Default
                                })
                                .size(ButtonSize::Default)
                                .text_size(14.0)
                                .disabled(running)
                                .on_click(cx.listener(|hub, _, _, cx| hub.confirm_action(cx)))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .when(running, |label| {
                                            label.child(spinner("pr-action-running", 14.0))
                                        })
                                        .child(text("confirm", cx)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pull_request_offers_what_its_state_allows() {
        // Open: closed always; updated only when behind its base.
        assert_eq!(offered(false, false, true), [Ask::Close]);
        assert_eq!(offered(false, true, true), [Ask::UpdateBranch, Ask::Close]);
        // Closed: reopened; its branch deleted when it is this repository's.
        assert_eq!(offered(true, true, true), [Ask::Reopen, Ask::DeleteBranch]);
        assert_eq!(offered(true, false, false), [Ask::Reopen]);
    }
}
