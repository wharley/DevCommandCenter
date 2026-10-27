//! The command palette over the window (`setIsCommandPaletteOpen`): ⌘K or
//! ⌘⇧P, from anywhere and with no task open too. What it runs goes where
//! the desktop sends it: the composer, the terminal dock, the code column,
//! the open-project dialog, the shortcuts, the help, a task, a
//! conversation (restored first when archived) or a file on the stage.
//! ⌘⇧L focuses the composer, as on the desktop.

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription, Window};

use super::{Shell, Surface};
use crate::data::command_palette::{PaletteSession, PaletteTask};
use crate::screens::code_column::CodeColumnTab;
use crate::screens::command_palette::{
    CommandPalette, PaletteCommand, PaletteContext, PaletteEvent, WorkbenchCommand,
};
use crate::screens::open_project::{DialogMode, OpenRequest};
use crate::screens::terminal::ScopeKind;

gpui::actions!(
    dcc_shell,
    [
        /// ⌘K (and the older ⌘⇧P): the command palette.
        OpenCommandPalette,
        /// ⌘⇧L: the composer takes the focus.
        FocusComposer
    ]
);

pub(super) fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-k", OpenCommandPalette, None),
        gpui::KeyBinding::new("cmd-shift-p", OpenCommandPalette, None),
        gpui::KeyBinding::new("cmd-shift-l", FocusComposer, None),
    ]);
}

pub(super) type OpenPalette = (Entity<CommandPalette>, Subscription);

impl Shell {
    fn palette_context(&self, cx: &gpui::App) -> PaletteContext {
        let conversation = self.conversation.read(cx);
        let data = conversation.data();
        let selected_task = data.map(|data| data.task_id.clone());
        let sidebar = self.sidebar.read(cx);
        let tasks = sidebar
            .tasks()
            .iter()
            .filter(|task| task.status.is_selectable())
            .map(|task| PaletteTask {
                id: task.id.clone(),
                name: task.name.clone(),
                branch: task.branch.clone(),
            })
            .collect();
        let open_task = selected_task
            .as_deref()
            .and_then(|id| sidebar.tasks().iter().find(|task| task.id == id));
        let recent = data
            .map(|data| {
                data.sessions
                    .iter()
                    .map(|row| PaletteSession {
                        session_id: row.id.clone(),
                        task_id: data.task_id.clone(),
                        title: row.title.clone(),
                        task_name: open_task.map(|task| task.name.clone()),
                        branch: open_task.map(|task| task.branch.clone()),
                        snippet: String::new(),
                        archived: row.archived,
                        updated_at: row.updated_at.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        PaletteContext {
            selected_task,
            tasks,
            recent,
            root: conversation.workspace_path(),
            db_path: crate::data::loader::database_path(),
        }
    }

    pub(super) fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            return;
        }
        let context = self.palette_context(cx);
        let palette = cx.new(|cx| CommandPalette::new(context, window, cx));
        let events = cx.subscribe_in(&palette, window, |shell, _, event, window, cx| {
            shell.palette = None;
            if let PaletteEvent::Run(command) = event {
                shell.run_palette_command(command.clone(), window, cx);
            }
            cx.notify();
        });
        self.palette = Some((palette, events));
        cx.notify();
    }

    pub(super) fn focus_composer(&mut self, cx: &mut Context<Self>) {
        if self.conversation.read(cx).workspace_path().is_none() {
            return;
        }
        self.show(Surface::Conversation, cx);
        self.conversation
            .update(cx, |conversation, cx| conversation.focus_composer_soon(cx));
    }

    fn run_palette_command(
        &mut self,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            PaletteCommand::Workbench(command) => self.run_workbench_command(command, window, cx),
            PaletteCommand::OpenProject | PaletteCommand::Clone => {
                let clone = command == PaletteCommand::Clone;
                self.show(Surface::Conversation, cx);
                self.open_project_dialog(
                    OpenRequest {
                        mode: if clone {
                            DialogMode::Clone
                        } else {
                            DialogMode::default()
                        },
                        worktree: true,
                        ..OpenRequest::default()
                    },
                    window,
                    cx,
                );
            }
            PaletteCommand::Shortcuts => {
                self.shortcuts_open = true;
                cx.notify();
            }
            PaletteCommand::Help(topic) => self.open_help(topic, window, cx),
            PaletteCommand::Usage => self.open_usage(cx),
            PaletteCommand::OpenTask(task) => {
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.reveal_task(&task, cx));
                self.conversation
                    .update(cx, |conversation, cx| conversation.open(&task, None, cx));
                self.show(Surface::Conversation, cx);
            }
            PaletteCommand::OpenSession(session) => {
                self.open_session(session.task_id, session.session_id, session.archived, cx)
            }
            PaletteCommand::OpenFile(path) => self.open_checkout_file(path, None, window, cx),
        }
    }

    /// `runWorkbenchCommand`
    fn run_workbench_command(
        &mut self,
        command: WorkbenchCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let composer = self.conversation.read(cx).composer.clone();
        match command {
            WorkbenchCommand::FocusComposer => self.focus_composer(cx),
            WorkbenchCommand::Execution => composer.update(cx, |composer, cx| {
                composer.set_picker_open(true, window, cx)
            }),
            WorkbenchCommand::TogglePlan => {
                composer.update(cx, |composer, cx| composer.toggle_plan(cx))
            }
            WorkbenchCommand::TerminalWorktree | WorkbenchCommand::NewTerminal => {
                self.terminal.update(cx, |dock, cx| {
                    dock.show(ScopeKind::Worktree, window, cx);
                    if command == WorkbenchCommand::NewTerminal {
                        dock.add_tab(window, cx);
                    }
                });
            }
            WorkbenchCommand::TerminalProject => self
                .terminal
                .update(cx, |dock, cx| dock.show(ScopeKind::Project, window, cx)),
            WorkbenchCommand::Changes => {
                self.set_code_column(true, Some(CodeColumnTab::Changes), cx)
            }
            WorkbenchCommand::Files => self.set_code_column(true, Some(CodeColumnTab::Files), cx),
        }
    }

    pub(super) fn palette_overlay(&self) -> Option<AnyElement> {
        self.palette
            .as_ref()
            .map(|(palette, _)| palette.clone().into_any_element())
    }
}
