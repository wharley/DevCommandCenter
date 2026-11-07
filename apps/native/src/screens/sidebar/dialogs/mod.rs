//! The sidebar's dialogs: delete a task, remove a project, clean up completed
//! tasks and edit a project. One open at a time; each file holds one dialog's
//! state, actions and drawing.

mod cleanup;
mod delete_task;
mod project_edit;
mod remove_project;
mod terminal_lifecycle;

use gpui::{AnyElement, App, Context, Window, div, prelude::*, px};

use super::Sidebar;
use crate::data::deletion::DeleteError;
use crate::data::loader;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;

pub use cleanup::CleanupDialog;
pub use delete_task::DeleteTaskDialog;
pub use project_edit::ProjectEditDialog;
pub use remove_project::RemoveProjectDialog;
pub use terminal_lifecycle::TerminalLifecycleDialog;

pub enum SidebarDialog {
    DeleteTask(DeleteTaskDialog),
    RemoveProject(RemoveProjectDialog),
    Cleanup(CleanupDialog),
    EditProject(Box<ProjectEditDialog>),
    TerminalLifecycle(TerminalLifecycleDialog),
}

impl Sidebar {
    pub(super) fn close_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialog = None;
        cx.notify();
    }

    /// Re-reads the rail right away after a write.
    pub(crate) fn reload_now(&mut self, cx: &mut Context<Self>) {
        let db_path = loader::database_path();
        let untitled = cx.t("sidebar.newWorkspace").to_string();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { loader::load(&db_path, &untitled) })
                .await;
            let _ = this.update(cx, |sidebar, cx| match loaded {
                Ok(data) => sidebar.apply(data, cx),
                Err(error) => eprintln!("[dcc-native] sidebar data: {error:#}"),
            });
        })
        .detach();
    }
}

/// The toast for a refusal every deletion shares; `None` for the ones the
/// dialog handles itself.
pub(super) fn refusal_message(error: &DeleteError, cx: &App) -> Option<gpui::SharedString> {
    match error {
        DeleteError::DesktopOpen => Some(cx.t("native.desktopOpen")),
        DeleteError::MemoryPending => Some(cx.t("native.memoryPending")),
        DeleteError::UnappliedEdits(_) | DeleteError::Failed(_) => None,
    }
}

/// `rounded-lg border border-border/70 bg-muted/20 px-3 py-2.5 text-sm`: the
/// boxes the deletion dialogs put facts in.
pub(super) fn fact_box(cx: &App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .min_w_0()
        .rounded(cx.theme().radius.lg)
        .border_1()
        .border_color(colors.border.opacity(0.7))
        .bg(colors.muted.opacity(0.2))
        .px(px(12.0))
        .py(px(10.0))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(colors.muted_foreground)
}

pub(super) fn amber_box(cx: &App) -> gpui::Div {
    let amber = crate::theme::tailwind::AMBER_500;
    fact_box(cx)
        .border_color(amber.alpha(0.4))
        .bg(amber.alpha(0.1))
        .text_color(cx.theme().colors.foreground)
}

pub fn render(
    sidebar: &Sidebar,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> Option<AnyElement> {
    Some(match sidebar.dialog.as_ref()? {
        SidebarDialog::DeleteTask(dialog) => delete_task::render(sidebar, dialog, cx),
        SidebarDialog::RemoveProject(dialog) => remove_project::render(dialog, cx),
        SidebarDialog::Cleanup(dialog) => cleanup::render(sidebar, dialog, window, cx),
        SidebarDialog::EditProject(dialog) => project_edit::render(sidebar, dialog, window, cx),
        SidebarDialog::TerminalLifecycle(dialog) => terminal_lifecycle::render(dialog, cx),
    })
}
