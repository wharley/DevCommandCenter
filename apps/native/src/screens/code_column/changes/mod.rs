//! "Mudanças" (`InspectorChangesSection`): what changed in the task's
//! checkout, by scope — "Workspace" (staged and unstaged, with stage,
//! unstage and discard) or "Branch" (the commits against the base) — as
//! cards with their diff or as a tree. The first two cards of a group load
//! at once, the rest near the viewport; a long review (more than 3 files)
//! starts as a list of closed cards.

mod actions;
mod bar;
mod card;
mod load;
#[cfg(test)]
mod tests;
mod tree;
mod view;

pub(crate) use view::changes;

use std::collections::HashSet;
use std::time::Instant;

use dcc_core::domain::workspace_git::{
    WorkspaceGitBranchDiffOutput, WorkspaceGitChangeEntry, WorkspaceGitPreviewScope,
    WorkspaceGitStatusOutput,
};
use gpui::{Entity, ListAlignment, ListState, Task, px};

use super::Loaded;
use crate::screens::diff_view::DiffView;

/// `REVIEW_CARDS_OPEN_LIMIT`
pub const CARDS_OPEN_LIMIT: usize = 3;
/// The IntersectionObserver's `rootMargin: "700px 0px"`: the list draws this
/// far past the viewport, and a card drawn loads its diff.
const LOAD_MARGIN: f32 = 700.0;
/// How long a file new to the branch shimmers (`ShinyFlash`).
pub const FLASH: std::time::Duration = std::time::Duration::from_millis(3000);

/// `InspectorReviewScope` without "last turn" (screen 5h).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Working,
    Branch,
}

impl Scope {
    pub fn key(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Branch => "branch",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeGroup {
    Staged,
    Unstaged,
    /// On the branch, against its base.
    Committed,
}

impl ChangeGroup {
    fn preview_scope(self) -> WorkspaceGitPreviewScope {
        match self {
            Self::Staged => WorkspaceGitPreviewScope::Staged,
            Self::Unstaged => WorkspaceGitPreviewScope::Unstaged,
            Self::Committed => WorkspaceGitPreviewScope::Committed,
        }
    }

    pub fn scope(self) -> Scope {
        match self {
            Self::Committed => Scope::Branch,
            _ => Scope::Working,
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

    /// A conflict: no discard or stage from the list.
    pub fn unmerged(&self) -> bool {
        matches!(
            self.entry.status.to_uppercase().as_str(),
            "U" | "AA" | "DD" | "AU" | "UA" | "DU" | "UD"
        )
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

/// A child of the scrolling list, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChangesRow {
    Group(ChangeGroup),
    Card(usize),
    Folder {
        group: ChangeGroup,
        path: String,
        name: String,
        depth: usize,
        open: bool,
    },
    File {
        card: usize,
        depth: usize,
    },
    /// "origin/main" over the branch's files.
    BranchHeader,
    Space(u8),
    Empty,
    EmptyBranch,
}

pub(crate) struct Changes {
    /// Staged, unstaged, then the branch's, each in git's order.
    pub cards: Vec<ChangeCard>,
    pub staged_open: bool,
    pub unstaged_open: bool,
    /// The header's "expand / collapse all", for one scope, until the
    /// checkout changes.
    pub all_open: Option<(Scope, bool)>,
    /// The scope picked for this checkout (else the default).
    pub picked: Option<Scope>,
    pub tree: bool,
    /// Folders closed in the tree (they start open).
    pub closed: HashSet<(ChangeGroup, String)>,
    pub branch: Loaded<WorkspaceGitBranchDiffOutput>,
    /// Files new to the branch since the last read, and until when.
    pub flashing: HashSet<String>,
    pub flash_until: Option<Instant>,
    /// The tree row under the pointer (its actions show).
    pub hovered_file: Option<usize>,
    /// A git action is running (`gitBusy`).
    pub busy: bool,
    /// "Reverter tudo" asks first.
    pub confirm_revert: bool,
    /// The rows on screen, virtual: only those near the viewport are drawn.
    pub list: ListState,
    /// The rows the list holds, and which of its cards were open when
    /// measured.
    shown: Vec<ChangesRow>,
    measured: Vec<bool>,
    /// When the list last moved (its scrollbar shows a moment).
    pub scrolled_at: Option<Instant>,
    /// The theme the diffs were colored for.
    dark: bool,
}

impl Changes {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            staged_open: true,
            unstaged_open: true,
            all_open: None,
            picked: None,
            tree: false,
            closed: HashSet::new(),
            branch: Loaded::Pending,
            flashing: HashSet::new(),
            flash_until: None,
            hovered_file: None,
            busy: false,
            confirm_revert: false,
            list: ListState::new(0, ListAlignment::Top, px(LOAD_MARGIN)),
            shown: Vec::new(),
            measured: Vec::new(),
            scrolled_at: None,
            dark: false,
        }
    }

    /// Another checkout: its own lists, scope and "open all".
    pub fn reset(&mut self) {
        self.cards.clear();
        self.all_open = None;
        self.picked = None;
        self.closed.clear();
        self.branch = Loaded::Pending;
        self.flashing.clear();
        self.flash_until = None;
        self.hovered_file = None;
        self.busy = false;
        self.confirm_revert = false;
        self.list.reset(0);
        self.shown.clear();
        self.measured.clear();
    }

    pub fn entries(&self, scope: Scope) -> impl Iterator<Item = &WorkspaceGitChangeEntry> {
        self.cards
            .iter()
            .filter(move |card| card.group.scope() == scope)
            .map(|card| &card.entry)
    }

    pub fn has(&self, group: ChangeGroup) -> bool {
        self.cards.iter().any(|card| card.group == group)
    }

    /// The files of the branch, once read (`null` while it reads or failed).
    pub fn branch_count(&self) -> Option<usize> {
        matches!(self.branch, Loaded::Ready(_)).then(|| self.entries(Scope::Branch).count())
    }

    /// `resolveInspectorReviewScope`: the one picked, else "Workspace" while
    /// it has changes or the branch has none.
    pub fn scope(&self) -> Scope {
        self.picked.unwrap_or_else(|| {
            let working = self.entries(Scope::Working).count();
            let branch = self.branch_count().unwrap_or(0);
            if working > 0 || branch == 0 {
                Scope::Working
            } else {
                Scope::Branch
            }
        })
    }

    /// `reviewCardsStartOpen`, unless the header said otherwise.
    pub fn cards_open(&self, scope: Scope) -> bool {
        match self.all_open {
            Some((picked, open)) if picked == scope => open,
            _ => self.entries(scope).count() <= CARDS_OPEN_LIMIT,
        }
    }

    /// Cards mount again (scope, tree or group shown anew): they start as
    /// the list says, as a React card does; their diffs stay read.
    pub fn remount(&mut self, groups: &[ChangeGroup]) {
        for group in groups {
            let open = self.cards_open(group.scope());
            for card in self.cards.iter_mut().filter(|card| card.group == *group) {
                card.open = open;
            }
        }
    }

    /// One group's cards follow a new read: kept ones keep their state (a
    /// changed count reads the diff again), new ones start as the list says.
    fn sync_group(
        &mut self,
        previous: &mut Vec<ChangeCard>,
        group: ChangeGroup,
        entries: &[WorkspaceGitChangeEntry],
        start_open: bool,
        out: &mut Vec<ChangeCard>,
    ) {
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
            out.push(card);
        }
    }

    fn rebuild(
        &mut self,
        staged: &[WorkspaceGitChangeEntry],
        unstaged: &[WorkspaceGitChangeEntry],
        committed: &[WorkspaceGitChangeEntry],
        working_open: bool,
        branch_open: bool,
    ) {
        let mut previous = std::mem::take(&mut self.cards);
        let mut cards = Vec::with_capacity(staged.len() + unstaged.len() + committed.len());
        self.sync_group(
            &mut previous,
            ChangeGroup::Staged,
            staged,
            working_open,
            &mut cards,
        );
        self.sync_group(
            &mut previous,
            ChangeGroup::Unstaged,
            unstaged,
            working_open,
            &mut cards,
        );
        self.sync_group(
            &mut previous,
            ChangeGroup::Committed,
            committed,
            branch_open,
            &mut cards,
        );
        self.cards = cards;
    }

    fn group_entries(&self, group: ChangeGroup) -> Vec<WorkspaceGitChangeEntry> {
        self.cards
            .iter()
            .filter(|card| card.group == group)
            .map(|card| card.entry.clone())
            .collect()
    }

    pub fn sync(&mut self, status: &WorkspaceGitStatusOutput) {
        let total = status.staged.len() + status.unstaged.len();
        let open = match self.all_open {
            Some((Scope::Working, open)) => open,
            _ => total <= CARDS_OPEN_LIMIT,
        };
        let committed = self.group_entries(ChangeGroup::Committed);
        let branch_open = self.cards_open(Scope::Branch);
        self.rebuild(
            &status.staged,
            &status.unstaged,
            &committed,
            open,
            branch_open,
        );
    }

    /// The branch read again: its cards follow, and files new to it shimmer.
    pub fn sync_branch(&mut self, diff: WorkspaceGitBranchDiffOutput, now: Instant) {
        let before: Option<HashSet<String>> = matches!(self.branch, Loaded::Ready(_)).then(|| {
            self.entries(Scope::Branch)
                .map(|entry| entry.path.clone())
                .collect()
        });
        if let Some(before) = before {
            let new: HashSet<String> = diff
                .changes
                .iter()
                .filter(|entry| !before.contains(&entry.path))
                .map(|entry| entry.path.clone())
                .collect();
            if !new.is_empty() {
                self.flashing = new;
                self.flash_until = Some(now + FLASH);
            }
        }
        let open = match self.all_open {
            Some((Scope::Branch, open)) => open,
            _ => diff.changes.len() <= CARDS_OPEN_LIMIT,
        };
        let (staged, unstaged) = (
            self.group_entries(ChangeGroup::Staged),
            self.group_entries(ChangeGroup::Unstaged),
        );
        let working_open = self.cards_open(Scope::Working);
        self.rebuild(&staged, &unstaged, &diff.changes, working_open, open);
        self.branch = Loaded::Ready(diff);
    }

    pub fn is_flashing(&self, path: &str) -> bool {
        self.flash_until.is_some_and(|until| Instant::now() < until) && self.flashing.contains(path)
    }

    pub fn base_branch(&self) -> Option<String> {
        match &self.branch {
            Loaded::Ready(diff) => diff.base_branch.clone(),
            _ => None,
        }
    }

    pub fn set_all_open(&mut self, scope: Scope, open: bool) {
        self.all_open = Some((scope, open));
        for card in self
            .cards
            .iter_mut()
            .filter(|card| card.group.scope() == scope)
        {
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

    /// The list's rows for the scope shown: groups and their cards (or
    /// tree), the branch's header and files, or the empty state.
    pub fn rows(&self) -> Vec<ChangesRow> {
        let mut rows = Vec::new();
        match self.scope() {
            Scope::Working => {
                for (group, open) in [
                    (ChangeGroup::Staged, self.staged_open),
                    (ChangeGroup::Unstaged, self.unstaged_open),
                ] {
                    if !self.has(group) {
                        continue;
                    }
                    rows.push(ChangesRow::Group(group));
                    if open {
                        self.group_rows(group, &mut rows);
                    }
                }
                if rows.is_empty() {
                    rows.push(ChangesRow::Empty);
                }
            }
            Scope::Branch => {
                let loading = matches!(self.branch, Loaded::Pending);
                if !loading && !self.has(ChangeGroup::Committed) {
                    rows.push(ChangesRow::EmptyBranch);
                } else {
                    rows.push(ChangesRow::BranchHeader);
                    rows.push(ChangesRow::Space(4));
                    self.group_rows(ChangeGroup::Committed, &mut rows);
                    rows.push(ChangesRow::Space(8));
                }
            }
        }
        rows
    }

    fn group_rows(&self, group: ChangeGroup, rows: &mut Vec<ChangesRow>) {
        if self.tree {
            tree::rows(self, group, rows);
            rows.push(ChangesRow::Space(8));
        } else {
            rows.extend(
                self.cards
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| card.group == group)
                    .map(|(at, _)| ChangesRow::Card(at)),
            );
        }
    }

    /// The eager cards (the first two of a group) still to load.
    pub fn due(&self, rows: &[ChangesRow]) -> Vec<usize> {
        rows.iter()
            .filter_map(|row| match row {
                ChangesRow::Card(at) => Some(*at),
                _ => None,
            })
            .filter(|at| self.waits(*at) && self.cards[*at].eager())
            .collect()
    }

    pub fn shown_row(&self, index: usize) -> Option<ChangesRow> {
        self.shown.get(index).cloned()
    }

    /// Open, and its diff not read yet.
    pub fn waits(&self, at: usize) -> bool {
        self.cards
            .get(at)
            .is_some_and(|card| card.open && matches!(card.diff, CardDiff::Idle))
    }

    /// The list follows the rows: only where they changed, so it keeps its
    /// place; heights are measured again where a card opened or closed.
    pub fn sync_list(&mut self) {
        let next = self.rows();
        if next != self.shown {
            let keep_start = self
                .shown
                .iter()
                .zip(&next)
                .take_while(|(a, b)| a == b)
                .count();
            let keep_end = self.shown[keep_start..]
                .iter()
                .rev()
                .zip(next[keep_start..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            let removed = keep_start..self.shown.len() - keep_end;
            self.list
                .splice(removed, next.len() - keep_end - keep_start);
            self.shown = next;
        }
        let opened: Vec<bool> = self
            .shown
            .iter()
            .map(|row| match row {
                ChangesRow::Card(at) => self.cards[*at].open,
                _ => false,
            })
            .collect();
        if opened != self.measured {
            for (row, _) in opened
                .iter()
                .zip(self.measured.iter().chain(std::iter::repeat(&false)))
                .enumerate()
                .filter(|(_, (now, before))| now != before)
            {
                self.list.remeasure_items(row..row + 1);
            }
            self.measured = opened;
        }
    }
}
