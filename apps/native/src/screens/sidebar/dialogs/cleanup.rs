//! `CompletedCleanupDialog`: deletes completed tasks in one go. Each worktree
//! is measured first; only what is committed and published comes checked,
//! and the rest says what deleting it would lose.

use std::collections::HashSet;

use dcc_infra::workspace_cleanup::{WorkspaceCleanupEntry, WorkspaceCleanupSafety};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

use super::SidebarDialog;
use crate::data::deletion::{self, DeleteError, DeleteTask};
use crate::data::disk::{CleanupRow, cleanup_rows, disk_usage_ids, format_disk_bytes, preselected};
use crate::data::loader;
use crate::i18n::{Args, I18n, Translate};
use crate::screens::sidebar::Sidebar;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Checkbox, Dialog, Toaster, dialog_footer_split,
    dialog_header, spinner, tabular_nums,
};

enum Scan {
    Loading,
    Failed,
    Ready { entries: Vec<WorkspaceCleanupEntry> },
}

pub struct CleanupDialog {
    scan: Scan,
    selected: HashSet<String>,
    /// `(done, total)` while deleting.
    progress: Option<(usize, usize)>,
}

impl Sidebar {
    /// The brush on the "Concluídos" header. Scans once per opening.
    pub(in crate::screens::sidebar) fn open_cleanup(&mut self, cx: &mut Context<Self>) {
        self.dialog = Some(SidebarDialog::Cleanup(CleanupDialog {
            scan: Scan::Loading,
            selected: HashSet::new(),
            progress: None,
        }));
        cx.notify();
        let completed = self.completed_tasks();
        let ids = disk_usage_ids(&completed.iter().collect::<Vec<_>>());
        let db_path = loader::database_path();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { deletion::cleanup_scan(&db_path, ids) })
                .await;
            let _ = this.update(cx, |sidebar, cx| {
                let completed = sidebar.completed_tasks();
                let Some(dialog) = sidebar.cleanup_dialog() else {
                    return;
                };
                match result {
                    Ok(output) => {
                        let tasks: Vec<_> = completed.iter().collect();
                        dialog.selected = cleanup_rows(&tasks, &output.workspaces)
                            .into_iter()
                            .filter(preselected)
                            .map(|row| row.task_id)
                            .collect();
                        dialog.scan = Scan::Ready {
                            entries: output.workspaces,
                        };
                    }
                    Err(error) => {
                        eprintln!("[dcc-native] cleanup scan: {error}");
                        dialog.scan = Scan::Failed;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn cleanup_dialog(&mut self) -> Option<&mut CleanupDialog> {
        match self.dialog.as_mut()? {
            SidebarDialog::Cleanup(dialog) => Some(dialog),
            _ => None,
        }
    }

    fn cleanup_rows(&self, entries: &[WorkspaceCleanupEntry]) -> Vec<CleanupRow> {
        let completed = self.completed_tasks();
        cleanup_rows(&completed.iter().collect::<Vec<_>>(), entries)
    }

    fn toggle_cleanup_row(&mut self, task_id: &str, cx: &mut Context<Self>) {
        if let Some(dialog) = self.cleanup_dialog()
            && dialog.progress.is_none()
            && !dialog.selected.remove(task_id)
        {
            dialog.selected.insert(task_id.to_string());
        }
        cx.notify();
    }

    /// Deletes the checked tasks one at a time, carrying on past a failure.
    fn confirm_cleanup(&mut self, cx: &mut Context<Self>) {
        let entries = match self.dialog.as_ref() {
            Some(SidebarDialog::Cleanup(CleanupDialog {
                scan: Scan::Ready { entries },
                progress: None,
                ..
            })) => entries.clone(),
            _ => return,
        };
        let rows = self.cleanup_rows(&entries);
        let Some(dialog) = self.cleanup_dialog() else {
            return;
        };
        let targets: Vec<(String, u64)> = rows
            .iter()
            .filter(|row| dialog.selected.contains(&row.task_id))
            .map(|row| (row.task_id.clone(), row.bytes))
            .collect();
        if targets.is_empty() {
            return;
        }
        dialog.progress = Some((0, targets.len()));
        cx.notify();
        let requests: Vec<(DeleteTask, u64)> = targets
            .into_iter()
            .map(|(task_id, bytes)| {
                let bundle_id = self
                    .state
                    .task(&task_id)
                    .and_then(|task| task.bundle_id.clone());
                (
                    DeleteTask {
                        task_id,
                        bundle_id,
                        ..DeleteTask::default()
                    },
                    bytes,
                )
            })
            .collect();
        let db_path = loader::database_path();
        cx.spawn(async move |this, cx| {
            let total = requests.len();
            let mut freed = 0;
            let mut failed = 0;
            let mut desktop_open = false;
            for (index, (request, bytes)) in requests.into_iter().enumerate() {
                let path = db_path.clone();
                let task_id = request.task_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { deletion::delete_task(&path, &request) })
                    .await;
                match result {
                    Ok(()) => freed += bytes,
                    Err(DeleteError::DesktopOpen) => {
                        desktop_open = true;
                        break;
                    }
                    Err(error) => {
                        failed += 1;
                        eprintln!("[dcc-native] cleanup could not delete {task_id}: {error:?}");
                    }
                }
                let done = index + 1;
                let alive = this.update(cx, |sidebar, cx| {
                    if let Some(dialog) = sidebar.cleanup_dialog() {
                        dialog.progress = Some((done, total));
                        dialog.selected.remove(&task_id);
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
            let _ = this.update(cx, |sidebar, cx| {
                sidebar.reload_now(cx);
                if let Some(dialog) = sidebar.cleanup_dialog() {
                    dialog.progress = None;
                }
                if desktop_open {
                    Toaster::error(cx.t("native.desktopOpen"), cx);
                } else if failed > 0 {
                    Toaster::error(
                        cx.t_with("sidebar.cleanup.partial", &Args::count(failed)),
                        cx,
                    );
                } else {
                    let size = format_disk_bytes(freed, cx.global::<I18n>().locale());
                    Toaster::success(
                        cx.t_with("sidebar.cleanup.freed", &Args::default().with("size", size)),
                        cx,
                    );
                    sidebar.dialog = None;
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn reason(row: &CleanupRow, branch: &str, cx: &Context<Sidebar>) -> SharedString {
    match row.safety {
        WorkspaceCleanupSafety::Uncommitted => cx.t_with(
            "sidebar.cleanup.safety.uncommitted",
            &Args::count(row.changed_files as i64),
        ),
        WorkspaceCleanupSafety::Unpushed => cx.t_with(
            "sidebar.cleanup.safety.unpushed",
            &Args::count(row.unpushed_commits as i64).with("branch", branch),
        ),
        WorkspaceCleanupSafety::Safe => cx.t("sidebar.cleanup.safety.safe"),
        WorkspaceCleanupSafety::NoWorktree => cx.t("sidebar.cleanup.safety.noWorktree"),
        WorkspaceCleanupSafety::Unknown => cx.t("sidebar.cleanup.safety.unknown"),
    }
}

fn row_element(
    sidebar: &Sidebar,
    row: &CleanupRow,
    checked: bool,
    busy: bool,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let task = sidebar.state.task(&row.task_id);
    let title = task.map(|task| task.display_title()).unwrap_or_default();
    let branch = task.map(|task| task.branch.clone()).unwrap_or_default();
    let project = task
        .and_then(|task| sidebar.project_for(task))
        .map(|project| project.display_name.clone());
    let mut line = reason(row, &branch, cx).to_string();
    if let Some(project) = project {
        line = format!("{line} · {project}");
    }
    let risky = matches!(
        row.safety,
        WorkspaceCleanupSafety::Uncommitted | WorkspaceCleanupSafety::Unpushed
    );
    let colors = &cx.theme().colors;
    let amber = match cx.theme().appearance {
        Appearance::Light => tw::AMBER_700.hsla(),
        Appearance::Dark => tw::AMBER_300.hsla(),
    };
    let hover = colors.accent.opacity(0.4);
    let size = format_disk_bytes(row.bytes, cx.global::<I18n>().locale());
    let id = row.task_id.clone();
    div()
        .id(SharedString::from(format!("cleanup-{}", row.task_id)))
        .flex()
        .min_w_0()
        .items_start()
        .gap(px(10.0))
        .px(px(12.0))
        .py(px(10.0))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(cx.listener(move |this, _, _, cx| this.toggle_cleanup_row(&id, cx)))
        .child(
            div().mt(px(2.0)).child(
                Checkbox::new(
                    SharedString::from(format!("cleanup-check-{}", row.task_id)),
                    checked,
                )
                .disabled(busy),
            ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(SharedString::from(title)),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(if risky {
                            amber
                        } else {
                            colors.muted_foreground
                        })
                        .child(SharedString::from(line)),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .pt(px(2.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_features(tabular_nums())
                .text_color(colors.muted_foreground)
                .child(SharedString::from(size)),
        )
        .into_any_element()
}

pub fn render(
    sidebar: &Sidebar,
    dialog: &CleanupDialog,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let busy = dialog.progress.is_some();
    let colors = cx.theme().colors.clone();
    let radius = cx.theme().radius;
    let header = dialog_header(
        cx.t("sidebar.cleanup.title"),
        Some(cx.t("sidebar.cleanup.description")),
        cx,
    );
    let message = |text: SharedString, color| {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .py(px(24.0))
            .text_size(px(14.0))
            .text_color(color)
            .child(text)
    };
    let mut rows = Vec::new();
    let body = match &dialog.scan {
        Scan::Loading => div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .py(px(24.0))
            .text_size(px(14.0))
            .text_color(colors.muted_foreground)
            .child(spinner("cleanup-scanning", 14.0))
            .child(cx.t("sidebar.cleanup.scanning"))
            .into_any_element(),
        Scan::Failed => {
            message(cx.t("sidebar.cleanup.scanFailed"), colors.destructive).into_any_element()
        }
        Scan::Ready { entries } => {
            rows = sidebar.cleanup_rows(entries);
            if rows.is_empty() {
                message(cx.t("sidebar.cleanup.empty"), colors.muted_foreground).into_any_element()
            } else {
                let mut list = Vec::new();
                for (index, row) in rows.iter().enumerate() {
                    let checked = dialog.selected.contains(&row.task_id);
                    let element = row_element(sidebar, row, checked, busy, cx);
                    list.push(
                        div()
                            .when(index > 0, |item| {
                                item.border_t_1().border_color(colors.border.opacity(0.6))
                            })
                            .child(element),
                    );
                }
                div()
                    .id("cleanup-list")
                    .mx(px(-4.0))
                    .max_h(window.viewport_size().height * 0.5)
                    .min_w_0()
                    .overflow_y_scroll()
                    .rounded(radius.lg)
                    .border_1()
                    .border_color(colors.border.opacity(0.7))
                    .children(list)
                    .into_any_element()
            }
        }
    };
    let selected: Vec<&CleanupRow> = rows
        .iter()
        .filter(|row| dialog.selected.contains(&row.task_id))
        .collect();
    let selected_bytes: u64 = selected.iter().map(|row| row.bytes).sum();
    let locale = cx.global::<I18n>().locale();
    let note = div()
        .text_size(px(12.0))
        .font_features(tabular_nums())
        .text_color(colors.muted_foreground)
        .when(!rows.is_empty(), |note| {
            note.child(
                cx.t_with(
                    "sidebar.cleanup.selected",
                    &Args::count(selected.len() as i64)
                        .with("size", format_disk_bytes(selected_bytes, locale)),
                ),
            )
        })
        .into_any_element();
    let confirm = match dialog.progress {
        Some((done, total)) => cx.t_with(
            "sidebar.cleanup.deleting",
            &Args::default()
                .with("done", done.to_string())
                .with("total", total.to_string()),
        ),
        None => cx.t_with(
            "sidebar.cleanup.confirm",
            &Args::count(selected.len() as i64),
        ),
    };
    let footer = dialog_footer_split(
        note,
        [
            Button::new("cleanup-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .disabled(busy)
                .child(cx.t("sidebar.cancel"))
                .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx)))
                .into_any_element(),
            Button::new("cleanup-confirm")
                .variant(ButtonVariant::Destructive)
                .size(ButtonSize::Default)
                .disabled(busy || selected.is_empty())
                .when(busy, |button| button.child(spinner("cleanup-busy", 14.0)))
                .child(confirm)
                .on_click(cx.listener(|this, _, _, cx| this.confirm_cleanup(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Dialog::new("cleanup-dialog", move |_: &mut Window, cx| {
        let _ = close.update(cx, |sidebar, cx| sidebar.close_dialog(cx));
    })
    .max_width(512.0)
    .close_button(!busy)
    .child(header)
    .child(body)
    .child(footer)
    .into_any_element()
}
