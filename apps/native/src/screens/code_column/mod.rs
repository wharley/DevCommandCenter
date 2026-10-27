//! The code column next to the sidebar (`code-column.tsx`): "Arquivos" and
//! "Mudanças" over the task's checkout. Lists only: a file picked here opens
//! in the center of the workbench.

mod changes;
mod delegation;
pub(crate) mod delivery;
mod recap;
pub(crate) mod review;
mod tree;
mod turn;
mod view;

pub use delegation::{Reviewable, reviewables};
pub use tree::DraggedFile;
#[cfg(test)]
mod delegation_tests;
#[cfg(test)]
mod recap_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod turn_tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use dcc_core::domain::workspace_git::WorkspaceGitStatusOutput;
use gpui::{Context, EventEmitter, ListAlignment, ListState, Task, px};

use crate::data::code_tree::{
    CodeTreeNode, CodeTreeRow, TreeStatus, ancestor_folder_paths, build_code_tree,
    file_set_signature, git_status, visible_rows,
};

/// `GIT_INSPECTOR_REFETCH_INTERVAL_MS`
const GIT_EVERY: Duration = Duration::from_secs(10);

/// `CodeColumnTab`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeColumnTab {
    #[default]
    Files,
    Changes,
}

impl CodeColumnTab {
    /// `CODE_COLUMN_WIDTH`: files are a narrow list; changes carry stats.
    pub fn width(self) -> f32 {
        match self {
            Self::Files => 260.0,
            Self::Changes => 340.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CodeColumnEvent {
    /// A file to open in the center.
    OpenFile(String),
    /// "Mencionar no chat": `@path` in the composer.
    Mention(String),
    /// A request for the agent after the composer's text (`onPrefillComposer`),
    /// and what to say once it is there.
    AskAgent {
        prompt: String,
        delivered: gpui::SharedString,
    },
    Close,
    /// The header's search: "Abrir arquivo" (⌘P).
    OpenQuickOpen,
    TabChanged(CodeColumnTab),
    /// Back to the parent's conversation after reviewing from the child's.
    OpenSession(String),
    /// A change's diff on the stage.
    OpenDiff(crate::screens::stage::DiffRequest),
    /// The merge conflict resolver, over the conversation.
    OpenMergeResolver(crate::screens::merge::MergeResolverRequest),
    /// Lines picked in a card's diff, for the agent.
    Annotate(crate::screens::annotation::AnnotationRequest),
    /// "Mudanças" to look at (the conversation menu's fix and conflicts).
    ReviewChanges,
}

/// The checkout's files: tracked and new, then the ignored ones (dimmed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Files {
    pub paths: Vec<String>,
    pub ignored: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Loaded<T> {
    Pending,
    Ready(T),
    Failed(String),
}

pub struct CodeColumn {
    pub(crate) root: Option<String>,
    pub(crate) tab: CodeColumnTab,
    pub(crate) files: Loaded<Files>,
    tree: Arc<CodeTreeNode>,
    pub(crate) status: Option<WorkspaceGitStatusOutput>,
    /// The last status read failed (shown until one succeeds).
    pub(crate) status_error: Option<String>,
    pub(crate) changes: changes::Changes,
    pub(crate) delivery: delivery::Delivery,
    pub(crate) review: review::Review,
    /// The open conversation and its delegations waiting for review, and
    /// the one under review.
    pub(crate) session_id: Option<String>,
    pub(crate) reviewables: Vec<delegation::Reviewable>,
    pub(crate) delegation_review: Option<delegation::DelegationReview>,
    /// The open conversation's turn is running, and the task's delegations
    /// whose edits wait on the person (the recap's inputs).
    pub(crate) turn_running: bool,
    pub(crate) pending_delegations: usize,
    /// The recap's signals are open.
    pub(crate) recap_details: bool,
    /// "Último turno": the session's last turn in this task.
    pub(crate) turn: turn::LastTurn,
    /// The change shown on the stage (its row is marked).
    pub(crate) open_diff: Option<(
        dcc_core::domain::workspace_git::WorkspaceGitPreviewScope,
        String,
    )>,
    pub(crate) by_path: HashMap<String, TreeStatus>,
    pub(crate) changed_folders: HashSet<String>,
    file_set: Option<String>,
    /// Open folders per checkout, kept while the app runs.
    expanded: HashMap<String, HashSet<String>>,
    pub(crate) rows: Vec<CodeTreeRow>,
    pub(crate) list: ListState,
    /// The file open in the center (revealed in the tree).
    pub(crate) selected: Option<String>,
    /// The row the arrow keys are on.
    pub(crate) focused: Option<usize>,
    pub(crate) focus: gpui::FocusHandle,
    /// The desktop to run git actions through (tests point it elsewhere).
    pub(crate) endpoint: Option<crate::data::desktop_api::DesktopApi>,
    /// The app's database (the task's base branch); tests use their own.
    pub(crate) db_path: std::path::PathBuf,
    _files: Option<Task<()>>,
    _status: Option<Task<()>>,
    _branch: Option<Task<()>>,
    _git: Option<Task<()>>,
    _scrollbar_fade: Option<Task<()>>,
    _pull_request: Option<Task<()>>,
    _delivery: Option<Task<()>>,
    _button: Option<Task<()>>,
    _review: Option<Task<()>>,
    _recovery: Option<Task<()>>,
    _reviewers: Option<Task<()>>,
    _job_log: Option<Task<()>>,
    _job_retry: Option<Task<()>>,
    _delegation: Option<Task<()>>,
}

impl EventEmitter<CodeColumnEvent> for CodeColumn {}

impl CodeColumn {
    pub fn new(tab: CodeColumnTab, cx: &mut Context<Self>) -> Self {
        Self {
            root: None,
            tab,
            files: Loaded::Pending,
            tree: Arc::default(),
            status: None,
            status_error: None,
            changes: changes::Changes::new(),
            delivery: delivery::Delivery::default(),
            review: review::Review::default(),
            session_id: None,
            reviewables: Vec::new(),
            delegation_review: None,
            turn_running: false,
            pending_delegations: 0,
            recap_details: false,
            turn: turn::LastTurn::default(),
            open_diff: None,
            by_path: HashMap::new(),
            changed_folders: HashSet::new(),
            file_set: None,
            expanded: HashMap::new(),
            rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(240.0)),
            selected: None,
            focused: None,
            focus: cx.focus_handle(),
            endpoint: None,
            db_path: crate::data::loader::database_path(),
            _files: None,
            _status: None,
            _branch: None,
            _git: None,
            _scrollbar_fade: None,
            _pull_request: None,
            _delivery: None,
            _button: None,
            _review: None,
            _recovery: None,
            _reviewers: None,
            _job_log: None,
            _job_retry: None,
            _delegation: None,
        }
    }

    /// The task's checkout; another one starts over (its folders stay open).
    /// Under a delegation's review the task's own checkout is kept for later.
    pub fn set_root(&mut self, root: Option<String>, cx: &mut Context<Self>) {
        let wanted = root.as_deref().map(str::trim);
        if let Some(review) = &self.delegation_review {
            if wanted == Some(review.parent_root.as_str()) {
                return;
            }
            self.delegation_review = None;
        }
        self.switch_root(root, cx);
    }

    /// The folder the column reads.
    pub(crate) fn switch_root(&mut self, root: Option<String>, cx: &mut Context<Self>) {
        let root = root
            .map(|root| root.trim().to_string())
            .filter(|root| !root.is_empty());
        if root == self.root {
            return;
        }
        self.root = root;
        self.files = Loaded::Pending;
        self.tree = Arc::default();
        self.status = None;
        self.status_error = None;
        self.changes.reset();
        self.delivery.reset();
        self.review.reset();
        self.by_path.clear();
        self.changed_folders.clear();
        self.file_set = None;
        self.focused = None;
        self.sync_rows();
        self.load_files(cx);
        self._status = Some(self.status_loop(cx));
        cx.notify();
    }

    pub fn set_tab(&mut self, tab: CodeColumnTab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            cx.emit(CodeColumnEvent::TabChanged(tab));
            // Read again now rather than at the next status.
            self.read_branch(cx);
            self.read_pull_request(cx);
            self.read_review(false, cx);
            if tab == CodeColumnTab::Changes {
                self.read_last_turn(cx);
            }
            cx.notify();
        }
    }

    /// The file open in the center: its folders open, its row in view.
    pub fn reveal(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        if path == self.selected {
            return;
        }
        self.selected = path.clone();
        if let (Some(path), Some(root)) = (path, self.root.clone()) {
            let open = self.expanded.entry(root).or_default();
            open.extend(ancestor_folder_paths(&path));
            self.sync_rows();
            if let Some(index) = self.rows.iter().position(|row| row.path == path) {
                self.list.scroll_to_reveal_item(index);
            }
        }
        cx.notify();
    }

    /// "Mudanças" carries the count of changed files.
    pub fn change_count(&self) -> usize {
        self.status.as_ref().map_or(0, |status| {
            status
                .staged
                .iter()
                .chain(&status.unstaged)
                .map(|entry| entry.path.as_str())
                .collect::<HashSet<_>>()
                .len()
        })
    }

    fn open_folders(&self) -> HashSet<String> {
        self.root
            .as_ref()
            .and_then(|root| self.expanded.get(root))
            .cloned()
            .unwrap_or_default()
    }

    /// Splices the list only where the rows changed.
    fn sync_rows(&mut self) {
        let next = visible_rows(&self.tree, &self.open_folders());
        if next == self.rows {
            return;
        }
        let keep_start = self
            .rows
            .iter()
            .zip(&next)
            .take_while(|(a, b)| a == b)
            .count();
        let keep_end = self.rows[keep_start..]
            .iter()
            .rev()
            .zip(next[keep_start..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let removed = keep_start..self.rows.len() - keep_end;
        let inserted = next.len() - keep_end - keep_start;
        self.list.splice(removed, inserted);
        self.rows = next;
    }

    pub(crate) fn toggle_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let open = self.expanded.entry(root).or_default();
        if !open.remove(path) {
            open.insert(path.to_string());
        }
        self.sync_rows();
        cx.notify();
    }

    pub(crate) fn set_folder_open(&mut self, path: &str, open: bool, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let folders = self.expanded.entry(root).or_default();
        let changed = if open {
            folders.insert(path.to_string())
        } else {
            folders.remove(path)
        };
        if changed {
            self.sync_rows();
            cx.notify();
        }
    }

    /// A file from the tree; one with PR conversations opens in the pull
    /// request's diff (`handleOpenCodeFileFromTree`).
    pub(crate) fn open_file(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(request) = self.file_pr_diff(path) {
            cx.emit(CodeColumnEvent::OpenDiff(request));
            return;
        }
        cx.emit(CodeColumnEvent::OpenFile(path.to_string()));
    }

    fn load_files(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self._files = Some(cx.spawn(async move |this, cx| {
            let path = root.clone();
            let listed = cx
                .background_executor()
                .spawn(async move {
                    let tracked = dcc_infra::workspace_git::git_tracked_files(&path)?;
                    let ignored = dcc_infra::workspace_git::git_ignored_files(&path)?;
                    Ok::<_, String>((tracked, ignored))
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.root.as_deref() != Some(root.as_str()) {
                    return;
                }
                column.files = match listed {
                    Ok((tracked, ignored)) => {
                        // Ignored files (`.env`) show too, dimmed, as editors do.
                        let ignored_set: HashSet<String> = ignored.iter().cloned().collect();
                        let mut paths = tracked;
                        paths.extend(ignored);
                        column.tree = Arc::new(build_code_tree(&paths));
                        Loaded::Ready(Files {
                            paths,
                            ignored: ignored_set,
                        })
                    }
                    Err(error) => Loaded::Failed(error),
                };
                column.sync_rows();
                cx.notify();
            });
        }));
    }

    /// Git status every ten seconds; when a file appears or goes away the
    /// file list is read again (an agent created or deleted something).
    fn status_loop(&self, cx: &mut Context<Self>) -> Task<()> {
        let root = self.root.clone();
        cx.spawn(async move |this, cx| {
            let Some(root) = root else {
                return;
            };
            loop {
                let path = root.clone();
                // A read after the first one runs the header's line (`isFetching`).
                let _ = this.update(cx, |column, cx| {
                    if column.status.is_some() && column.tab == CodeColumnTab::Changes {
                        column.delivery.reading = true;
                        cx.notify();
                    }
                });
                let status = cx
                    .background_executor()
                    .spawn(
                        async move { dcc_infra::workspace_git::read_workspace_git_status(&path) },
                    )
                    .await;
                let alive = this.update(cx, |column, cx| {
                    column.delivery.reading = false;
                    if column.root.as_deref() != Some(root.as_str()) {
                        return;
                    }
                    match status {
                        Ok(status) => {
                            column.apply_status(status, cx);
                            column.read_branch(cx);
                            column.read_pull_request(cx);
                            column.read_review(false, cx);
                        }
                        Err(error) => {
                            column.status_error = Some(error);
                            cx.notify();
                        }
                    }
                });
                if alive.is_err() {
                    return;
                }
                cx.background_executor().timer(GIT_EVERY).await;
            }
        })
    }

    fn apply_status(&mut self, status: WorkspaceGitStatusOutput, cx: &mut Context<Self>) {
        let pairs = |entries: &[dcc_core::domain::workspace_git::WorkspaceGitChangeEntry]| {
            entries
                .iter()
                .map(|entry| (entry.path.clone(), entry.status.clone()))
                .collect::<Vec<_>>()
        };
        let (staged, unstaged) = (pairs(&status.staged), pairs(&status.unstaged));
        let (by_path, changed) = git_status(
            staged
                .iter()
                .map(|(path, status)| (path.as_str(), status.as_str())),
            unstaged
                .iter()
                .map(|(path, status)| (path.as_str(), status.as_str())),
        );
        let signature = file_set_signature(
            staged
                .iter()
                .chain(&unstaged)
                .map(|(path, status)| (path.as_str(), status.as_str())),
        );
        if self
            .file_set
            .as_ref()
            .is_some_and(|previous| *previous != signature)
        {
            self.load_files(cx);
        }
        self.file_set = Some(signature);
        self.by_path = by_path;
        self.changed_folders = changed;
        self.changes.sync(&status);
        self.status_error = None;
        self.status = Some(status);
        self.follow_conflicts(cx);
        cx.notify();
    }
}
