//! "Remover projeto": drops a project and its tasks from DCC; the folder on
//! disk stays.

use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

use super::{SidebarDialog, refusal_message};
use crate::data::deletion::{self, DeleteError};
use crate::data::loader;
use crate::data::tasks::Project;
use crate::i18n::{Args, Translate};
use crate::screens::sidebar::Sidebar;
use crate::theme::ActiveTheme;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Toaster, dialog_footer, dialog_header, spinner,
};

pub struct RemoveProjectDialog {
    repository_id: String,
    label: String,
    root_path: String,
    task_count: usize,
    busy: bool,
}

impl Sidebar {
    /// `openProjectRemovalDialog`: the project and how many tasks go with it.
    pub(in crate::screens::sidebar) fn open_remove_project(
        &mut self,
        project: &Project,
        cx: &mut Context<Self>,
    ) {
        let root = project.root_path.trim();
        let task_count = self
            .state
            .data
            .tasks
            .iter()
            .filter(|task| task.project_key() == root)
            .count();
        self.dialog = Some(SidebarDialog::RemoveProject(RemoveProjectDialog {
            repository_id: project.id.clone(),
            label: project.display_name.clone(),
            root_path: root.to_string(),
            task_count,
            busy: false,
        }));
        cx.notify();
    }

    /// The ✕ on a project in "Nova tarefa".
    pub fn open_remove_project_by_id(&mut self, repository_id: &str, cx: &mut Context<Self>) {
        let project = self
            .state
            .data
            .projects
            .iter()
            .find(|project| project.id == repository_id)
            .cloned();
        if let Some(project) = project {
            self.open_remove_project(&project, cx);
        }
    }

    #[cfg(test)]
    pub(crate) fn removing_project(&self) -> Option<&str> {
        match self.dialog.as_ref()? {
            SidebarDialog::RemoveProject(dialog) => Some(dialog.repository_id.as_str()),
            _ => None,
        }
    }

    fn remove_project_dialog(&mut self) -> Option<&mut RemoveProjectDialog> {
        match self.dialog.as_mut()? {
            SidebarDialog::RemoveProject(dialog) => Some(dialog),
            _ => None,
        }
    }

    fn confirm_remove_project(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.remove_project_dialog() else {
            return;
        };
        if dialog.busy {
            return;
        }
        dialog.busy = true;
        let repository_id = dialog.repository_id.clone();
        cx.notify();
        let db_path = loader::database_path();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { deletion::remove_project(&db_path, &repository_id) })
                .await;
            let _ = this.update(cx, |sidebar, cx| sidebar.finish_remove_project(result, cx));
        })
        .detach();
    }

    fn finish_remove_project(&mut self, result: Result<(), DeleteError>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.dialog = None;
                self.reload_now(cx);
            }
            Err(error) => {
                if let Some(dialog) = self.remove_project_dialog() {
                    dialog.busy = false;
                }
                let message = refusal_message(&error, cx).unwrap_or_else(|| match error {
                    DeleteError::Failed(message) if !message.is_empty() => message.into(),
                    _ => cx.t("sidebar.removeProjectError"),
                });
                Toaster::error(message, cx);
            }
        }
        cx.notify();
    }
}

pub fn render(dialog: &RemoveProjectDialog, cx: &mut Context<Sidebar>) -> AnyElement {
    let busy = dialog.busy;
    let colors = cx.theme().colors.clone();
    let radius = cx.theme().radius;
    let mono = cx.theme().fonts.mono.clone();
    let header = dialog_header(
        cx.t_with(
            "sidebar.removeProjectTitle",
            &Args::default().with("label", dialog.label.as_str()),
        ),
        Some(cx.t_with(
            "sidebar.removeProjectDescription",
            &Args::count(dialog.task_count as i64),
        )),
        cx,
    );
    let path = (!dialog.root_path.is_empty()).then(|| {
        div()
            .rounded(radius.lg)
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.muted.opacity(0.3))
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(12.0))
            .line_height(px(16.0))
            .text_color(colors.muted_foreground)
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(cx.t("sidebar.removeProjectPathLabel")),
                    )
                    // `break-all`: a long path wraps inside the box.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(mono)
                            .child(SharedString::from(dialog.root_path.clone())),
                    ),
            )
    });
    let warning = div()
        .text_size(px(12.0))
        .line_height(px(20.0))
        .text_color(colors.muted_foreground)
        .child(cx.t("sidebar.removeProjectWarning"));
    let footer = dialog_footer(
        [
            Button::new("remove-project-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .disabled(busy)
                .child(cx.t("sidebar.cancel"))
                .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx)))
                .into_any_element(),
            Button::new("remove-project-confirm")
                .variant(ButtonVariant::Destructive)
                .size(ButtonSize::Default)
                .disabled(busy)
                .when(busy, |button| {
                    button.child(spinner("remove-project-busy", 14.0))
                })
                .child(if busy {
                    cx.t("sidebar.removeProjectConfirming")
                } else {
                    cx.t("sidebar.removeProjectConfirm")
                })
                .on_click(cx.listener(|this, _, _, cx| this.confirm_remove_project(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Dialog::new("remove-project-dialog", move |_: &mut Window, cx| {
        let _ = close.update(cx, |sidebar, cx| sidebar.close_dialog(cx));
    })
    .close_button(!busy)
    .child(header)
    .children(path)
    .child(warning)
    .child(footer)
    .into_any_element()
}
