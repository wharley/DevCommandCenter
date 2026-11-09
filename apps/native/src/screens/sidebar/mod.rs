//! Screen 1: the left sidebar (`features/workspaces/sidebar.tsx`). This file
//! owns the state, the live loading and the events; each region of the
//! sidebar renders from its own file.

mod actions;
mod agents;
pub use agents::AgentPageData;
mod arrival;
mod attention;
mod child_task_row;
mod collapsed;
mod dialogs;
mod disk_usage;
mod empty;
mod footer;
mod header;
mod identity_card;
mod inbox;
mod labels;
pub use labels::reason_label;
mod list;
pub mod mascot;
mod meta_chips;
mod navigation;
#[cfg(test)]
mod open_tests;
mod piece;
mod project_menu;
#[cfg(test)]
mod rail_tests;
mod repo_picker;
mod row;
mod row_actions;
mod row_status;
mod running;
mod sections;
mod skills_count;
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
    /// "Abrir branch" on a project's menu (`onOpenBranchFromProject`).
    OpenBranchInProject {
        project_id: String,
        root_path: String,
        base_branch: String,
        label: String,
    },
    OpenPullRequests,
    OpenNotes,
    /// Tasks just completed (a bundle's members, or the task): their notes may be done.
    TasksCompleted(Vec<String>),
    /// Pull requests an agent asked DCC to review, not taken yet.
    ReviewRequests(Vec<dcc_core::domain::pr_review_request::PrReviewRequest>),
    /// A resident agent's page.
    /// "Agentes": the team's screen.
    OpenAgents,
    /// One conversation of a task (a child task's, or its parent's proposal).
    OpenSession {
        task_id: String,
        session_id: String,
    },
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
    /// Rows, the running pulse and agents' faces, each redrawn on its own.
    pieces: piece::Pieces,
    mascot_frame: bool,
    /// The page on screen a navigation row stands for.
    current_page: Option<NavPage>,
    /// Pull requests reviewed that got news since (`PrNews`).
    pr_news: usize,
    /// A task just created: opened as soon as the list has it.
    pending_open: Option<String>,
    /// The first load opens the first task, like the desktop does.
    opened_first: bool,
    /// Tasks whose latest result has not been opened ("New result").
    unseen_results: HashSet<String>,
    /// Rows whose restore, complete or hold is in flight.
    pending: HashMap<String, actions::PendingAction>,
    /// The row whose ⋯ menu is open keeps its actions shown.
    row_menu_open: Option<String>,
    renaming: Option<actions::Rename>,
    repo_picker: Option<repo_picker::RepoPicker>,
    completed_scan: disk_usage::CompletedScan,
    dialog: Option<dialogs::SidebarDialog>,
    agent_markers: agents::AgentMarkers,
    /// The reviewer's pull request reviews from the hub.
    pr_reviews: Vec<crate::data::pr_review_jobs::PrReviewJob>,
    _pr_reviews: gpui::Subscription,
    /// The conversation on screen, when the task has several.
    selected_session_id: Option<String>,
    /// Skills and instruction files the open task's agents get (footer badge).
    skill_count: usize,
    /// Every shell the desktop holds (the row's count of running programs).
    terminals: std::rc::Rc<Vec<crate::data::terminal_api::TerminalActivity>>,
    /// Where the task's shells are ended (the desktop's API; a fake in tests).
    terminal_api: Option<crate::data::desktop_api::DesktopApi>,
    task_flash: Option<arrival::Flash<String>>,
    section_flash: Option<arrival::Flash<state::HoldSection>>,
    membership: Option<arrival::Membership>,
    /// When the task list last moved: its scrollbar shows a moment after.
    scrolled_at: Option<std::time::Instant>,
    _scrollbar_fade: Option<Task<()>>,
    _refresh: Task<()>,
    _clock: Task<()>,
    _git: Task<()>,
    /// The watching mascots' blink (`mascot`).
    _blink: Task<()>,
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
        let weak = cx.weak_entity();
        list.set_scroll_handler(move |_, _, cx| {
            let _ = weak.update(cx, |sidebar: &mut Sidebar, cx| sidebar.list_scrolled(cx));
        });
        // The update button in the footer and the rail.
        if let Some(updater) = crate::app::updater::updater(cx) {
            cx.observe(&updater, |_, _, cx| cx.notify()).detach();
        }
        Self {
            state,
            collapsed,
            list,
            items,
            pieces: piece::Pieces::default(),
            mascot_frame: false,
            current_page: None,
            pr_news: 0,
            pending_open: None,
            opened_first: false,
            unseen_results: HashSet::new(),
            pending: HashMap::new(),
            row_menu_open: None,
            renaming: None,
            repo_picker: None,
            completed_scan: disk_usage::CompletedScan::default(),
            dialog: None,
            agent_markers: agents::AgentMarkers::load(
                seen_path.parent().unwrap_or(std::path::Path::new(".")),
            ),
            pr_reviews: Vec::new(),
            _pr_reviews: cx.observe_global::<crate::data::pr_review_jobs::PrReviewJobs>(
                |sidebar, cx| {
                    sidebar.pr_reviews = crate::data::pr_review_jobs::PrReviewJobs::all(cx);
                    cx.notify();
                },
            ),
            selected_session_id: None,
            skill_count: 0,
            terminals: Default::default(),
            terminal_api: None,
            task_flash: None,
            section_flash: None,
            membership: None,
            scrolled_at: None,
            _scrollbar_fade: None,
            _refresh: Self::refresh_loop(cx),
            _clock: Self::clock_loop(cx),
            _git: Self::git_loop(cx),
            _blink: Self::blink_loop(cx),
        }
    }

    /// Git and PR of every open task, off the UI thread (`git_watch`).
    fn git_loop(cx: &mut Context<Self>) -> Task<()> {
        let db_path = loader::database_path();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(GIT_WAKE).await;
                let Ok((watched, cache, foreground)) = this.update(cx, |sidebar, cx| {
                    (
                        sidebar.state.watched(),
                        sidebar.state.git.clone(),
                        crate::data::foreground::is_foreground(cx),
                    )
                }) else {
                    return;
                };
                // The window behind others: no `git`/`gh` per task meanwhile.
                if !foreground {
                    continue;
                }
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
                    // Mascots walk and seconds tick only in front of the person.
                    let front = crate::data::foreground::is_foreground(cx);
                    let animating = front && !sidebar.state.running_ids().is_empty();
                    // Seconds tick while a turn is open (running or waiting);
                    // otherwise times read in minutes and a 30s beat is enough.
                    let open_turn = front
                        && sidebar
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
                    // The next daily recap opens at the first look after 4am.
                    if stale {
                        sidebar.agent_markers.recap.refresh(chrono::Local::now());
                        sidebar.refresh_skill_count(cx);
                    }
                    sidebar.mascot_frame = animating && !sidebar.mascot_frame;
                    let moved = sidebar.sync_items();
                    if stale || moved || sidebar.collapsed {
                        cx.notify();
                    } else {
                        // A step or a second: only the rows that walk or tick.
                        let live = sidebar.state.live_ids();
                        sidebar.pieces.notify_rows(&live, cx);
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
        })
    }

    /// Data as if just read, for other screens' tests.
    #[cfg(test)]
    pub(crate) fn apply_for_tests(&mut self, data: SidebarData, cx: &mut Context<Self>) {
        self.apply(data, cx);
    }

    fn apply(&mut self, data: SidebarData, cx: &mut Context<Self>) {
        if data == self.state.data {
            // A task already listed by the loop's previous read still opens.
            if self.pending_open.is_some() {
                self.open_listed(cx);
            }
            return;
        }
        if !data.review_requests.is_empty() {
            cx.emit(SidebarEvent::ReviewRequests(data.review_requests.clone()));
        }
        self.state.data = data;
        self.state.now_ms = time::now_ms();
        self.sync_items();
        self.refresh_completed_scan(cx);
        self.notice_section_moves(cx);
        self.open_listed(cx);
        cx.notify();
    }

    /// A pending task once it is listed; on the first load, the first open
    /// task in the rail's order (`workspaceList.find(isWorkspaceSelectable)`).
    fn open_listed(&mut self, cx: &mut Context<Self>) {
        if let Some(task_id) = self.pending_open.clone()
            && self.state.task(&task_id).is_some()
        {
            self.pending_open = None;
            self.opened_first = true;
            self.open_task(&task_id, cx);
            return;
        }
        if self.opened_first || self.state.selected_task_id.is_some() {
            self.opened_first = true;
            return;
        }
        self.opened_first = true;
        let first = self.items.iter().find_map(|item| match item {
            RailItem::Row { task_id, .. } => self
                .state
                .task(task_id)
                .filter(|task| task.status.is_selectable())
                .map(|task| task.id.clone()),
            _ => None,
        });
        if let Some(task_id) = first {
            self.open_task(&task_id, cx);
        }
    }

    /// Reads the list now and opens `task_id` once it is there (a task the
    /// desktop just created).
    pub fn open_when_listed(&mut self, task_id: String, cx: &mut Context<Self>) {
        self.pending_open = Some(task_id);
        // The periodic read may have listed it before the desktop answered.
        if self.opened_first {
            self.open_listed(cx);
        }
        self._refresh = Self::refresh_loop(cx);
    }

    /// Marks `task_id` as the open one without opening it again (the shell
    /// opens a chosen conversation of it).
    pub fn reveal_task(&mut self, task_id: &str, cx: &mut Context<Self>) {
        if self.state.task(task_id).is_none() {
            return;
        }
        self.state.select(task_id);
        self.sync_items();
        cx.notify();
    }

    /// A listed task's name.
    pub fn task_name(&self, task_id: &str) -> Option<String> {
        self.state.task(task_id).map(|task| task.name.clone())
    }

    /// Whether a task is open, once the list has loaded (`None` before).
    pub fn has_open_task(&self) -> Option<bool> {
        self.opened_first
            .then(|| self.state.selected_task_id.is_some())
    }

    /// The resident agents and their sessions' state, as last read.
    pub fn agents(&self) -> &[crate::data::agents::AgentRecord] {
        &self.state.data.agents
    }

    /// Agents an agent proposed, waiting for the person.
    pub fn agent_proposals(&self) -> &[dcc_core::domain::agent_proposal::AgentProposal] {
        &self.state.data.agent_proposals
    }

    /// Every task the list has (ideas excluded).
    pub fn tasks(&self) -> &[crate::data::tasks::TaskSummary] {
        &self.state.data.tasks
    }

    pub fn projects(&self) -> &[Project] {
        &self.state.data.projects
    }

    /// The folders each project links, by repository id.
    pub fn linked_folders(
        &self,
    ) -> &HashMap<String, dcc_core::application::linked_task::LinkedFolders> {
        &self.state.data.linked_folders
    }

    /// Updates the list in place: only the changed span is spliced, so live
    /// updates never move the scroll position.
    /// Whether the list changed.
    fn sync_items(&mut self) -> bool {
        let next = self.state.items();
        if next == self.items {
            return false;
        }
        let (range, count) = list::changed_span(&self.items, &next);
        self.list.splice(range, count);
        self.items = next;
        let listed = self.items.iter().filter_map(list::row_piece).collect();
        self.pieces.retain_rows(&listed);
        true
    }

    /// What runs in the desktop's shells, as the terminal dock last read it.
    pub fn set_terminals(
        &mut self,
        terminals: std::rc::Rc<Vec<crate::data::terminal_api::TerminalActivity>>,
        cx: &mut Context<Self>,
    ) {
        if !std::rc::Rc::ptr_eq(&self.terminals, &terminals) {
            self.terminals = terminals;
            cx.notify();
        }
    }

    /// `useWorkspaceActiveTerminalCount`: programs running in the task's
    /// worktrees (its own and, for a multi-project task, its members').
    pub(crate) fn active_terminals(&self, task_id: &str) -> usize {
        let Some(task) = self.state.task(task_id) else {
            return 0;
        };
        let mut ids = vec![task.id.clone()];
        ids.extend(
            task.member_workspace_ids
                .iter()
                .filter(|id| **id != task.id)
                .cloned(),
        );
        crate::data::terminal_api::active_count(&self.terminals, &ids)
    }

    pub fn set_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        self.collapsed = collapsed;
        cx.notify();
    }

    /// The open notes' count, as soon as the notes change.
    pub fn set_open_notes(&mut self, count: usize, cx: &mut Context<Self>) {
        if self.state.data.open_notes != count {
            self.state.data.open_notes = count;
            cx.notify();
        }
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
        self.refresh_skill_count(cx);
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

    /// Shows the scrollbar and hides it once the list rests.
    fn list_scrolled(&mut self, cx: &mut Context<Self>) {
        self.scrolled_at = Some(std::time::Instant::now());
        self._scrollbar_fade = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(
                    crate::ui::scrollbar::SCROLLBAR_LINGER + std::time::Duration::from_millis(50),
                )
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        }));
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
        self.flash_task(task_id, cx);
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let dialog = dialogs::render(self, window, cx);
        let root = root.children(dialog);
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
