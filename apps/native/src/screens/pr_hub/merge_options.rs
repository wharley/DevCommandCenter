//! What the merge of a GitHub pull request offers besides its method: what
//! it still waits on, merging when it becomes ready (GitHub's auto-merge,
//! and turning it off), and deleting its branch afterwards. Each is the
//! person's choice in the dialog; nothing here acts by itself.

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::PrHub;
use crate::data::desktop_api::ApiError;
use crate::data::pr_hub::{Item, MergeCapabilities};
use crate::data::pr_merge_readiness::{MergePath, PrAction, merge_path};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Switch, Toaster, spinner};

impl PrHub {
    pub(super) fn merge_capabilities(&self, item: &Item) -> Option<&MergeCapabilities> {
        self.merge_detail(item)?
            .as_ref()
            .ok()?
            .merge_capabilities
            .as_ref()
    }

    /// Merge now, when ready, or held; "now" while GitHub's answer is read.
    pub(super) fn merge_path(&self, item: &Item) -> MergePath {
        self.merge_capabilities(item)
            .map_or(MergePath::Now, |merge| merge_path(item, merge))
    }

    /// The branch goes with the merge only when asked, merging now, and it
    /// is this repository's own.
    pub(super) fn deletes_branch(&self, item: &Item) -> bool {
        self.merge_delete_branch
            && self.merge_path(item) == MergePath::Now
            && self
                .merge_capabilities(item)
                .is_some_and(|merge| merge.can_delete_branch)
    }

    /// The toast after the forge's answer.
    pub(super) fn merge_result_key(
        status: &str,
        branch_deleted: bool,
        when_ready: bool,
    ) -> &'static str {
        match (status, branch_deleted, when_ready) {
            ("merged", true, _) => "pullRequests.merge.successDeleted",
            ("merged", false, _) => "pullRequests.merge.success",
            (_, _, true) => "pullRequests.merge.whenReadySet",
            ("queued", _, _) => "pullRequests.merge.queued",
            _ => "pullRequests.merge.submitted",
        }
    }

    /// In the dialog: what it waits on, and what confirming will do then.
    pub(super) fn merge_wait_note(
        &self,
        item: &Item,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (wait, then) = match self.merge_path(item) {
            MergePath::Now => return None,
            MergePath::WhenReady(wait) => (wait, "pullRequests.merge.whenReadyNote"),
            MergePath::Held(wait) => (wait, "pullRequests.merge.heldNote"),
        };
        Some(
            div()
                .debug_selector(|| "pr-merge-wait".into())
                .flex()
                .gap(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(tw::AMBER_500.alpha(0.35))
                .bg(tw::AMBER_500.alpha(0.08))
                .p(px(10.0))
                .text_size(px(11.0))
                .line_height(px(16.0))
                .child(
                    Icon::new("clock-3")
                        .size(14.0)
                        .stroke(2.0)
                        .color(tw::AMBER_500.hsla()),
                )
                .child(div().flex_1().min_w_0().child(format!(
                    "{} {}",
                    cx.t(wait.key()),
                    cx.t(then)
                )))
                .into_any_element(),
        )
    }

    /// In the dialog: "Apagar o branch depois do merge".
    pub(super) fn delete_branch_row(
        &self,
        item: &Item,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let offered = self.merge_path(item) == MergePath::Now
            && self
                .merge_capabilities(item)
                .is_some_and(|merge| merge.can_delete_branch);
        if !offered {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let this = cx.entity().downgrade();
        Some(
            div()
                .debug_selector(|| "pr-merge-delete-branch".into())
                .flex()
                .items_center()
                .gap(px(10.0))
                .text_size(px(12.0))
                .child(
                    Switch::new("pr-merge-delete-branch", self.merge_delete_branch)
                        .disabled(self.merging)
                        .on_toggle(move |on, _, cx| {
                            let _ = this.update(cx, |hub, cx| {
                                hub.merge_delete_branch = on;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(cx.t("pullRequests.merge.deleteBranch"))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .font_family(cx.theme().fonts.mono.clone())
                                .child(item.head_branch.clone()),
                        ),
                )
                .into_any_element(),
        )
    }

    /// In place of "Mesclar" while GitHub will merge it when ready: says so,
    /// with the way out.
    pub(super) fn auto_merge_button(
        &self,
        item: &Item,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self
            .merge_capabilities(item)
            .is_some_and(|merge| merge.auto_merge_enabled)
        {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let working = self.merging;
        Some(
            div()
                .debug_selector(|| "pr-auto-merge".into())
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .child(
                            Icon::new("clock-3")
                                .size(14.0)
                                .stroke(2.0)
                                .color(tw::AMBER_500.hsla()),
                        )
                        .child(cx.t("pullRequests.merge.whenReadyOn")),
                )
                .child(
                    Button::new("pr-auto-merge-cancel")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .text_size(12.0)
                        .disabled(working)
                        .on_click(cx.listener(|hub, _, _, cx| hub.disable_auto_merge(cx)))
                        .child(if working {
                            spinner("pr-auto-merge-cancelling", 14.0).into_any_element()
                        } else {
                            div().into_any_element()
                        })
                        .child(cx.t("pullRequests.merge.whenReadyCancel")),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn disable_auto_merge(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        if self.merging {
            return;
        }
        self.merging = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { api.pr_hub_action(&item, &PrAction::DisableAutoMerge) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.merging = false;
                match done {
                    Ok(_) => {
                        Toaster::success(cx.t("pullRequests.merge.whenReadyOff"), cx);
                        hub.detail = None;
                        hub.ensure_detail(cx);
                    }
                    Err(ApiError::Failed(message)) if !message.is_empty() => {
                        Toaster::error(message, cx)
                    }
                    Err(_) => Toaster::error(cx.t("pullRequests.merge.error").to_string(), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::PrHub;

    #[test]
    fn the_toast_says_what_the_forge_did() {
        let key = PrHub::merge_result_key;
        assert_eq!(
            key("merged", true, false),
            "pullRequests.merge.successDeleted"
        );
        assert_eq!(key("merged", false, false), "pullRequests.merge.success");
        // Asked to merge when ready and it was ready: merged, said as such.
        assert_eq!(key("merged", false, true), "pullRequests.merge.success");
        assert_eq!(
            key("queued", false, true),
            "pullRequests.merge.whenReadySet"
        );
        assert_eq!(
            key("submitted", false, true),
            "pullRequests.merge.whenReadySet"
        );
        assert_eq!(key("queued", false, false), "pullRequests.merge.queued");
    }
}
