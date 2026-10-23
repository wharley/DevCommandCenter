//! What a row's buttons and menu do: the write runs off the UI thread, the
//! rail re-reads the database right after, and a failure shows the desktop's
//! toast. Inline rename lives here too.

use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::Sidebar;
use crate::data::actions::{self, TaskAction};
use crate::data::loader;
use crate::data::snooze::{SnoozeOption, snooze_until};
use crate::i18n::Translate;
use crate::ui::{TextInput, TextInputEvent, Toaster};

/// The row's own spinner (`pendingAction`): while it runs the row dims and
/// takes no clicks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingAction {
    Restore,
    Complete,
    Archive,
}

pub struct Rename {
    pub task_id: String,
    pub input: Entity<TextInput>,
    /// The write is in flight (`isRenaming`): the input is disabled.
    pub busy: bool,
    _events: Subscription,
}

type Done = Box<dyn FnOnce(&mut Sidebar, Result<(), String>, &mut Window, &mut Context<Sidebar>)>;

impl Sidebar {
    /// Runs `action`, reloads the rail, then calls `done` with the result.
    fn perform(
        &mut self,
        task_id: &str,
        action: TaskAction,
        done: Done,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.state.task(task_id) else {
            return;
        };
        let bundle_id = task.bundle_id.clone();
        let id = task_id.to_string();
        let db_path = loader::database_path();
        let untitled = cx.t("sidebar.newWorkspace").to_string();
        cx.spawn_in(window, async move |this, cx| {
            let write = action.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = actions::run(&db_path, &id, bundle_id.as_deref(), &write);
                    (result, loader::load(&db_path, &untitled))
                })
                .await;
            let (result, loaded) = result;
            let _ = this.update_in(cx, |sidebar, window, cx| {
                match loaded {
                    Ok(data) => sidebar.apply(data, cx),
                    Err(error) => eprintln!("[dcc-native] sidebar data: {error:#}"),
                }
                if let Err(error) = &result {
                    eprintln!("[dcc-native] task action {action:?} failed: {error}");
                    let message = match action.error_key() {
                        Some(key) => cx.t(key),
                        None => error.clone().into(),
                    };
                    Toaster::error(message, cx);
                }
                done(sidebar, result, window, cx);
            });
        })
        .detach();
    }

    /// Restore, complete or put on hold, with the row's spinner.
    pub(super) fn run_row_action(
        &mut self,
        task_id: &str,
        pending: PendingAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending.contains_key(task_id) {
            return;
        }
        self.pending.insert(task_id.to_string(), pending);
        cx.notify();
        let action = match pending {
            PendingAction::Restore => TaskAction::Restore,
            PendingAction::Complete => TaskAction::Complete,
            PendingAction::Archive => TaskAction::Archive,
        };
        let id = task_id.to_string();
        self.perform(
            task_id,
            action,
            Box::new(move |sidebar, result, _, cx| {
                sidebar.pending.remove(&id);
                // A restored task opens, like the desktop's `restoreWorkspace`.
                if pending == PendingAction::Restore && result.is_ok() {
                    sidebar.open_task(&id, cx);
                }
                cx.notify();
            }),
            window,
            cx,
        );
    }

    pub(super) fn set_pinned(
        &mut self,
        task_id: &str,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.perform(
            task_id,
            TaskAction::SetPinned(pinned),
            Box::new(|_, _, _, _| {}),
            window,
            cx,
        );
    }

    /// `None` wakes the task now.
    pub(super) fn snooze(
        &mut self,
        task_id: &str,
        option: Option<SnoozeOption>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let until = option.map(|option| {
            snooze_until(option, chrono::Local::now())
                .to_utc()
                .to_rfc3339()
        });
        self.perform(
            task_id,
            TaskAction::Snooze(until),
            Box::new(|_, _, _, _| {}),
            window,
            cx,
        );
    }

    /// Opening a snoozed task wakes it (`selectWorkspace` in `sidebar.tsx`);
    /// quietly, like the desktop.
    pub(super) fn wake_quietly(&mut self, task_id: &str, cx: &mut Context<Self>) {
        let Some(task) = self.state.task(task_id) else {
            return;
        };
        if task.snoozed_until.is_none() {
            return;
        }
        let bundle_id = task.bundle_id.clone();
        let id = task_id.to_string();
        let db_path = loader::database_path();
        cx.background_executor()
            .spawn(async move {
                let _ = actions::run(
                    &db_path,
                    &id,
                    bundle_id.as_deref(),
                    &TaskAction::Snooze(None),
                );
            })
            .detach();
    }

    pub(super) fn set_row_menu_open(&mut self, task_id: &str, open: bool, cx: &mut Context<Self>) {
        let current = self.row_menu_open.as_deref() == Some(task_id);
        if open {
            self.row_menu_open = Some(task_id.to_string());
        } else if current {
            self.row_menu_open = None;
        }
        cx.notify();
    }

    pub(super) fn start_rename(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.state.task(task_id) else {
            return;
        };
        let title = task.display_title();
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_max_len(120)
                .with_placeholder(cx.t("sidebar.renameWorkspaceInput"))
        });
        input.update(cx, |input, cx| {
            input.set_text(title, cx);
            input.focus_and_select_all(window, cx);
        });
        let events = cx.subscribe_in(
            &input,
            window,
            |this, _, event: &TextInputEvent, window, cx| match event {
                TextInputEvent::Submitted | TextInputEvent::Blurred => {
                    this.submit_rename(window, cx)
                }
                TextInputEvent::Cancelled => this.cancel_rename(cx),
                TextInputEvent::Changed => {}
            },
        );
        self.renaming = Some(Rename {
            task_id: task_id.to_string(),
            input,
            busy: false,
            _events: events,
        });
        cx.notify();
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        if self.renaming.as_ref().is_some_and(|rename| rename.busy) {
            return;
        }
        self.renaming = None;
        cx.notify();
    }

    fn submit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.renaming.as_mut() else {
            return;
        };
        if rename.busy {
            return;
        }
        let task_id = rename.task_id.clone();
        let input = rename.input.clone();
        let name = input
            .read(cx)
            .text()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if name.is_empty() {
            Toaster::error(cx.t("sidebar.renameWorkspaceEmpty"), cx);
            input.update(cx, |input, cx| input.focus_and_select_all(window, cx));
            return;
        }
        let unchanged = self
            .state
            .task(&task_id)
            .is_none_or(|task| task.display_title() == name);
        if unchanged {
            self.renaming = None;
            cx.notify();
            return;
        }
        rename.busy = true;
        cx.notify();
        self.perform(
            &task_id,
            TaskAction::Rename(name),
            Box::new(move |sidebar, result, window, cx| {
                match result {
                    Ok(()) => sidebar.renaming = None,
                    Err(_) => {
                        if let Some(rename) = sidebar.renaming.as_mut() {
                            rename.busy = false;
                            rename
                                .input
                                .update(cx, |input, cx| input.focus_and_select_all(window, cx));
                        }
                    }
                }
                cx.notify();
            }),
            window,
            cx,
        );
    }

    pub(super) fn is_renaming(&self, task_id: &str) -> bool {
        self.renaming
            .as_ref()
            .is_some_and(|rename| rename.task_id == task_id)
    }
}
