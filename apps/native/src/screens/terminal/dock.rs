//! The bottom dock's state (`WorkspaceTerminalDrawer` with the store
//! behind it): the task's terminal scopes, the tabs of the one shown, a
//! session per tab kept across tasks, the desktop's event stream routed to
//! them by PTY, their activity read every 2 seconds, and the dock's height.

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Task, WeakEntity, Window};

use super::session::{SessionEvent, ShellContext, TerminalSession};
use super::tabs::{TabStore, owner_key};
use crate::data::desktop_api::DesktopApi;
use crate::data::terminal_api::{ShellSpec, TerminalEvent};

/// `DEFAULT_HEIGHT_PX`, `MIN_HEIGHT_PX`, `COMPACT_MIN_HEIGHT_PX`, `MIN_CHAT_HEIGHT_PX`
pub const DEFAULT_HEIGHT: f32 = 340.0;
const MIN_HEIGHT: f32 = 220.0;
const COMPACT_MIN_HEIGHT: f32 = 160.0;
const MIN_CHAT_HEIGHT: f32 = 360.0;
/// `ensureTerminal`'s first size; the view fits it right after.
const FIRST_COLS: u16 = 120;
const FIRST_ROWS: u16 = 32;
const ACTIVITY_EVERY: Duration = Duration::from_secs(2);

/// `clampHeight` inside a parent this tall.
pub fn clamp_height(height: f32, parent: f32) -> f32 {
    let upper = (parent - MIN_CHAT_HEIGHT).max(COMPACT_MIN_HEIGHT);
    let lower = MIN_HEIGHT.min(upper);
    let height = if height.is_finite() {
        height
    } else {
        DEFAULT_HEIGHT
    };
    height.clamp(lower, upper).round()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Worktree,
    Project,
}

/// `TerminalScopeTarget`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalScope {
    pub kind: ScopeKind,
    pub key: String,
    pub cwd: Option<String>,
    pub project_label: String,
    pub branch: Option<String>,
}

/// Where the open task's terminals run (`terminalScopes`): its worktree
/// (`worktree:<id>`), then the project's checkout (`<projectId>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalPlace {
    pub workspace_id: String,
    pub project_id: Option<String>,
    pub root_path: Option<String>,
    pub worktree_path: Option<String>,
    pub project_label: String,
    pub branch: Option<String>,
    pub base_branch: Option<String>,
    pub context: ShellContext,
}

impl TerminalPlace {
    pub fn scopes(&self) -> [TerminalScope; 2] {
        [
            TerminalScope {
                kind: ScopeKind::Worktree,
                key: format!("worktree:{}", self.workspace_id),
                cwd: self.worktree_path.clone(),
                project_label: self.project_label.clone(),
                branch: self.branch.clone(),
            },
            TerminalScope {
                kind: ScopeKind::Project,
                key: self
                    .project_id
                    .clone()
                    .unwrap_or_else(|| self.workspace_id.clone()),
                cwd: self.root_path.clone(),
                project_label: self.project_label.clone(),
                branch: self.base_branch.clone(),
            },
        ]
    }

    /// `activeTerminalScope`: the kind asked for when it has a folder, else
    /// the first that has one.
    pub fn scope(&self, kind: ScopeKind) -> TerminalScope {
        let scopes = self.scopes();
        scopes
            .iter()
            .find(|scope| scope.kind == kind && scope.cwd.is_some())
            .or_else(|| scopes.iter().find(|scope| scope.cwd.is_some()))
            .unwrap_or(&scopes[0])
            .clone()
    }
}

pub enum DockEvent {
    /// The dock closed: the composer takes the focus back.
    Hidden,
    /// A drag ended at this height.
    Resized(f32),
    /// It took over the conversation's room, or gave it back.
    Expanded(bool),
    /// Output for the composer's draft (`pushDebugEvidence`).
    SendToAgent(String),
}

/// What the dock was like on a task (`WorkspaceTerminalUiState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct UiState {
    open: bool,
    expanded: bool,
    kind: ScopeKind,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            open: false,
            expanded: false,
            kind: ScopeKind::Worktree,
        }
    }
}

pub struct TerminalDock {
    pub open: bool,
    /// The dock takes the conversation's room (the chat hides).
    pub expanded: bool,
    pub height: f32,
    /// The pointer y and height when a resize drag started.
    pub(super) drag: Option<(f32, f32)>,
    pub(super) tabs: TabStore,
    pub(super) place: Option<TerminalPlace>,
    pub(super) kind: ScopeKind,
    pub(super) sessions: HashMap<String, Entity<TerminalSession>>,
    pub(super) by_pty: HashMap<String, WeakEntity<TerminalSession>>,
    pub(super) api: Option<DesktopApi>,
    /// Every shell the desktop holds, as last read.
    pub activity: std::rc::Rc<Vec<crate::data::terminal_api::TerminalActivity>>,
    ui_states: HashMap<String, UiState>,
    /// "Renomear terminal", while open.
    pub(super) renaming: Option<super::dock_actions::Renaming>,
    /// "Há um processo em execução": the tab and what runs in it.
    pub(super) closing: Option<(String, String)>,
    _stream: Option<Task<()>>,
    _activity: Option<Task<()>>,
    _sessions: Vec<Subscription>,
}

impl EventEmitter<DockEvent> for TerminalDock {}

impl TerminalDock {
    pub fn new(height: f32, tabs: TabStore, api: Option<DesktopApi>) -> Self {
        Self {
            open: false,
            expanded: false,
            height,
            drag: None,
            tabs,
            place: None,
            kind: ScopeKind::Worktree,
            sessions: HashMap::new(),
            by_pty: HashMap::new(),
            api,
            ui_states: HashMap::new(),
            activity: Default::default(),
            renaming: None,
            closing: None,
            _stream: None,
            _activity: None,
            _sessions: Vec::new(),
        }
    }

    /// The open task changed (or its folders did).
    pub fn set_place(&mut self, place: Option<TerminalPlace>, cx: &mut Context<Self>) {
        if self.place == place {
            return;
        }
        let before = self.place.as_ref().map(|place| place.workspace_id.clone());
        let after = place.as_ref().map(|place| place.workspace_id.clone());
        self.place = place;
        if before != after {
            // Each task keeps its own dock: open or not, expanded, its scope.
            if let Some(before) = before {
                self.ui_states.insert(
                    before,
                    UiState {
                        open: self.open,
                        expanded: self.expanded,
                        kind: self.kind,
                    },
                );
            }
            let state = after
                .and_then(|after| self.ui_states.get(&after).copied())
                .unwrap_or_default();
            let was_expanded = self.expanded;
            self.open = state.open;
            self.expanded = state.expanded;
            self.kind = state.kind;
            self.renaming = None;
            self.closing = None;
            if was_expanded != self.expanded {
                cx.emit(DockEvent::Expanded(self.expanded));
            }
        }
        if self.open {
            self.show_active(cx);
        }
        cx.notify();
    }

    pub fn scope(&self) -> Option<TerminalScope> {
        self.place.as_ref().map(|place| place.scope(self.kind))
    }

    /// `terminal.toggle`: hides, or opens on the worktree.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.hide(cx);
        } else {
            self.show(ScopeKind::Worktree, window, cx);
        }
    }

    /// `handleOpenTerminal`: needs a folder; the scope's tab shows, focused.
    pub fn show(&mut self, kind: ScopeKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(place) = self.place.as_ref() else {
            return;
        };
        let scope = place.scope(kind);
        if scope.cwd.is_none() {
            return;
        }
        self.kind = scope.kind;
        self.open = true;
        self.show_active(cx);
        if let Some(session) = self.active_session(cx) {
            let focus = session.read(cx).focus_handle();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.renaming = None;
        self.closing = None;
        if self.expanded {
            self.expanded = false;
            cx.emit(DockEvent::Expanded(false));
        }
        cx.emit(DockEvent::Hidden);
        cx.notify();
    }

    /// The scope's active tab and its session, made when missing.
    fn show_active(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.scope() else {
            return;
        };
        if scope.cwd.is_none() {
            return;
        }
        if let Some(tab) = self.tabs.ensure(&scope.key) {
            self.session_for(&scope.key, &tab, cx);
        }
    }

    pub fn active_session(&self, cx: &gpui::App) -> Option<Entity<TerminalSession>> {
        let _ = cx;
        let scope = self.scope()?;
        let tab = self.tabs.scope(&scope.key).active()?.id.clone();
        self.sessions.get(&owner_key(&scope.key, &tab)).cloned()
    }

    pub(super) fn session_for(
        &mut self,
        scope_key: &str,
        tab: &str,
        cx: &mut Context<Self>,
    ) -> Option<Entity<TerminalSession>> {
        let owner = owner_key(scope_key, tab);
        if let Some(session) = self.sessions.get(&owner) {
            return Some(session.clone());
        }
        let scope = self.scope()?;
        let cwd = scope.cwd?;
        self.ensure_stream(cx);
        let context = self
            .place
            .as_ref()
            .map(|place| place.context.clone())
            .unwrap_or_default();
        let spec = ShellSpec::login(cwd, std::env::var("SHELL").ok(), FIRST_COLS, FIRST_ROWS);
        let api = self.api.clone();
        let session = cx.new(|cx| TerminalSession::new(owner.clone(), spec, context, api, cx));
        self._sessions.push(
            cx.subscribe(&session, |dock, session, event, cx| match event {
                SessionEvent::Opened(pty_id) => {
                    dock.by_pty.insert(pty_id.clone(), session.downgrade());
                }
                SessionEvent::Changed => cx.notify(),
                SessionEvent::SendSelection => dock.send_to_agent(cx),
            }),
        );
        self.sessions.insert(owner, session.clone());
        Some(session)
    }

    /// The `+`: a new tab (or the last one at the cap), focused.
    pub fn add_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scope) = self.scope().filter(|scope| scope.cwd.is_some()) else {
            return;
        };
        if let Some(tab) = self.tabs.add(&scope.key)
            && let Some(session) = self.session_for(&scope.key, &tab, cx)
        {
            let focus = session.read(cx).focus_handle();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub fn select_tab(&mut self, tab: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scope) = self.scope() else {
            return;
        };
        self.tabs.set_active(&scope.key, tab);
        if let Some(session) = self.session_for(&scope.key, tab, cx) {
            let focus = session.read(cx).focus_handle();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// `removeTerminal`: the tab and its shell go; an open dock keeps one.
    pub fn close_tab(&mut self, tab: &str, cx: &mut Context<Self>) {
        let Some(scope) = self.scope() else {
            return;
        };
        if let Some(session) = self.sessions.remove(&owner_key(&scope.key, tab)) {
            session.update(cx, |session, _| {
                if let Some(pty_id) = session.pty_id() {
                    self.by_pty.remove(pty_id);
                }
                session.kill();
            });
        }
        self.tabs.remove(&scope.key, tab);
        if self.open {
            self.show_active(cx);
        }
        cx.notify();
    }

    /// One connection to the desktop's stream, kept open (and opened again
    /// after the desktop restarts, every shell then read again).
    fn ensure_stream(&mut self, cx: &mut Context<Self>) {
        if self._stream.is_some() {
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<TerminalEvent>();
        std::thread::spawn(move || {
            let mut first = true;
            while !sender.is_closed() {
                if let Ok(events) = api.terminal_events() {
                    if !first && sender.unbounded_send(TerminalEvent::Lagged).is_err() {
                        return;
                    }
                    first = false;
                    for event in events {
                        if sender.unbounded_send(event).is_err() {
                            return;
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        self._stream = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = receiver.next().await {
                if this
                    .update(cx, |dock, cx| dock.dispatch(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        self.follow_activity(cx);
    }

    pub(super) fn dispatch(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        match event {
            TerminalEvent::Output { pty_id, data } => {
                if let Some(session) = self.by_pty.get(&pty_id).and_then(WeakEntity::upgrade) {
                    session.update(cx, |session, cx| session.output(&data, cx));
                }
            }
            TerminalEvent::Exit { pty_id, code } => {
                if let Some(session) = self.by_pty.remove(&pty_id).and_then(|weak| weak.upgrade()) {
                    session.update(cx, |session, cx| session.exited(code, cx));
                    cx.notify();
                }
            }
            TerminalEvent::Lagged => {
                self.by_pty.clear();
                for session in self.sessions.values() {
                    session.update(cx, |session, cx| session.resync(cx));
                }
            }
        }
    }

    /// `refreshTerminalActivity` every 2 seconds: every shell the desktop
    /// holds, ours and its own (the rail counts what runs in each task).
    pub fn follow_activity(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        if self._activity.is_some() {
            return;
        }
        self._activity = Some(cx.spawn(async move |this, cx| {
            loop {
                let api = api.clone();
                let Ok(items) = cx
                    .background_executor()
                    .spawn(async move { api.terminal_activity() })
                    .await
                else {
                    // The desktop closed: its shells went with it.
                    let alive = this.update(cx, |dock, cx| {
                        if !dock.activity.is_empty() {
                            dock.activity = Default::default();
                            cx.notify();
                        }
                    });
                    if alive.is_err() {
                        return;
                    }
                    cx.background_executor().timer(ACTIVITY_EVERY).await;
                    continue;
                };
                let alive = this.update(cx, |dock, cx| {
                    if *dock.activity != items {
                        dock.activity = std::rc::Rc::new(items.clone());
                        cx.notify();
                    }
                    // The tabs' dots and names are drawn by the dock.
                    let mut changed = false;
                    for session in dock.sessions.values() {
                        session.update(cx, |session, cx| {
                            let item = session
                                .pty_id()
                                .and_then(|pty| items.iter().find(|item| item.pty_id == pty))
                                .cloned();
                            changed |= session.activity != item;
                            session.set_activity(item, cx);
                        });
                    }
                    if changed {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    return;
                }
                cx.background_executor().timer(ACTIVITY_EVERY).await;
            }
        }));
    }

    pub(super) fn start_resize(&mut self, pointer_y: f32) {
        self.drag = Some((pointer_y, self.height));
    }

    /// The dock sits at the bottom: dragging up grows it.
    pub(super) fn resize_to(&mut self, pointer_y: f32, parent: f32, cx: &mut Context<Self>) {
        let Some((origin, height)) = self.drag else {
            return;
        };
        let next = clamp_height(height + origin - pointer_y, parent);
        if next != self.height {
            self.height = next;
            cx.notify();
        }
    }

    pub(super) fn finish_resize(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            cx.emit(DockEvent::Resized(self.height));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_height_leaves_room_for_the_conversation() {
        assert_eq!(clamp_height(340.0, 1000.0), 340.0);
        assert_eq!(clamp_height(900.0, 1000.0), 640.0);
        assert_eq!(clamp_height(100.0, 1000.0), 220.0);
        // A short window: the compact floor wins over the minimum.
        assert_eq!(clamp_height(300.0, 480.0), 160.0);
        assert_eq!(clamp_height(f32::NAN, 1000.0), 340.0);
    }

    #[test]
    fn a_task_without_worktree_falls_back_to_the_project() {
        let place = TerminalPlace {
            workspace_id: "w".into(),
            project_id: Some("p".into()),
            root_path: Some("/repo".into()),
            worktree_path: None,
            ..TerminalPlace::default()
        };
        let scope = place.scope(ScopeKind::Worktree);
        assert_eq!(scope.kind, ScopeKind::Project);
        assert_eq!(scope.key, "p");
        let isolated = TerminalPlace {
            worktree_path: Some("/wt".into()),
            ..place
        };
        assert_eq!(isolated.scope(ScopeKind::Worktree).key, "worktree:w");
        assert_eq!(
            isolated.scope(ScopeKind::Project).cwd.as_deref(),
            Some("/repo")
        );
    }
}
