//! "Excluir workspace permanentemente": what deleting frees, the remote
//! branch it can also remove and, after a refusal, the child tasks' edits
//! that would be lost.

use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

use super::{SidebarDialog, amber_box, fact_box, refusal_message};
use crate::data::deletion::{self, DeleteError, DeleteTask};
use crate::data::disk::{format_disk_bytes, task_bytes};
use crate::data::loader;
use crate::i18n::{Args, I18n, Translate};
use crate::screens::sidebar::Sidebar;
use crate::theme::ActiveTheme;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Checkbox, Dialog, Toaster, dialog_footer, dialog_header,
    spinner, tabular_nums,
};
use dcc_infra::workspace_deletion::{
    UnappliedDelegationEdits, WorkspaceRemoteBranchDeletionTarget,
};

pub struct DeleteTaskDialog {
    task_id: String,
    bundle_id: Option<String>,
    title: String,
    /// `None` while the remote branches are being read.
    targets: Option<Vec<WorkspaceRemoteBranchDeletionTarget>>,
    delete_remote: bool,
    unapplied: Option<Vec<UnappliedDelegationEdits>>,
    busy: bool,
}

impl Sidebar {
    /// The trash button of a completed row (`openWorkspaceDeletionDialog`).
    pub(in crate::screens::sidebar) fn open_delete_task(
        &mut self,
        task_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.state.task(task_id) else {
            return;
        };
        let bundle_id = task.bundle_id.clone();
        self.dialog = Some(SidebarDialog::DeleteTask(DeleteTaskDialog {
            task_id: task_id.to_string(),
            bundle_id: bundle_id.clone(),
            title: task.display_title(),
            targets: None,
            delete_remote: false,
            unapplied: None,
            busy: false,
        }));
        cx.notify();
        let id = task_id.to_string();
        let db_path = loader::database_path();
        cx.spawn(async move |this, cx| {
            let lookup = id.clone();
            let targets =
                cx.background_executor()
                    .spawn(async move {
                        deletion::remote_targets(&db_path, &lookup, bundle_id.as_deref())
                    })
                    .await;
            let _ = this.update(cx, |sidebar, cx| {
                if let Some(SidebarDialog::DeleteTask(dialog)) = sidebar.dialog.as_mut()
                    && dialog.task_id == id
                {
                    dialog.targets = Some(targets);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn delete_task_dialog(&mut self) -> Option<&mut DeleteTaskDialog> {
        match self.dialog.as_mut()? {
            SidebarDialog::DeleteTask(dialog) => Some(dialog),
            _ => None,
        }
    }

    fn confirm_delete_task(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.delete_task_dialog() else {
            return;
        };
        if dialog.busy {
            return;
        }
        dialog.busy = true;
        let request = DeleteTask {
            task_id: dialog.task_id.clone(),
            bundle_id: dialog.bundle_id.clone(),
            delete_remote_branches: dialog.delete_remote,
            remote_targets: dialog.targets.clone().unwrap_or_default(),
            // Only after the dialog listed them and the person confirmed again.
            discard_child_edits: dialog.unapplied.is_some(),
        };
        cx.notify();
        let db_path = loader::database_path();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { deletion::delete_task(&db_path, &request) })
                .await;
            let _ = this.update(cx, |sidebar, cx| sidebar.finish_delete_task(result, cx));
        })
        .detach();
    }

    fn finish_delete_task(&mut self, result: Result<(), DeleteError>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.dialog = None;
                self.reload_now(cx);
            }
            Err(DeleteError::UnappliedEdits(list)) => {
                // Nothing was removed: keep the dialog and ask about the edits.
                if let Some(dialog) = self.delete_task_dialog() {
                    dialog.busy = false;
                    dialog.unapplied = Some(list);
                }
            }
            Err(error) => {
                if let Some(message) = refusal_message(&error, cx) {
                    // Refused before anything changed: the dialog stays.
                    if let Some(dialog) = self.delete_task_dialog() {
                        dialog.busy = false;
                    }
                    Toaster::error(message, cx);
                } else if let DeleteError::Failed(message) = error {
                    eprintln!("[dcc-native] delete task failed: {message}");
                    let text = if message.contains("Undo recovery is still active") {
                        cx.t("sidebar.deleteWorkspaceUndoRecoveryActive")
                    } else if message.contains(
                        dcc_infra::workspace_deletion::delegation::UNAPPLIED_DELEGATION_EDITS,
                    ) {
                        cx.t("sidebar.deleteWorkspaceUnappliedEditsBlocked")
                    } else if message.is_empty() {
                        cx.t("sidebar.deleteWorkspaceError")
                    } else {
                        message.into()
                    };
                    Toaster::error(text, cx);
                    // The backend may already have removed a branch or a
                    // worktree: the next confirmation starts from fresh data.
                    self.dialog = None;
                    self.reload_now(cx);
                }
            }
        }
        cx.notify();
    }
}

fn disk_line(
    sidebar: &Sidebar,
    dialog: &DeleteTaskDialog,
    cx: &mut Context<Sidebar>,
) -> Option<AnyElement> {
    let task = sidebar.state.task(&dialog.task_id)?;
    let scan = sidebar.completed_scan.output.as_ref();
    if sidebar.completed_scan_loading() {
        let muted = cx.theme().colors.muted_foreground;
        return Some(
            fact_box(cx)
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_color(muted)
                .child(spinner("delete-disk-loading", 14.0))
                .child(cx.t("sidebar.calculatingWorkspaceDiskUsage"))
                .into_any_element(),
        );
    }
    let bytes = task_bytes(task, &scan?.workspaces)?;
    let locale = cx.global::<I18n>().locale();
    let size = format_disk_bytes(bytes, locale);
    Some(
        fact_box(cx)
            .child(cx.t_with(
                "sidebar.workspaceDiskSpaceFreed",
                &Args::default().with("size", size),
            ))
            .into_any_element(),
    )
}

fn remote_block(dialog: &DeleteTaskDialog, cx: &mut Context<Sidebar>) -> Option<AnyElement> {
    let targets = dialog.targets.as_ref()?;
    if targets.is_empty() {
        return Some(
            fact_box(cx)
                .child(cx.t("sidebar.noRemoteBranchToDelete"))
                .into_any_element(),
        );
    }
    let mut names: Vec<String> = Vec::new();
    for target in targets {
        let name = format!("{}/{}", target.remote, target.branch);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let colors = &cx.theme().colors;
    let checked = dialog.delete_remote;
    let label = if dialog.bundle_id.is_some() {
        cx.t("sidebar.deleteRemoteBranches")
    } else {
        cx.t("sidebar.deleteRemoteBranch")
    };
    let mono = cx.theme().fonts.mono.clone();
    Some(
        fact_box(cx)
            .id("delete-remote-branch")
            .flex()
            .items_start()
            .gap(px(10.0))
            .cursor_pointer()
            .when(!dialog.busy, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(dialog) = this.delete_task_dialog() {
                        dialog.delete_remote = !checked;
                        cx.notify();
                    }
                }))
            })
            .child(
                div()
                    .mt(px(2.0))
                    .child(Checkbox::new("delete-remote-check", checked).disabled(dialog.busy)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(label),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .truncate()
                            .font_family(mono)
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .child(SharedString::from(names.join(", "))),
                    ),
            )
            .into_any_element(),
    )
}

fn unapplied_block(list: &[UnappliedDelegationEdits], cx: &mut Context<Sidebar>) -> AnyElement {
    let colors = &cx.theme().colors;
    let muted = colors.muted_foreground;
    let mut rows = Vec::new();
    for child in list {
        rows.push(
            div()
                .flex()
                .min_w_0()
                .items_baseline()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(child.title.clone())),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .font_features(tabular_nums())
                        .child(cx.t_with(
                            "sidebar.deleteWorkspaceUnappliedEditsFiles",
                            &Args::count(child.files as i64),
                        )),
                ),
        );
    }
    amber_box(cx)
        .child(
            div()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(cx.t("sidebar.deleteWorkspaceUnappliedEditsTitle")),
        )
        .child(
            div()
                .mt(px(6.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .text_color(muted)
                .children(rows),
        )
        .child(
            div()
                .mt(px(8.0))
                .text_color(muted)
                .child(cx.t("sidebar.deleteWorkspaceUnappliedEdits")),
        )
        .into_any_element()
}

pub fn render(
    sidebar: &Sidebar,
    dialog: &DeleteTaskDialog,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let busy = dialog.busy;
    let title = cx.t_with(
        "sidebar.deleteWorkspaceTitle",
        &Args::default().with("label", dialog.title.as_str()),
    );
    let header = dialog_header(title, Some(cx.t("sidebar.deleteWorkspaceDescription")), cx);
    let disk = disk_line(sidebar, dialog, cx);
    let remote = remote_block(dialog, cx);
    let unapplied = dialog
        .unapplied
        .as_deref()
        .map(|list| unapplied_block(list, cx));
    let confirm_label = if busy {
        cx.t("sidebar.deletingWorkspace")
    } else if dialog.unapplied.is_some() {
        cx.t("sidebar.deleteWorkspaceDiscardEdits")
    } else {
        cx.t("sidebar.deleteWorkspaceConfirm")
    };
    let footer = dialog_footer(
        [
            Button::new("delete-task-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .disabled(busy)
                .child(cx.t("sidebar.cancel"))
                .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx)))
                .into_any_element(),
            Button::new("delete-task-confirm")
                .variant(ButtonVariant::Destructive)
                .size(ButtonSize::Default)
                .disabled(busy)
                .when(busy, |button| {
                    button.child(spinner("delete-task-busy", 14.0))
                })
                .child(confirm_label)
                .on_click(cx.listener(|this, _, _, cx| this.confirm_delete_task(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Dialog::new("delete-task-dialog", move |_: &mut Window, cx| {
        let _ = close.update(cx, |sidebar, cx| sidebar.close_dialog(cx));
    })
    .max_width(448.0)
    .close_button(!busy)
    .child(header)
    .children(disk)
    .children(remote)
    .children(unapplied)
    .child(footer)
    .into_any_element()
}
