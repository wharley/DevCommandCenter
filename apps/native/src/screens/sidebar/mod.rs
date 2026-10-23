//! Screen 1: the left sidebar (`features/workspaces/sidebar.tsx`). This file
//! owns the state, the live loading and the events; each region of the
//! sidebar renders from its own file.

mod actions;
mod attention;
mod collapsed;
mod empty;
mod footer;
mod header;
mod identity_card;
mod labels;
mod list;
mod meta_chips;
mod navigation;
mod repo_picker;
mod row;
mod row_actions;
mod row_status;
mod sections;
pub mod state;
mod tasks_header;

pub use navigation::NavPage;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use gpui::{Context, EventEmitter, ListAlignment, ListState, Task, Window, div, prelude::*, px};

use crate::data::git_watch;
use crate::data::loader::{self, SidebarData};
use crate::data::seen::SeenResults;
use crate::data::tasks::{Project, TaskSummary};
use crate::data::time;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use state::{RailItem, SidebarState};

/// How often the rail re-reads the database. The desktop refreshes on
/// backend events; the native app runs beside it and reads instead.
const REFRESH: Duration = Duration::from_secs(2);
/// Mascot frame length while an agent runs (`dcc-mascot` 0.9s, two steps).
const MASCOT_STEP: Duration = Duration::from_millis(450);
/// How often the git watcher wakes; each task is only re-read when its own
/// cadence (git 30s, PR 60s) is due.
const GIT_WAKE: Duration = Duration::from_secs(5);

/// What the sidebar asks the rest of the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarEvent {
    OpenTask(String),
    NewTask,
    /// A project picked in the + menu (`onCreateWorkspaceFromProject`).
    NewTaskInProject {
        project_id: String,
        root_path: String,
        base_branch: String,
        label: String,
    },
    OpenPullRequests,
    OpenNotes,
    OpenUsage,
    OpenSkills,
    OpenFeedback,
    OpenSettings,
    OpenHelp,
    OpenProject,
    CloneProject,
    ToggleCollapsed,
    ToggleCodeColumn,
    /// "Em espera" or "Concluídos" opened or closed (saved with the layout).
    SectionsChanged,
}

pub struct Sidebar {
    state: SidebarState,
    collapsed: bool,
    list: ListState,
    items: Vec<RailItem>,
    mascot_frame: bool,
    /// The page on screen a navigation row stands for.
    current_page: Option<NavPage>,
    /// Tasks whose latest result has not been opened ("New result").
    unseen_results: HashSet<String>,
    /// Rows whose restore, complete or hold is in flight.
    pending: HashMap<String, actions::PendingAction>,
    /// The row whose ⋯ menu is open keeps its actions shown.
    row_menu_open: Option<String>,
    renaming: Option<actions::Rename>,
    repo_picker: Option<repo_picker::RepoPicker>,
    _refresh: Task<()>,
    _clock: Task<()>,
    _git: Task<()>,
}

impl EventEmitter<SidebarEvent> for Sidebar {}

impl Sidebar {
    pub fn new(
        collapsed: bool,
        waiting_open: bool,
        completed_open: bool,
        seen_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let now = time::now_ms();
        let seen = SeenResults::load(&seen_path, &chrono::Utc::now().to_rfc3339());
        let state = SidebarState {
            seen,
            waiting_open,
            completed_open,
            now_ms: now,
            ..SidebarState::default()
        };
        let items = state.items();
        let list = ListState::new(items.len(), ListAlignment::Top, px(400.0));
        Self {
            state,
            collapsed,
            list,
            items,
            mascot_frame: false,
            current_page: None,
            unseen_results: HashSet::new(),
            pending: HashMap::new(),
            row_menu_open: None,
            renaming: None,
            repo_picker: None,
            _refresh: Self::refresh_loop(cx),
            _clock: Self::clock_loop(cx),
            _git: Self::git_loop(cx),
        }
    }

    /// Git and PR of every open task, off the UI thread (`git_watch`).
    fn git_loop(cx: &mut Context<Self>) -> Task<()> {
        let db_path = loader::database_path();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(GIT_WAKE).await;
                let Ok((watched, cache)) = this.update(cx, |sidebar, _| {
                    (sidebar.state.watched(), sidebar.state.git.clone())
                }) else {
                    return;
                };
                let path = db_path.clone();
                let now = time::now_ms();
                let cache = cx
                    .background_executor()
                    .spawn(async move { git_watch::refresh(&path, &watched, cache, now) })
                    .await;
                if this
                    .update(cx, |sidebar, cx| sidebar.apply_git(cache, cx))
                    .is_err()
                {
                    return;
                }
            }
        })
    }

    fn apply_git(
        &mut self,
        cache: HashMap<String, git_watch::GitSnapshot>,
        cx: &mut Context<Self>,
    ) {
        let open: Vec<String> = self
            .state
            .data
            .tasks
            .iter()
            .filter(|task| task.status.is_selectable())
            .map(|task| task.id.clone())
            .collect();
        let state = &mut self.state;
        let blockers = git_watch::blockers(
            &open,
            &cache,
            &state.data.review_pending,
            &mut state.first_seen,
            &chrono::Utc::now().to_rfc3339(),
        );
        if cache == state.git && blockers == state.blockers {
            return;
        }
        state.git = cache;
        state.blockers = blockers;
        self.sync_items();
        cx.notify();
    }

    fn refresh_loop(cx: &mut Context<Self>) -> Task<()> {
        let db_path = loader::database_path();
        let untitled = cx.t("sidebar.newWorkspace").to_string();
        cx.spawn(async move |this, cx| {
            loop {
                let (path, name) = (db_path.clone(), untitled.clone());
                let loaded = cx
                    .background_executor()
                    .spawn(async move { loader::load(&path, &name) })
                    .await;
                let alive = this.update(cx, |sidebar, cx| match loaded {
                    Ok(data) => sidebar.apply(data, cx),
                    Err(error) => eprintln!("[dcc-native] sidebar data: {error:#}"),
                });
                if alive.is_err() {
                    return;
                }
                cx.background_executor().timer(REFRESH).await;
            }
        })
    }

    /// Advances the clock (elapsed times, day sections) and the mascots.
    fn clock_loop(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(MASCOT_STEP).await;
                let alive = this.update(cx, |sidebar, cx| {
                    let now = time::now_ms();
                    let animating = !sidebar.state.running_ids().is_empty();
                    // Seconds tick while a turn is open (running or waiting);
                    // otherwise times read in minutes and a 30s beat is enough.
                    let open_turn = sidebar
                        .state
                        .data
                        .activities
                        .values()
                        .any(|activity| activity.is_turn_open());
                    let stale = now - sidebar.state.now_ms >= 30_000;
                    if !(animating || open_turn || stale) {
                        return;
                    }
                    sidebar.state.now_ms = now;
                    sidebar.mascot_frame = animating && !sidebar.mascot_frame;
                    sidebar.sync_items();
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
        })
    }

    fn apply(&mut self, data: SidebarData, cx: &mut Context<Self>) {
        if data == self.state.data {
            return;
        }
        self.state.data = data;
        self.state.now_ms = time::now_ms();
        self.sync_items();
        cx.notify();
    }

    /// Updates the list in place: only the changed span is spliced, so live
    /// updates never move the scroll position.
    fn sync_items(&mut self) {
        let next = self.state.items();
        if next == self.items {
            return;
        }
        let (range, count) = list::changed_span(&self.items, &next);
        self.list.splice(range, count);
        self.items = next;
    }

    pub fn set_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        self.collapsed = collapsed;
        cx.notify();
    }

    pub fn set_current_page(&mut self, page: Option<NavPage>, cx: &mut Context<Self>) {
        self.current_page = page;
        cx.notify();
    }

    /// The project a task belongs to (`repositoriesBySourceKey`).
    fn project_for(&self, task: &TaskSummary) -> Option<&Project> {
        let root = task.root_path.trim();
        if root.is_empty() {
            return None;
        }
        self.state
            .data
            .projects
            .iter()
            .find(|project| project.root_path.trim() == root)
    }

    pub fn section_state(&self) -> (bool, bool) {
        (self.state.waiting_open, self.state.completed_open)
    }

    fn open_task(&mut self, task_id: &str, cx: &mut Context<Self>) {
        let Some(task) = self.state.task(task_id) else {
            return;
        };
        if !task.status.is_selectable() {
            return;
        }
        self.wake_quietly(task_id, cx);
        self.state.select(task_id);
        self.sync_items();
        cx.emit(SidebarEvent::OpenTask(task_id.to_string()));
        cx.notify();
    }

    fn toggle_section(&mut self, section: state::HoldSection, cx: &mut Context<Self>) {
        self.state.toggle_section(section);
        self.sync_items();
        cx.emit(SidebarEvent::SectionsChanged);
        cx.notify();
    }

    fn jump_to_running(&mut self, cx: &mut Context<Self>) {
        let Some(task_id) = self.state.next_running() else {
            return;
        };
        if let Some(index) = self
            .items
            .iter()
            .position(|item| matches!(item, RailItem::Row { task_id: id, .. } if *id == task_id))
        {
            self.list.scroll_to_reveal_item(index);
        }
        cx.notify();
    }

    /// Nothing moves under the pointer: the list and "Needs you" hold their
    /// order while hovered and catch up when it leaves.
    fn hold_attention(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.state.frozen_attention = hovered.then(|| self.state.attention());
        cx.notify();
    }

    fn hold_order(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.state.frozen_order =
            hovered.then(|| crate::data::rail::freeze(&self.state.task_sections()));
        self.sync_items();
        cx.notify();
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        // `.dcc-project-sidebar`: the sidebar warmed 3% toward the idea accent.
        let background = mix_oklab(theme.colors.sidebar, 0.97, theme.colors.accent_idea);
        let root = div()
            .id("sidebar")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(background);
        if self.collapsed {
            return root.child(collapsed::render(self, cx));
        }
        self.unseen_results = self.state.unseen_result_ids();
        let body = if self.state.is_empty() {
            empty::render(self, cx)
        } else {
            list::render(self, cx).into_any_element()
        };
        root.on_hover(cx.listener(|this, hovered: &bool, _, cx| this.hold_attention(*hovered, cx)))
            .child(header::render(self, cx))
            .child(navigation::render(self, cx))
            .children(attention::render(self, cx))
            .child(tasks_header::render(self, cx))
            .child(body)
            .child(footer::render(self, cx))
    }
}
