//! The terminal dock under the conversation (`WorkspaceTerminalDrawer` in
//! the session workbench): it follows the open task's folders, ⌘J shows or
//! hides it, and its height is kept with the layout.

use gpui::{AnyElement, AppContext, Context, Entity, Subscription, Window, div, prelude::*};

use super::Shell;
use crate::data::desktop_api::DesktopApi;
use crate::prefs::Prefs;
use crate::screens::conversation::ConversationView;
use crate::screens::terminal::dock::clamp_height;
use crate::screens::terminal::tabs::TabStore;
use crate::screens::terminal::{DockEvent, ShellContext, TerminalDock, TerminalPlace};

pub(super) fn new_dock(
    prefs: &Prefs,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> (Entity<TerminalDock>, Subscription) {
    let parent = f32::from(window.viewport_size().height);
    let height = clamp_height(prefs.terminal_height, parent);
    let tabs = TabStore::load(&TabStore::default_path());
    let dock = cx.new(|cx| {
        let mut dock = TerminalDock::new(height, tabs, DesktopApi::load());
        dock.follow_activity(cx);
        dock
    });
    let events = cx.subscribe_in(&dock, window, Shell::on_terminal_event);
    (dock, events)
}

/// The rail counts what runs in each task's shells.
pub(super) fn follow_terminals(
    dock: &Entity<TerminalDock>,
    cx: &mut Context<Shell>,
) -> Subscription {
    cx.observe(dock, |shell, dock, cx| {
        let terminals = dock.read(cx).activity.clone();
        shell
            .sidebar
            .update(cx, |sidebar, cx| sidebar.set_terminals(terminals, cx));
    })
}

/// The open task's terminal scopes and what a new shell's banner says.
pub(super) fn place_of(view: &ConversationView, cx: &gpui::App) -> Option<TerminalPlace> {
    let data = view.data()?;
    let root_path = data.root_path.clone()?;
    let provider_label = view
        .composer
        .read(cx)
        .provider()
        .map(|provider| provider.label.clone());
    let session_state = match (&data.session_id, data.aborted) {
        (None, _) => "idle",
        (Some(_), true) => "aborted",
        (Some(_), false) => "active",
    };
    let branch = data
        .context
        .as_ref()
        .and_then(|context| context.current_branch.clone())
        .or_else(|| data.branch.clone());
    Some(TerminalPlace {
        workspace_id: data.task_id.clone(),
        project_id: data.project_id.clone(),
        root_path: Some(root_path),
        worktree_path: data.worktree_path.clone(),
        project_label: data
            .context
            .as_ref()
            .and_then(|context| context.project_label.clone())
            .unwrap_or_default(),
        branch: branch.clone(),
        base_branch: data.base_branch.clone(),
        context: ShellContext {
            workspace_name: data.workspace_name.clone().unwrap_or_default(),
            branch: branch.unwrap_or_default(),
            provider_label,
            session_state: session_state.to_string(),
        },
    })
}

impl Shell {
    pub(super) fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar.read(cx).has_open_task() != Some(true) {
            return;
        }
        self.terminal.update(cx, |dock, cx| dock.toggle(window, cx));
    }

    fn on_terminal_event(
        &mut self,
        _: &Entity<TerminalDock>,
        event: &DockEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            DockEvent::Hidden => self
                .conversation
                .update(cx, |conversation, cx| conversation.focus_composer_soon(cx)),
            // The conversation hides under an expanded dock.
            DockEvent::Expanded(_) => cx.notify(),
            // `handleSendTerminalToAgent`: into the draft, then the dock hides.
            DockEvent::SendToAgent(text) => {
                let composer = self.conversation.read(cx).composer.clone();
                let added =
                    composer.update(cx, |composer, cx| composer.append_text(text, window, cx));
                if added {
                    self.terminal.update(cx, |dock, cx| dock.hide(cx));
                }
            }
            DockEvent::Resized(height) => {
                let next = Prefs {
                    terminal_height: *height,
                    ..self.prefs.clone()
                };
                if next != self.prefs {
                    self.prefs = next;
                    if let Err(error) = self.prefs.save(&self.prefs_path) {
                        eprintln!("[dcc-native] could not save the layout: {error}");
                    }
                }
            }
        }
    }

    /// The task's content over the dock, split like the desktop's workbench.
    pub(super) fn with_terminal(&self, content: AnyElement, cx: &gpui::App) -> AnyElement {
        let dock = self.terminal.read(cx);
        let chat_hidden = dock.open && dock.expanded;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            // `chatHidden`: the conversation steps aside, still mounted.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .when(chat_hidden, |content| content.hidden())
                    .child(content),
            )
            .child(self.terminal.clone())
            .into_any_element()
    }
}
