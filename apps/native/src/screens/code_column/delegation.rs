//! Reviewing an isolated delegation in "Mudanças": an implementation a
//! child agent left in its own worktree ("Delegar implementação" on a
//! plan). The session's notices offer "Ver mudanças"; while one is under
//! review the list shows the worktree's changes (no delivery from there),
//! and the bar applies them onto the task's checkout, discards them, or goes
//! back. Opening the child's conversation reviews it too.

use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use super::{CodeColumn, CodeColumnEvent};
use crate::data::conversation::Conversation;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::{Args, Translate};
use crate::project::provider_label;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Icon, Toaster, spinner};

/// A delegation whose worktree waits for the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reviewable {
    pub id: String,
    pub provider_id: String,
    pub parent_session_id: String,
    pub child_session_id: Option<String>,
    pub touched_files: usize,
    pub worktree: String,
}

impl Reviewable {
    /// `providerLabelForDelegation`
    pub fn label(&self) -> String {
        provider_label(&self.provider_id).map_or_else(|| self.provider_id.clone(), str::to_string)
    }
}

/// The conversation's delegations that wait for review.
pub fn reviewables(conversation: &Conversation) -> Vec<Reviewable> {
    let mut found: Vec<Reviewable> = conversation
        .delegations
        .iter()
        .filter(|(_, record)| record.awaits_review())
        .filter_map(|(id, record)| {
            Some(Reviewable {
                id: id.clone(),
                provider_id: record.provider_id.clone(),
                parent_session_id: record.parent_session_id.clone(),
                child_session_id: record.child_session_id.clone(),
                touched_files: record.touched_files.len(),
                worktree: record.worktree.clone()?,
            })
        })
        .collect();
    found.sort_by(|left, right| left.id.cmp(&right.id));
    found
}

pub(crate) struct DelegationReview {
    pub delegation: Reviewable,
    /// The task's checkout, back when the review ends.
    pub parent_root: String,
    /// Opened from the child's conversation (ending it goes to the parent's).
    pub from_child: bool,
    pub busy: bool,
}

impl CodeColumn {
    /// The open conversation and its delegations waiting for review; the
    /// child's own conversation opens its review.
    pub fn set_reviews(
        &mut self,
        session: Option<String>,
        found: Vec<Reviewable>,
        cx: &mut Context<Self>,
    ) {
        let session_changed = session != self.session_id;
        self.session_id = session;
        self.reviewables = found;
        let still_waiting = |review: &DelegationReview, reviewables: &[Reviewable]| {
            reviewables
                .iter()
                .any(|item| item.id == review.delegation.id)
        };
        if let Some(review) = &self.delegation_review
            && (session_changed || !still_waiting(review, &self.reviewables))
            && !review.busy
        {
            self.end_review(cx);
        }
        if self.delegation_review.is_none()
            && let Some(child) = self.reviewables.iter().find(|item| {
                item.child_session_id.is_some() && item.child_session_id == self.session_id
            })
        {
            let child = child.clone();
            self.start_review(child, true, cx);
        }
        cx.notify();
    }

    /// "Ver mudanças" / "Revisar mudanças": the delegation's worktree in the list.
    /// Anything else only shows the changes (the review, if any, ends).
    pub fn review_delegation(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.reviewables.iter().find(|item| item.id == id).cloned() {
            Some(item) => self.start_review(item, false, cx),
            None => self.end_review(cx),
        }
    }

    fn start_review(&mut self, delegation: Reviewable, from_child: bool, cx: &mut Context<Self>) {
        let parent_root = match &self.delegation_review {
            Some(review) => review.parent_root.clone(),
            None => match self.root.clone() {
                Some(root) => root,
                None => return,
            },
        };
        let worktree = delegation.worktree.clone();
        self.delegation_review = Some(DelegationReview {
            delegation,
            parent_root,
            from_child,
            busy: false,
        });
        self.switch_root(Some(worktree), cx);
        self.set_tab(super::CodeColumnTab::Changes, cx);
    }

    /// Back to the task's checkout.
    pub(crate) fn end_review(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.delegation_review.take() else {
            return;
        };
        self.switch_root(Some(review.parent_root), cx);
        if review.from_child {
            cx.emit(CodeColumnEvent::OpenSession(
                review.delegation.parent_session_id,
            ));
        }
        cx.notify();
    }

    /// "Aplicar" (`handleApplyDelegation`) or "Descartar" (`handleDiscardDelegation`).
    pub(crate) fn settle_delegation(&mut self, apply: bool, cx: &mut Context<Self>) {
        let Some(review) = self.delegation_review.as_mut() else {
            return;
        };
        if review.busy {
            return;
        }
        review.busy = true;
        let (id, root) = (review.delegation.id.clone(), review.parent_root.clone());
        let api = self
            .endpoint
            .clone()
            .or_else(DesktopApi::load)
            .ok_or(ApiError::DesktopClosed);
        cx.notify();
        self._delegation = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    if apply {
                        api.apply_delegation(&id, &root).map(Some)
                    } else {
                        api.discard_delegation(&id, &root).map(|()| None)
                    }
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                if let Some(review) = column.delegation_review.as_mut() {
                    review.busy = false;
                }
                match result {
                    Ok(applied) => {
                        let message = match applied {
                            Some(count) => cx.t_with(
                                "inspector.delegations.applied",
                                &Args::count(count as i64),
                            ),
                            None => cx.t("inspector.delegations.discarded"),
                        };
                        Toaster::success(message, cx);
                        column.end_review(cx);
                        column.refresh_git(cx);
                    }
                    Err(ApiError::DesktopClosed) => {
                        Toaster::error(cx.t("native.desktopClosed"), cx)
                    }
                    Err(error) => Toaster::error(error.to_string(), cx),
                }
                cx.notify();
            });
        }));
    }
}

fn amber_text(cx: &Context<CodeColumn>) -> Hsla {
    if cx.theme().appearance == Appearance::Dark {
        tw::AMBER_200
    } else {
        tw::AMBER_800
    }
    .hsla()
}

/// `rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5`
fn strip() -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .overflow_hidden()
        .rounded(px(6.0))
        .border_1()
        .border_color(tw::AMBER_500.alpha(0.3))
        .bg(tw::AMBER_500.alpha(0.1))
        .px(px(8.0))
        .py(px(6.0))
}

/// `Button size="xs" className="h-6 px-2 text-[11px]"`
fn small_button(
    id: SharedString,
    label: SharedString,
    kind: &'static str,
    disabled: bool,
    cx: &Context<CodeColumn>,
) -> gpui::Stateful<gpui::Div> {
    let colors = cx.theme().colors.clone();
    let (background, text, hover) = match kind {
        "default" => (
            Some(colors.primary),
            colors.primary_foreground,
            Some(colors.primary.opacity(0.8)),
        ),
        "destructive" => (
            Some(colors.destructive.opacity(0.1)),
            colors.destructive,
            Some(colors.destructive.opacity(0.2)),
        ),
        _ => (None, colors.foreground, Some(colors.muted)),
    };
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .flex()
        .h(px(24.0))
        .flex_shrink_0()
        .items_center()
        .rounded(px(6.0))
        .px(px(8.0))
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(text)
        .when_some(background, |button, background| button.bg(background))
        .map(|button| {
            if disabled {
                button.opacity(0.5)
            } else {
                button.cursor_pointer().when_some(hover, |button, hover| {
                    button.hover(move |style| style.bg(hover))
                })
            }
        })
        .child(label)
}

/// The session's notices (no review open) or the review's bar.
pub(crate) fn delegation_rows(
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> Vec<AnyElement> {
    let text = amber_text(cx);
    let fork = || Icon::new("git-fork").size(14.0).stroke(2.0).color(text);
    if let Some(review) = &column.delegation_review {
        let busy = review.busy;
        let label = SharedString::from(review.delegation.label());
        let files = cx.t_with(
            "inspector.delegations.files",
            &Args::count(review.delegation.touched_files as i64),
        );
        let bar = strip()
            .debug_selector(|| "delegation-review".into())
            .child(fork())
            .child(
                div().flex_1().min_w_0().child(
                    div()
                        .truncate()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(text)
                        .child(cx.t_with(
                            "inspector.delegations.reviewingOutputCompact",
                            &Args::default().with("provider", label.to_string()),
                        )),
                ),
            )
            .child(
                div()
                    .flex()
                    .h(px(24.0))
                    .flex_shrink_0()
                    .items_center()
                    .rounded_full()
                    .border_1()
                    .border_color(tw::AMBER_500.alpha(0.3))
                    .bg(cx.theme().colors.background.opacity(0.4))
                    .px(px(6.0))
                    .text_size(px(10.0))
                    .text_color(text)
                    .child(files),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(4.0))
                    .when(busy, |row| row.child(spinner("delegation-busy", 14.0)))
                    .child(
                        small_button(
                            "delegation-apply".into(),
                            cx.t("inspector.delegations.applyShort"),
                            "default",
                            busy,
                            cx,
                        )
                        .when(!busy, |button| {
                            button.on_click(
                                cx.listener(|column, _, _, cx| column.settle_delegation(true, cx)),
                            )
                        }),
                    )
                    .child(
                        small_button(
                            "delegation-discard".into(),
                            cx.t("inspector.delegations.discardShort"),
                            "destructive",
                            busy,
                            cx,
                        )
                        .when(!busy, |button| {
                            button.on_click(
                                cx.listener(|column, _, _, cx| column.settle_delegation(false, cx)),
                            )
                        }),
                    )
                    .child(
                        small_button(
                            "delegation-back".into(),
                            cx.t("inspector.delegations.backShort"),
                            "ghost",
                            busy,
                            cx,
                        )
                        .when(!busy, |button| {
                            button.on_click(cx.listener(|column, _, _, cx| column.end_review(cx)))
                        }),
                    ),
            );
        return vec![bar.into_any_element()];
    }
    let Some(session) = column.session_id.clone() else {
        return Vec::new();
    };
    column
        .reviewables
        .iter()
        .filter(|item| item.parent_session_id == session)
        .map(|item| {
            let id = item.id.clone();
            strip()
                .child(fork())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(text)
                        .child(cx.t_with(
                            "inspector.delegations.pendingFromSession",
                            &Args::count(item.touched_files as i64).with("agent", item.label()),
                        )),
                )
                .child(
                    small_button(
                        SharedString::from(format!("delegation-show-{id}")),
                        cx.t("inspector.delegations.showPendingChanges"),
                        "default",
                        false,
                        cx,
                    )
                    .on_click(
                        cx.listener(move |column, _, _, cx| column.review_delegation(&id, cx)),
                    ),
                )
                .into_any_element()
        })
        .collect()
}
