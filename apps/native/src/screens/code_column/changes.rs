//! "Mudanças" (`InspectorChangesSection` on the working scope): the staged
//! and unstaged files as cards, each opening on its own diff. The first two
//! cards of a group load at once, the rest as they come near the viewport;
//! a long review (more than 3 files) starts as a list of closed cards.

use dcc_core::domain::workspace_git::{
    WorkspaceGitChangeEntry, WorkspaceGitPreviewScope, WorkspaceGitStatusOutput,
};
use gpui::{AppContext, Bounds, Context, Entity, Pixels, ScrollHandle, Task, Window, px};

use super::CodeColumn;
use super::changes_view::ChangesRow;
use crate::screens::diff_view::DiffView;

/// `REVIEW_CARDS_OPEN_LIMIT`
pub const CARDS_OPEN_LIMIT: usize = 3;
/// The IntersectionObserver's `rootMargin: "700px 0px"`.
const LOAD_MARGIN: f32 = 700.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeGroup {
    Staged,
    Unstaged,
}

impl ChangeGroup {
    fn scope(self) -> WorkspaceGitPreviewScope {
        match self {
            Self::Staged => WorkspaceGitPreviewScope::Staged,
            Self::Unstaged => WorkspaceGitPreviewScope::Unstaged,
        }
    }
}

pub(crate) enum CardDiff {
    Idle,
    Loading(#[allow(dead_code)] Task<()>),
    Ready(Entity<DiffView>),
    Failed(String),
}

pub(crate) struct ChangeCard {
    pub group: ChangeGroup,
    pub entry: WorkspaceGitChangeEntry,
    /// Its place in the group: the first two load without waiting.
    pub index: usize,
    pub open: bool,
    pub diff: CardDiff,
}

impl ChangeCard {
    /// The card's key (`${group}-${path}-${status}`): a new status is a new card.
    fn same(&self, group: ChangeGroup, entry: &WorkspaceGitChangeEntry) -> bool {
        self.group == group && self.entry.path == entry.path && self.entry.status == entry.status
    }

    pub fn eager(&self) -> bool {
        self.index < 2
    }
}

/// `summarizeInspectorChanges`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub files: usize,
    pub insertions: u32,
    pub deletions: u32,
}

pub fn summarize<'a>(entries: impl IntoIterator<Item = &'a WorkspaceGitChangeEntry>) -> Summary {
    entries
        .into_iter()
        .fold(Summary::default(), |summary, entry| Summary {
            files: summary.files + 1,
            insertions: summary.insertions + entry.insertions,
            deletions: summary.deletions + entry.deletions,
        })
}

/// `reviewCardDiffHeight`: small diffs stay readable, a big file doesn't take
/// the whole list (it scrolls inside its card).
pub fn card_diff_height(insertions: u32, deletions: u32) -> f32 {
    let changed = (insertions + deletions) as f32;
    (190.0 + changed * 18.0).clamp(190.0, 520.0)
}

#[derive(Default)]
pub(crate) struct Changes {
    pub cards: Vec<ChangeCard>,
    pub staged_open: bool,
    pub unstaged_open: bool,
    /// The header's "expand / collapse all", until the checkout changes.
    pub all_open: Option<bool>,
    pub scroll: ScrollHandle,
    /// The theme the diffs were colored for.
    dark: bool,
}

impl Changes {
    pub fn new() -> Self {
        Self {
            staged_open: true,
            unstaged_open: true,
            ..Self::default()
        }
    }

    pub fn reset(&mut self) {
        self.cards.clear();
        self.all_open = None;
        self.scroll = ScrollHandle::new();
    }

    /// `reviewCardsStartOpen`, unless the header said otherwise.
    pub fn cards_open(&self) -> bool {
        self.all_open
            .unwrap_or(self.cards.len() <= CARDS_OPEN_LIMIT)
    }

    /// Cards follow the status: kept ones keep their state (a changed count
    /// reads the diff again), new ones start as the list says.
    pub fn sync(&mut self, status: &WorkspaceGitStatusOutput) {
        let total = status.staged.len() + status.unstaged.len();
        let start_open = self.all_open.unwrap_or(total <= CARDS_OPEN_LIMIT);
        let mut previous = std::mem::take(&mut self.cards);
        let groups = [
            (ChangeGroup::Staged, &status.staged),
            (ChangeGroup::Unstaged, &status.unstaged),
        ];
        for (group, entries) in groups {
            for (index, entry) in entries.iter().enumerate() {
                let card = match previous.iter().position(|card| card.same(group, entry)) {
                    Some(at) => {
                        let mut card = previous.swap_remove(at);
                        if card.entry.insertions != entry.insertions
                            || card.entry.deletions != entry.deletions
                        {
                            card.diff = CardDiff::Idle;
                        }
                        card.entry = entry.clone();
                        card.index = index;
                        card
                    }
                    None => ChangeCard {
                        group,
                        entry: entry.clone(),
                        index,
                        open: start_open,
                        diff: CardDiff::Idle,
                    },
                };
                self.cards.push(card);
            }
        }
    }

    pub fn set_all_open(&mut self, open: bool) {
        self.all_open = Some(open);
        for card in &mut self.cards {
            card.open = open;
        }
    }

    /// The diffs are colored for one theme: another one reads them again.
    pub fn follow_theme(&mut self, dark: bool) {
        if self.dark != dark {
            self.dark = dark;
            for card in &mut self.cards {
                card.diff = CardDiff::Idle;
            }
        }
    }

    pub fn dark(&self) -> bool {
        self.dark
    }

    /// Open cards still to load: the eager ones, or near the viewport by
    /// their bounds in the last frame (`child` maps a card to its row).
    pub fn due(&self, child: impl Fn(usize) -> Option<usize>) -> Vec<usize> {
        let viewport = self.scroll.bounds();
        let near = Bounds {
            origin: viewport.origin - gpui::point(px(0.0), px(LOAD_MARGIN)),
            size: viewport.size + gpui::size(Pixels::ZERO, px(2.0 * LOAD_MARGIN)),
        };
        self.cards
            .iter()
            .enumerate()
            .filter(|(_, card)| card.open && matches!(card.diff, CardDiff::Idle))
            .filter(|(at, card)| {
                card.eager()
                    || child(*at)
                        .and_then(|row| self.scroll.bounds_for_item(row))
                        .is_some_and(|bounds| bounds.intersects(&near))
            })
            .map(|(at, _)| at)
            .collect()
    }

    pub fn has_waiting(&self) -> bool {
        self.cards
            .iter()
            .any(|card| card.open && matches!(card.diff, CardDiff::Idle))
    }
}

impl CodeColumn {
    pub(crate) fn toggle_card(&mut self, at: usize, cx: &mut Context<Self>) {
        if let Some(card) = self.changes.cards.get_mut(at) {
            card.open = !card.open;
            cx.notify();
        }
    }

    pub(crate) fn toggle_all_cards(&mut self, cx: &mut Context<Self>) {
        let open = !self.changes.cards_open();
        self.changes.set_all_open(open);
        cx.notify();
    }

    pub(crate) fn toggle_group(&mut self, group: ChangeGroup, cx: &mut Context<Self>) {
        match group {
            ChangeGroup::Staged => self.changes.staged_open = !self.changes.staged_open,
            ChangeGroup::Unstaged => self.changes.unstaged_open = !self.changes.unstaged_open,
        }
        cx.notify();
    }

    /// Starts the diffs that are due by the last frame's layout.
    pub(crate) fn load_due_diffs(&mut self, cx: &mut Context<Self>) {
        let rows = self.changes_rows();
        let due = self
            .changes
            .due(|card| rows.iter().position(|row| *row == ChangesRow::Card(card)));
        for at in due {
            self.load_diff(at, cx);
        }
    }

    /// Each frame of the list: what is due now, and once more after this
    /// frame lays the cards out (a card's place is known only then).
    pub(crate) fn load_diffs_in_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load_due_diffs(cx);
        if self.changes.has_waiting() {
            let entity = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                let _ = entity.update(cx, |column, cx| column.load_due_diffs(cx));
            });
        }
    }

    fn load_diff(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(card) = self.changes.cards.get(at) else {
            return;
        };
        let (group, entry, dark) = (card.group, card.entry.clone(), self.changes.dark());
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
                        &group.scope(),
                        None,
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
}
