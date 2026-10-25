//! What the list reads: each card's diff when it comes near the viewport,
//! and the branch against its base (every ten seconds while "Mudanças"
//! shows). Plus the toggles that change which cards are on screen.

use gpui::{AppContext, Context};

use super::super::{CodeColumn, CodeColumnTab, Loaded};
use super::{CardDiff, ChangeGroup, Scope};
use crate::screens::diff_view::DiffView;

impl CodeColumn {
    pub(crate) fn toggle_card(&mut self, at: usize, cx: &mut Context<Self>) {
        if let Some(card) = self.changes.cards.get_mut(at) {
            card.open = !card.open;
            cx.notify();
        }
    }

    pub(crate) fn toggle_all_cards(&mut self, cx: &mut Context<Self>) {
        let scope = self.review_scope();
        if scope == Scope::LastTurn {
            return self.toggle_turn_cards(cx);
        }
        let open = !self.changes.cards_open(scope);
        self.changes.set_all_open(scope, open);
        cx.notify();
    }

    pub(crate) fn toggle_group(&mut self, group: ChangeGroup, cx: &mut Context<Self>) {
        let open = match group {
            ChangeGroup::Staged => &mut self.changes.staged_open,
            ChangeGroup::Unstaged => &mut self.changes.unstaged_open,
            ChangeGroup::Committed => return,
        };
        *open = !*open;
        if *open {
            self.changes.remount(&[group]);
        }
        cx.notify();
    }

    pub(crate) fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        if self.review_scope() != scope {
            self.changes.picked = Some(scope);
            let groups: &[ChangeGroup] = match scope {
                Scope::Working => &[ChangeGroup::Staged, ChangeGroup::Unstaged],
                Scope::LastTurn => &[],
                Scope::Branch => &[ChangeGroup::Committed],
            };
            self.changes.remount(groups);
            cx.notify();
        } else if self.changes.picked.is_none() {
            self.changes.picked = Some(scope);
        }
    }

    pub(crate) fn toggle_changes_tree(&mut self, cx: &mut Context<Self>) {
        self.changes.tree = !self.changes.tree;
        self.changes.closed.clear();
        if !self.changes.tree {
            self.changes.remount(&[
                ChangeGroup::Staged,
                ChangeGroup::Unstaged,
                ChangeGroup::Committed,
            ]);
        }
        cx.notify();
    }

    pub(crate) fn toggle_changes_folder(
        &mut self,
        group: ChangeGroup,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        let key = (group, path.to_string());
        if !self.changes.closed.remove(&key) {
            self.changes.closed.insert(key);
        }
        cx.notify();
    }

    /// Shows the scrollbar and hides it once the list rests.
    pub(crate) fn changes_scrolled(&mut self, cx: &mut Context<Self>) {
        self.changes.scrolled_at = Some(std::time::Instant::now());
        self._scrollbar_fade = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(
                    crate::ui::scrollbar::SCROLLBAR_LINGER + std::time::Duration::from_millis(50),
                )
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        }));
    }

    /// The first two cards of each group shown load without waiting to be
    /// drawn (`shouldEagerLoadReviewCard`).
    pub(crate) fn load_eager_diffs(&mut self, cx: &mut Context<Self>) {
        for at in self.changes.due(&self.changes.rows()) {
            self.load_diff(at, cx);
        }
    }

    /// Reads one card's diff (its two sides, then the diff off the UI thread).
    pub(crate) fn load_diff(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(card) = self.changes.cards.get(at) else {
            return;
        };
        let (group, entry, dark) = (card.group, card.entry.clone(), self.changes.dark());
        let base = (group == ChangeGroup::Committed)
            .then(|| self.changes.base_branch())
            .flatten();
        let task = cx.spawn(async move |this, cx| {
            let (path, status) = (entry.path.clone(), entry.status.clone());
            let read_root = root.clone();
            let content = cx
                .background_executor()
                .spawn(async move {
                    dcc_infra::workspace_file_preview::file_preview_content(
                        &read_root,
                        &path,
                        &status,
                        &group.preview_scope(),
                        base.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.root.as_deref() != Some(root.as_str()) {
                    return;
                }
                let Some(card) = column
                    .changes
                    .cards
                    .iter_mut()
                    .find(|card| card.same(group, &entry))
                else {
                    return;
                };
                if !matches!(card.diff, CardDiff::Loading(_)) {
                    return;
                }
                card.diff = match content {
                    Ok((original, modified, _)) => {
                        let path = entry.path.clone();
                        CardDiff::Ready(
                            cx.new(|cx| DiffView::new(path, original, modified, dark, cx)),
                        )
                    }
                    Err(error) => CardDiff::Failed(error),
                };
                cx.notify();
            });
        });
        if let Some(card) = self.changes.cards.get_mut(at) {
            card.diff = CardDiff::Loading(task);
        }
    }

    /// `useWorkspaceGitBranchDiff`: the files the branch changed against its
    /// base, while "Mudanças" shows.
    pub(crate) fn read_branch(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.tab != CodeColumnTab::Changes {
            return;
        }
        let db_path = self.db_path.clone();
        self._branch = Some(cx.spawn(async move |this, cx| {
            let path = root.clone();
            let diff = cx
                .background_executor()
                .spawn(async move {
                    dcc_infra::workspace_git::workspace_git_branch_diff(&db_path, &path).await
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.root.as_deref() != Some(root.as_str()) {
                    return;
                }
                match diff {
                    Ok(diff) => {
                        column.changes.sync_branch(diff, std::time::Instant::now());
                        if !column.changes.flashing.is_empty() {
                            // The shimmer stops by itself after its three sweeps.
                            cx.spawn(async move |this, cx| {
                                cx.background_executor().timer(super::FLASH).await;
                                let _ = this.update(cx, |_, cx| cx.notify());
                            })
                            .detach();
                        }
                    }
                    // The desktop shows "—" and the empty branch on a failed read.
                    Err(error) => {
                        if matches!(column.changes.branch, Loaded::Pending) {
                            column.changes.branch = Loaded::Failed(error);
                        }
                    }
                }
                cx.notify();
            });
        }));
    }
}
